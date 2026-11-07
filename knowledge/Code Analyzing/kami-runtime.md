# Kami Runtime (`kami-runtime`)

Analysis Baseline: Git branch `protocol-storage`, HEAD commit `0de16c9` ("Merge KAMI-25 into main") with uncommitted KAMI-26 working-tree implementation.

---

## 1. Overview & Purpose

`kami-runtime` is the bounded Tokio orchestration library for Kami (KAMI-26). It composes the existing synchronous KAMI-24 sender, KAMI-25 receiver/receipt coordinator, SQLCipher persistence engine, and injected key-provider contracts into a supervised asynchronous runtime.

> [!IMPORTANT]
> `kami-runtime` is a **reusable runtime library**, not a standalone daemon or complete messaging service. It contains **no live P2P network transport** (deferred to KAMI-27), **no functioning relay network** (deferred to KAMI-29/30), **no local CLI/IPC server** (deferred to KAMI-32), and **no production authenticated end-peer producer**.

```
                           +-------------------------------------+
                           |            kami-runtime             |
                           +-------------------------------------+
                                              |
                     +------------------------+------------------------+
                     |                                                 |
                     v                                                 v
        +-------------------------+                       +-------------------------+
        |   Single-Owner Engine   |                       |    Dedicated Worker     |
        |   (Tokio Async Task)    |                       |    (spawn_blocking)     |
        +-------------------------+                       +-------------------------+
         - Lifecycle state machine                         - OS Profile Lease (Lock)
         - Bounded command queue (32)                      - KeyProvider Contract
         - Bounded event stream (32)                       - SQLCipher Transactions
         - Supervised Relay JoinSet                        - kami-sender Coordinator
         - Coalesced retry scanner                         - kami-receiver Coordinator
         - Watch snapshot broadcaster                      - Durable Reconciliation
```

---

## 2. Core Architecture & Responsibilities

The runtime enforces strict boundaries between asynchronous control flow and synchronous blocking storage operations:

### 2.1 Single-Owner Tokio Engine (`src/engine.rs`)
- Mutable lifecycle state is owned exclusively by **one** central Tokio event loop task (`run`).
- External callers communicate via bounded Tokio `mpsc` and `watch` channels; child tasks never mutate engine state directly.
- Uses `tokio::select!` with a `biased` strategy: shutdown cancellation signals take absolute precedence over new incoming commands.

### 2.2 Dedicated Blocking Worker (`src/worker.rs`)
- Storage initialization, SQLCipher transactions, key custody, and sender/receiver composition execute sequentially on **one persistent `tokio::task::spawn_blocking` thread**.
- **No SQLite transaction, key-provider borrow, or Double Ratchet session crosses an async `.await`**.
- This guarantees that slow disk I/O or key derivation cannot starve the Tokio event loop, block relay renewal timers, or delay graceful shutdown.

### 2.3 Cross-Process Profile Ownership
- Profile directory ownership reuses `kami_storage::ProfileLease`.
- Authoritative protection is provided by the OS kernel advisory lock (`flock` on Unix).
- Diagnostic owner metadata (`record_owner`) provides visibility but cannot steal a live profile.
- **Lease Retention**: The profile lock remains held until the blocking worker thread completes. A shutdown timeout reports `ShutdownOutcome::TimedOut`, but does **not** terminate active kernel I/O or release the lease prematurely.

### 2.4 Bounded Channels & Backpressure
- **Commands**: Handled via `mpsc::channel(config.command_capacity)` (default: 32). When full, commands return `Error::Busy`.
- **Events**: Handled via `mpsc::channel(config.event_capacity)` (default: 32). Diagnostic events are **best-effort**: if the consumer channel is full, events are dropped, and `dropped_events` counter increments. Snapshots (`watch`) and command replies (`oneshot`) are authoritative.
- **Blocking Operations**: Bounded by `config.blocking_capacity` (default: 8, including the currently executing job). When full, new operations are rejected with `Error::Busy`.

### 2.5 Supervised Relay Seam (`src/relay.rs`)
- Transport-neutral scheduler managing reservation lifetimes (`Reserve`, `Renew`, `Release`).
- Supervised inside a `tokio::task::JoinSet`.
- Enforces operation timeouts, renewal margins, exponential retry backoff, and graceful release upon shutdown.
- **Scheduling Only**: Returns relative reservation durations; it does not implement relay networking, frame relaying, or message delivery.

---

## 3. Public API & Types

Defined in [`crates/kami-runtime/src/lib.rs`](file:///home/phu/dev/kami/crates/kami-runtime/src/lib.rs) and [`crates/kami-runtime/src/engine.rs`](file:///home/phu/dev/kami/crates/kami-runtime/src/engine.rs):

### 3.1 `Profile`
```rust
pub struct Profile {
    pub directory: PathBuf,
    pub key_id: ProfileKeyId,
    pub key_provider: Arc<dyn KeyProvider>,
    pub create: bool,
    pub storage: kami_storage::Config,
}
```
Requires explicit choice between opening an existing profile or creating a new one (`create: bool`). Silently creating an empty database when open fails is forbidden.

### 3.2 `Config`
```rust
pub struct Config {
    pub command_capacity: usize,    // default: 32 (bounds: 1..=1024)
    pub event_capacity: usize,      // default: 32 (bounds: 1..=1024)
    pub blocking_capacity: usize,   // default: 8 (bounds: 1..=1024)
    pub shutdown_timeout: Duration, // default: 10s (max: 1 hour)
    pub retry_scan_interval: Duration, // default: 1s (bounds: 10ms..=1 hour)
}
```

### 3.3 Lifecycle State Machine
```rust
pub enum Lifecycle {
    Starting,     // Spawning blocking worker, acquiring OS lease
    Reconciling,  // Scanning durable state for uncommitted/interrupted sends
    Running,      // Fully operational, accepting commands
    Draining,     // Shutting down, draining queues, releasing relay
    Stopped,      // Cleanly terminated, storage closed, lease released
    Failed,       // Fatal error (worker panic or storage failure)
}
```

### 3.4 Key Engine Methods
- `Engine::start(profile, config) -> Result<(Engine, mpsc::Receiver<Event>), Error>`: Launches the engine inside an active Tokio context.
- `Engine::start_with_relay(profile, config, relay_config, adapter)`: Starts engine with supervised relay renewal scheduler.
- `engine.wait_running().await -> Result<(), Error>`: Awaits successful transition from `Starting`/`Reconciling` to `Running`.
- `engine.health().await -> Result<Lifecycle, Error>`: Authoritative check across command queues.
- `engine.snapshot() -> Snapshot`: Non-blocking, instant snapshot of current engine metrics.
- `engine.recovered().await -> Result<usize, Error>`: Returns count of reconciled messages from durable storage.
- `engine.cancel(sender, id).await -> Result<(), Error>`: Requests durable cancellation of an outbound message attempt.
- `engine.enqueue(request).await -> Result<(), Error>`: **Fails closed** with `Error::LiveTransportUnavailable`. The runtime refuses unauthenticated enqueue submissions without verified live transport provenance.
- `engine.request_shutdown()`: Out-of-band shutdown signal that bypasses full command queues.
- `engine.shutdown().await -> ShutdownOutcome`: Coordinates graceful drain within `shutdown_timeout`. Returns `Clean`, `TimedOut`, or `Failed`.
- `engine.wait_stopped().await`: Awaits full worker and child task termination (may wait past timeout if blocking kernel I/O is held).

---

## 4. Ingress Boundary & Private Mock Seam (`src/ingress.rs`)

`src/ingress.rs` houses the private seam connecting the runtime to KAMI-24 `Coordinator` and KAMI-25 `Receiver`:

- **Private Capability Token**: `AuthenticatedPeer`
  - Encapsulates verified `PeerId`, local/remote `DeviceId`, `ConversationId`, monotonic trust generation, and valid epoch.
  - Has private fields, no public constructor, no `Default`, and no serde serialization.
  - Can only be minted by internal integration test harnesses (`#[cfg(test)]`).
- **Why Caller-Supplied Data is Rejected**:
  - Caller-supplied `PeerId` strings, DTOs, or database generations do not prove live cryptographic connection authority.
  - Real live provenance requires KAMI-27 (authenticated rust-libp2p stream boundary).
  - Consequently, `Engine::enqueue` returns `Error::LiveTransportUnavailable` in production.

---

## 5. Test Harness Correction (TMPDIR Independence)

During KAMI-26 development and evaluator verification, an environmental dependency was identified and resolved:

- **Defect**: Unit tests previously relied on `std::env::temp_dir()`, which defaults to `/tmp` via `TMPDIR`. In containerized, sandboxed, or restrictive multi-user environments with a read-only root or missing `TMPDIR`, tests panicked with OS error 30 (`Read-only file system`).
- **Correction**: Test harnesses now construct private scoped temporary directories with strict `0700` POSIX permissions located within `HOME`, `USERPROFILE`, or the local working directory:
  ```rust
  let base = std::env::var_os("HOME")
      .or_else(|| std::env::var_os("USERPROFILE"))
      .map(PathBuf::from)
      .or_else(|| std::env::current_dir().ok())
      .expect("usable private test-directory base");
  tempfile::Builder::new().prefix("kami-runtime-").tempdir_in(base).unwrap();
  ```
- **Verification Command**:
  ```sh
  env -u TMPDIR cargo test --locked -p kami-runtime
  ```

---

## 6. Verification Evidence & Quality Profile

Verified on Linux (`protocol-storage` branch):
- **Focused Test Suite**: 21 passing tests (17 unit/integration tests, 2 cross-process ownership tests, 1 privacy probe, 1 real SQLCipher reopen test).
- **Clippy**: Clean with `-D warnings`.
- **Formatting**: Verified via `cargo fmt --all -- --check`.
- **Ignored Tests**: Two pre-existing native secure-store tests in `kami-identity` remain ignored due to environment-dependent D-Bus/gnome-keyring prerequisites.

---

## 7. Current Limitations & Deferred Work

| Capability | Status | Description |
| :--- | :--- | :--- |
| Direct Transport | Deferred | Socket I/O, NAT traversal, and wire framing belong to KAMI-27. |
| Functioning Relay | Deferred | Relay network connection and packet routing belong to KAMI-29/30. |
| Local IPC / CLI | Deferred | Named pipes, Unix domain sockets, and daemon rendering belong to KAMI-32. |
| Production Ingress | Deferred | Public authenticated end-peer factory will be introduced with KAMI-27. |
| OS Worker Preemption | By Design | Kernel file I/O cannot be preempted; shutdown timeouts report status without killing blocking threads. |

---

## 8. Interactive Architecture Visualizations

- **KAMI-VIS-002**: [Rust Crate Dependency Graph](file:///home/phu/dev/kami/knowledge/Code%20Analyzing/Visualizations/01-System-Architecture/KAMI-VIS-002/crate-dependencies.html) — 10-crate dependency topology showing `kami-runtime` hosting service coordinators, storage persistence, and cryptographic modules.
- **KAMI-VIS-027**: [Supervised Tokio Runtime & Task Ownership](file:///home/phu/dev/kami/knowledge/Code%20Analyzing/Visualizations/01-System-Architecture/KAMI-VIS-027/runtime-supervision.html) — Dedicated visual model detailing `Engine` actor loop, bounded channels, dedicated single-thread blocking worker, profile lease retention across timeout, and supervised relay seam.

