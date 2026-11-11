# Kami Design Patterns, Concurrency Models, and Algorithms Inventory

Analysis Baseline: Git branch `protocol-storage`, HEAD commit `0de16c9` with uncommitted KAMI-26 working tree.

---

## Executive Summary Table

| Category | Pattern / Algorithm Name | Primary Role in Kami | Implementation Status | Source Crate(s) |
| :--- | :--- | :--- | :--- | :--- |
| **A. Architecture** | Hexagonal / Ports & Adapters | Decouples pure domain rules from I/O and engines | IMPLEMENTED AND VERIFIED | `kami-core`, `kami-storage`, `kami-key-provider` |
| **A. Architecture** | Reverse Dependency Boundary | Enforces E2EE purity; storage depends on crypto | IMPLEMENTED AND VERIFIED | `kami-storage` $\to$ `kami-e2ee` |
| **A. Architecture** | Type-State & Capability Pattern | Prevents unauthenticated writes at compile time | IMPLEMENTED AND VERIFIED | `kami-e2ee`, `kami-storage`, `kami-runtime` |
| **A. Architecture** | Strong Typing / Newtype Idiom | Eliminates ID confusion across domain entities | IMPLEMENTED AND VERIFIED | `kami-core`, `kami-protocol` |
| **A. Architecture** | Opaque Token Encapsulation | Bounds operational custody without leaking state | IMPLEMENTED AND VERIFIED | `kami-storage`, `kami-sender` |
| **B. Concurrency** | Single-Owner Actor Loop | Centralizes mutable runtime state in one task | IMPLEMENTED AND VERIFIED | `kami-runtime`, `kami-storage` |
| **B. Concurrency** | Supervisor & Child Task Pattern | Supervises background tasks and isolates crashes | IMPLEMENTED AND VERIFIED | `kami-runtime` |
| **B. Concurrency** | Bounded Channel Passing | Enforces backpressure; prevents unbounded memory | IMPLEMENTED AND VERIFIED | `kami-runtime` |
| **B. Concurrency** | Dedicated Blocking Offloading | Protects async Tokio loop from slow disk/crypto | IMPLEMENTED AND VERIFIED | `kami-runtime`, `kami-storage` |
| **B. Concurrency** | Finite State Machine (FSM) | Governs message, contact trust, and engine stages | IMPLEMENTED AND VERIFIED | `kami-core`, `kami-storage`, `kami-runtime` |
| **B. Concurrency** | Dual-Channel Telemetry | Authoritative watch snapshots vs. best-effort drops | IMPLEMENTED AND VERIFIED | `kami-runtime` |
| **C. Persistence** | Write-Ahead Logging & Transactions | Guarantees atomic state updates and crash recovery | IMPLEMENTED AND VERIFIED | `kami-storage` |
| **C. Persistence** | Atomic Compare-And-Swap (CAS) | Prevents concurrent overwrite of trust states | IMPLEMENTED AND VERIFIED | `kami-storage`, `kami-identity` |
| **C. Persistence** | OS Advisory Directory Leasing | Prevents concurrent process profile corruption | IMPLEMENTED AND VERIFIED | `kami-storage`, `kami-runtime` |
| **C. Persistence** | Idempotent Crash Recovery | Re-emits exact ciphertext without re-encrypting | IMPLEMENTED AND VERIFIED | `kami-storage`, `kami-sender` |
| **C. Persistence** | Lane Fencing & Reservations | Serializes message ordering per conversation lane | IMPLEMENTED AND VERIFIED | `kami-storage`, `kami-sender` |
| **D. Security** | Double Ratchet Protocol | Guarantees forward secrecy and break-in recovery | IMPLEMENTED AND VERIFIED | `kami-e2ee` (Vodozemac Olm v1) |
| **D. Security** | Fail-Closed Authenticated Envelopes | Binds canonical facts, prevents plaintext leakage | IMPLEMENTED AND VERIFIED | `kami-e2ee`, `kami-protocol` |
| **D. Security** | Bilateral Trust & Key Pinning | Protects human identity with cryptographic proofs | IMPLEMENTED AND VERIFIED | `kami-identity`, `kami-storage` |
| **D. Security** | Keyed Request Binding | Fences message reservations with HMAC secrets | IMPLEMENTED AND VERIFIED | `kami-storage`, `kami-sender` |
| **D. Security** | Same-Session Receipt Staging | Binds delivery ACKs across 2 ratchet steps | IMPLEMENTED AND VERIFIED | `kami-e2ee`, `kami-receiver` |
| **E. Messaging** | Exact Wire-Frame Deduplication | Re-emits receipts without ratcheting on replays | IMPLEMENTED AND VERIFIED | `kami-storage`, `kami-receiver` |
| **E. Messaging** | Deterministic Exponential Backoff | Schedules transmission retries with 5m cap | IMPLEMENTED AND VERIFIED | `kami-storage`, `kami-sender` |
| **E. Messaging** | Presentation Ordering & Gap Aging | Waits 60s for missing frames before display | IMPLEMENTED AND VERIFIED | `kami-storage`, `kami-receiver` |
| **E. Messaging** | Terminal Attempt Lane Precedence | Genuine late receipts supersede canceled states | IMPLEMENTED AND VERIFIED | `kami-storage`, `kami-sender` |
| **E. Messaging** | Monotonic Sequence Allocation | Prevents out-of-order sequence assignment | IMPLEMENTED AND VERIFIED | `kami-core`, `kami-storage` |
| **F. Resilience** | Zeroization / Memory Scrubbing | Erases sensitive private keys and pickles on drop | IMPLEMENTED AND VERIFIED | `kami-e2ee`, `kami-storage`, `kami-key-provider` |
| **F. Resilience** | Bounded Input Pre-Scanning | Rejects oversized or malicious frames early | IMPLEMENTED AND VERIFIED | `kami-protocol` |
| **F. Resilience** | Fault Injection & Boundary Probes | Proves compiler encapsulation and privacy | IMPLEMENTED AND VERIFIED | `kami-e2ee`, `kami-storage`, `kami-runtime` |
| **F. Resilience** | Scoped Directory Isolation | Prevents cross-test interference without `/tmp` | IMPLEMENTED AND VERIFIED | `kami-runtime` |
| **G. Network & Direct Transport** | Revocable Linear Capability | Unforgeable connection handle with generation checks | IMPLEMENTED AND VERIFIED | `kami-transport` |
| **G. Network & Direct Transport** | Expected-Peer Swarm Dial Binding | Aborts connections if remote peer does not match expected ID | IMPLEMENTED AND VERIFIED | `kami-transport` |
| **D. Security** | Inner Plaintext Envelope Authentication | Authenticates inner envelope; outer SHA-256 is correspondence check | IMPLEMENTED AND VERIFIED | `kami-e2ee` (WP-A) |
| **F. Resilience** | Two-Process Interactive Qualification | Manual out-of-band fingerprint comparison and rejection verification | IMPLEMENTED AND VERIFIED | `kami-transport`, `kami-runtime` |

---

## Category A: Software Architecture and Composition Patterns

### 1. Hexagonal Architecture (Ports and Adapters)
- **Simple Explanation**: Core application logic is isolated at the center; external concerns (databases, keyrings, network protocols) interact strictly through abstract contracts ("ports") implemented by interchangeable components ("adapters").
- **Why Kami Uses It**: Kami must operate identically across diverse operating systems (Linux Secret Service, macOS Keychain, Windows Credential Manager) and storage backends without coupling domain logic to system libraries.
- **Source Evidence**:
  - `crates/kami-core`: Contains pure domain models (`MessageFacts`, `DeviceId`, `MessageId`) with zero I/O, network, or database dependencies.
  - `crates/kami-key-provider`: Defines the synchronous `KeyProvider` trait (port), implemented by platform key providers and test harnesses.
  - `crates/kami-runtime/src/relay.rs`: Defines the `RelayAdapter` trait (port) for pluggable relay reservation scheduling.
- **Implementation Mechanism**: Rust traits and workspace crate layering.
- **Relationships**: Works with Dependency Inversion and Finite State Machines.
- **Status**: IMPLEMENTED AND VERIFIED.
- **Tradeoffs / Limitations**: Requires boilerplate mapping between domain representations and external DTOs.

### 2. Reverse Dependency Architectural Pattern
- **Simple Explanation**: Storage depends on encryption; encryption never depends on storage.
- **Why Kami Uses It**: In conventional architectures, storage is at the bottom, and crypto sits on top. In Kami, `kami-storage` imports `kami-e2ee`. This ensures that crypto primitives remain completely pure and free of database logic, while storage owns transaction boundaries and enforces that only cryptographically verified data can be persisted.
- **Source Evidence**:
  - `crates/kami-storage/Cargo.toml`: Declares `kami-e2ee = { path = "../kami-e2ee" }`.
  - `crates/kami-e2ee/Cargo.toml`: Has zero references to `kami-storage`.
- **Implementation Mechanism**: Cargo dependency direction and private method visibility.
- **Relationships**: Pairs with the Type-State & Capability Pattern.
- **Status**: IMPLEMENTED AND VERIFIED.
- **Tradeoffs / Limitations**: Storage APIs cannot be directly invoked from within crypto session methods; coordination must be orchestrated by service layers (`kami-sender`, `kami-receiver`, `kami-runtime`).

### 3. Type-State and Capability Pattern
- **Simple Explanation**: The compiler enforces valid lifecycle transitions and access authorization by requiring callers to possess unforgeable, private-field token types.
- **Why Kami Uses It**: Eliminates unauthenticated database writes, replay vulnerabilities, and state forgery at compile time.
- **Source Evidence**:
  - `crates/kami-e2ee/src/authenticated.rs`: `AuthenticatedCommit` has private fields, no public constructor, and cannot be constructed from raw bytes.
  - `crates/kami-e2ee/src/authenticated/receipts.rs`: `AuthenticatedDelivery`, `AuthenticatedReceiptCommit`, and `ReplayPermit`.
  - `crates/kami-storage/src/lib.rs`: `Storage::commit_authenticated` and `commit_prepared` exclusively take `AuthenticatedCommit`.
  - `crates/kami-runtime/src/ingress.rs`: `AuthenticatedPeer` capability token can only be minted in `#[cfg(test)]`.
- **Implementation Mechanism**: Rust structs with private fields, module privacy (`pub(crate)`), and non-default constructors.
- **Relationships**: Reinforces Hexagonal Architecture and Reverse Dependency.
- **Status**: IMPLEMENTED AND VERIFIED.
- **Tradeoffs / Limitations**: Capability tokens cannot cross network or process boundaries directly; they protect in-process caller boundaries only.

### 4. Strong Typing / Newtype Idiom
- **Simple Explanation**: Wrapping primitive types (integers, arrays, UUIDs) in distinct, dedicated Rust structs so they cannot be accidentally mixed up.
- **Why Kami Uses It**: Prevents devastating domain bugs, such as passing a `DeviceId` where a `MessageId` or `ConversationId` is expected.
- **Source Evidence**:
  - `crates/kami-core/src/models.rs`: `MessageId(u128)`, `ConversationId(u128)`, `DeviceId([u8; 32])`, `SenderSequence(u64)`.
  - `crates/kami-protocol/src/lib.rs`: Dedicated strongly-typed wrapper structures over Protobuf messages.
- **Implementation Mechanism**: `pub struct MessageId(u128);` with explicit traits (`Copy`, `Clone`, `PartialEq`, `Eq`, `Hash`).
- **Relationships**: Foundational to Domain-Driven Design in `kami-core`.
- **Status**: IMPLEMENTED AND VERIFIED.
- **Tradeoffs / Limitations**: Requires explicit conversion (`.as_u128()`, `.as_bytes()`) when interfacing with raw database columns or wire buffers.

---

## Category B: Concurrency, Supervision, and Lifecycle Patterns

### 5. Actor Pattern (Single-Owner Event Loop)
- **Simple Explanation**: A software entity that owns its state privately and modifies it exclusively in response to messages received through an incoming mailbox queue.
- **Why Kami Uses It**: Prevents shared-memory multithreaded data races, lock contention, and inconsistent lifecycle states.
- **Source Evidence**:
  - `crates/kami-runtime/src/engine.rs`: The `run()` async function owns engine state (`Lifecycle`, `Snapshot`, `pending`, `draining`). All user commands pass through an `mpsc::Receiver<Command>`.
  - `crates/kami-storage/src/worker.rs`: `StorageWorker` owns the exclusive SQLite connection on a dedicated blocking thread, processing jobs sequentially via channel.
- **Implementation Mechanism**: Tokio `mpsc` channels and infinite `select!` loops.
- **Relationships**: Directly drives the Supervisor Pattern.
- **Status**: IMPLEMENTED AND VERIFIED.
- **Tradeoffs / Limitations**: Asynchronous message passing introduces slight latency compared to raw mutex locks, but guarantees thread safety.

### 6. Supervisor Pattern
- **Simple Explanation**: A parent process or task monitors the lifecycle of subordinate tasks, catches faults, manages graceful shutdown, and handles child termination.
- **Why Kami Uses It**: In Kami, background network timers (relay renewals) and database workers must be supervised so that worker panics or network timeouts never crash the entire application unnoticed.
- **Source Evidence**:
  - `crates/kami-runtime/src/engine.rs`: Uses `tokio::task::JoinSet` to supervise the relay adapter child task (`children.spawn(relay::run(...))`) and catches child termination via `children.join_next()`.
  - `crates/kami-runtime/src/worker.rs`: The blocking storage worker is spawned via `tokio::task::spawn_blocking`; if the worker thread terminates or panics, the engine detects channel closure, marks lifecycle as `Failed`, drains queues, and stops cleanly.
- **Implementation Mechanism**: `tokio::task::JoinSet` and `tokio::task::spawn_blocking`.
- **Relationships**: Works with Actor Model and Graceful Shutdown.
- **Status**: IMPLEMENTED AND VERIFIED.
- **Tradeoffs / Limitations**: Child tasks must cooperate with cancellation signals to ensure timely shutdown.

### 7. Thread Pool / Blocking Work Offloading
- **Simple Explanation**: Offloading heavy CPU work or blocking operating system I/O (file access, SQLite queries) to a dedicated thread pool so the asynchronous event loop stays responsive.
- **Why Kami Uses It**: SQLite disk I/O and SQLCipher key derivation are synchronous and blocking. Running them directly on Tokio worker threads would cause event-loop starvation and timer jitter.
- **Source Evidence**:
  - `crates/kami-runtime/src/engine.rs`: Spawns the storage worker on `tokio::task::spawn_blocking`.
  - `crates/kami-runtime/src/worker.rs`: Executes all SQLCipher transactions, directory locking, and key-provider queries inside the blocking thread.
- **Implementation Mechanism**: `tokio::task::spawn_blocking(move || worker::run(...))`.
- **Relationships**: Protects Bounded Channel Passing from starvation.
- **Status**: IMPLEMENTED AND VERIFIED.
- **Tradeoffs / Limitations**: Thread context-switching overhead between Tokio async tasks and OS blocking threads.

### 8. Bounded Channels and Backpressure Pattern
- **Simple Explanation**: Restricting message queues to a fixed maximum capacity so producers are slowed down or rejected when consumers cannot keep up.
- **Why Kami Uses It**: Prevents memory exhaustion attacks and queue bloat during high inbound message rates or slow disk writes.
- **Source Evidence**:
  - `crates/kami-runtime/src/lib.rs`: `Config` sets `command_capacity` (32), `event_capacity` (32), and `blocking_capacity` (8).
  - `crates/kami-runtime/src/engine.rs`: `commands.try_send()` returns `Error::Busy` when full.
  - `crates/kami-storage/src/open.rs`: `queue_capacity` bounds the storage worker mailbox.
- **Implementation Mechanism**: `tokio::sync::mpsc::channel(capacity)`.
- **Relationships**: Integral to Actor Model and System Stability.
- **Status**: IMPLEMENTED AND VERIFIED.
- **Tradeoffs / Limitations**: Callers must handle `Error::Busy` and implement backoff/retry.

### 9. Dual-Channel Telemetry (Authoritative Snapshots vs. Best-Effort Events)
- **Simple Explanation**: Separating critical state queries from high-volume diagnostic logs: state queries are always accurate and lossless, while diagnostic logs are allowed to drop under load.
- **Why Kami Uses It**: Telemetry consumers (UI loggers, metrics) must never slow down or deadlock core messaging operations.
- **Source Evidence**:
  - `crates/kami-runtime/src/engine.rs`: `Engine::snapshot()` uses `tokio::sync::watch` to guarantee instantaneous, non-blocking reads of authoritative state.
  - `crates/kami-runtime/src/engine.rs` (`fn event`): Diagnostics use `events.try_send()`. If full, the event is dropped and `s.dropped_events` increments.
- **Implementation Mechanism**: `tokio::sync::watch` for state; `tokio::sync::mpsc` for diagnostic streams.
- **Relationships**: Supports Bounded Channels and Backpressure.
- **Status**: IMPLEMENTED AND VERIFIED.
- **Tradeoffs / Limitations**: Telemetry consumers may miss individual historical event notifications during extreme load spikes.

---

## Category C: Persistence, Transactions, and Reliability Patterns

### 10. Write-Ahead Logging (WAL) and Immediate Transactions
- **Simple Explanation**: Writing database changes to an append-only log before applying them to physical database pages, combined with strict transaction locks that prevent concurrent write deadlocks.
- **Why Kami Uses It**: Prevents database corruption during sudden power failures or process crashes, and ensures concurrent readers do not block writers.
- **Source Evidence**:
  - `crates/kami-storage/src/open.rs`: Executes `PRAGMA journal_mode=WAL;`, `PRAGMA synchronous=FULL;`, `PRAGMA temp_store=MEMORY;`.
  - `crates/kami-storage/src/transactions.rs`: All state-modifying database operations use `TransactionBehavior::Immediate` (`BEGIN IMMEDIATE`).
- **Implementation Mechanism**: SQLite SQLCipher PRAGMA configurations and Rusqlite transaction builders.
- **Relationships**: Foundation for Atomic Compare-And-Swap and Crash Recovery.
- **Status**: IMPLEMENTED AND VERIFIED.
- **Tradeoffs / Limitations**: Requires periodic WAL checkpointing; increased disk write amplification.

### 11. Atomic Compare-And-Swap (CAS)
- **Simple Explanation**: Modifying a database record only if its current state matches an expected version or generation number.
- **Why Kami Uses It**: Protects contact verification trust states and E2EE session ratchets against concurrent, conflicting updates from multiple threads or stale processes.
- **Source Evidence**:
  - `crates/kami-storage/src/contact_trust.rs`: `compare_and_swap_contact(expected_generation, row)` verifies that the generation matches before updating; returns `StorageError::Conflict` on mismatch.
  - `crates/kami-storage/src/session_state.rs`: Validates previous session generation and SHA-256 digest before committing next ratchet state.
- **Implementation Mechanism**: SQL queries checking `WHERE generation = ?1`.
- **Relationships**: Core to Optimistic Concurrency Control.
- **Status**: IMPLEMENTED AND VERIFIED.
- **Tradeoffs / Limitations**: Conflicting writers fail and must reload current state before retrying.

### 12. Advisory File Locking / Profile Leasing
- **Simple Explanation**: Acquiring an exclusive operating system kernel file lock on a profile directory to guarantee that only one Kami process accesses the database at a time.
- **Why Kami Uses It**: Multiple Kami processes opening the same encrypted SQLite database simultaneously would result in database file corruption and cryptographic key conflicts.
- **Source Evidence**:
  - `crates/kami-storage/src/lease.rs`: `ProfileLease::acquire(path)` validates directory ownership, enforces `0700` permissions (Unix), and acquires an exclusive OS lock (`flock(LOCK_EX | LOCK_NB)`).
  - `crates/kami-runtime/src/worker.rs`: Retains `ProfileLease` on the blocking thread for the lifetime of the engine.
- **Implementation Mechanism**: `rustix::fs::flock` on Unix, OS-specific advisory locks.
- **Relationships**: Guards SQLite Database Integrity.
- **Status**: IMPLEMENTED AND VERIFIED (Linux verified; Windows/macOS native qualification pending).
- **Tradeoffs / Limitations**: A crashed process must release its file descriptor before another instance can acquire the profile.

### 13. Idempotent Crash Recovery and Exact-Frame Readback
- **Simple Explanation**: After a process restart or crash, recovering in-flight operations by reading exact persisted frames from disk rather than speculatively re-encrypting.
- **Why Kami Uses It**: Re-encrypting a message after a crash advances the Double Ratchet key, desynchronizing the sender and receiver and breaking conversation forward secrecy.
- **Source Evidence**:
  - `crates/kami-storage/src/sender_state.rs`: `Coordinator::enqueue` returns `EnqueueOutcome::Recovered(QueuedSend)` if the message ID already exists, re-emitting the exact stored ciphertext frame.
  - `crates/kami-runtime/src/engine.rs`: `run()` executes `worker::reconcile` on startup before entering `Lifecycle::Running`.
- **Implementation Mechanism**: SQL lookups by stable `MessageId` and raw frame storage in `e2ee_frame`.
- **Relationships**: Protects Double Ratchet State Monotonicity.
- **Status**: IMPLEMENTED AND VERIFIED.
- **Tradeoffs / Limitations**: Requires retaining raw encrypted frames in database tables until terminal delivery.

### 14. Lane Fencing and Sequence Reservation
- **Simple Explanation**: Enforcing strict per-conversation serialization by acquiring a durable reservation token before computing cryptographic operations.
- **Why Kami Uses It**: Multiple concurrent outgoing messages in the same conversation must be ordered strictly sequentially without interleaving sequence numbers.
- **Source Evidence**:
  - `crates/kami-storage/migrations/004_sender_state.sql`: `CREATE UNIQUE INDEX sender_prepare_lane ON sender_prepare(conversation, sender) WHERE state=0;`.
  - `crates/kami-storage/src/sender_state.rs`: `prepare_send()` reserves sequence; only one active preparation is permitted per lane at a time.
- **Implementation Mechanism**: SQLite partial unique index (`WHERE state=0`) and reservation expiration timers.
- **Relationships**: Works with Idempotent Crash Recovery.
- **Status**: IMPLEMENTED AND VERIFIED.
- **Tradeoffs / Limitations**: Concurrent sends in the exact same conversation lane are serialized; they do not execute in parallel.

---

## Category D: Security, Cryptography, and Trust Management Patterns

### 15. Double Ratchet Cryptographic Protocol
- **Simple Explanation**: A cryptographic key-management protocol that generates fresh ephemeral symmetric keys for every single message using a combination of Diffie-Hellman ratchets and symmetric KDF chains.
- **Why Kami Uses It**: Guarantees **Forward Secrecy** (past messages cannot be decrypted if current keys are compromised) and **Break-in Recovery** (an eavesdropper who learns a key cannot read future messages once fresh ratchet steps occur).
- **Source Evidence**:
  - `crates/kami-e2ee`: Wraps Matrix Vodozemac 0.11.1 Olm v1 Double Ratchet in `QuarantinedSession`.
- **Implementation Mechanism**: Ephemeral Curve25519 DH exchanges and HKDF key derivation.
- **Relationships**: Drives Same-Session Receipt Staging and Exact Replay Prevention.
- **Status**: IMPLEMENTED AND VERIFIED (Quarantined scope; production activation pending KAMI-27).
- **Tradeoffs / Limitations**: Upstream Vodozemac pins a 40-key retention ceiling for skipped keys and a 2,000-message gap ceiling.

### 16. Canonical Inner Envelope Framing
- **Simple Explanation**: Packaging application plaintext, sender/recipient device IDs, message IDs, and conversation IDs into a canonical binary envelope that is encrypted inside the AEAD payload.
- **Why Kami Uses It**: Prevents ciphertext-replay and identity-misbinding attacks where an attacker intercepts ciphertext and presents it to a different recipient or in a different conversation.
- **Source Evidence**:
  - `crates/kami-e2ee/src/envelope.rs`: `InnerEnvelopeV1` encodes fixed fields and validates length $\le 65,536$ bytes.
- **Implementation Mechanism**: Canonical binary serialization with strict byte-length bounds.
- **Relationships**: Protects Double Ratchet Payloads.
- **Status**: IMPLEMENTED AND VERIFIED.
- **Tradeoffs / Limitations**: Adds ~100 bytes of protocol framing overhead per message.

### 17. Trust-On-First-Use (TOFU) with Bilateral Human Verification
- **Simple Explanation**: Key fingerprint pinning where peers independently verify cryptographic fingerprints out-of-band before granting trusted status.
- **Why Kami Uses It**: Defends against Man-in-the-Middle (MitM) attacks by malicious network relays or key servers.
- **Source Evidence**:
  - `crates/kami-identity/src/trust.rs`: `ContactTrust` enforces `TrustState::{Pending, Verified, Changed, Blocked}`.
  - `crates/kami-identity/src/decision.rs`: `send_decision` allows transmission only when trust state is `Verified` and pins match.
- **Implementation Mechanism**: SHA-256 fingerprint generation, format versioning, and monotonic CAS storage.
- **Relationships**: Feeds into `ReceiveContext` and `Coordinator::claim`.
- **Status**: IMPLEMENTED AND VERIFIED.
- **Tradeoffs / Limitations**: Requires human interaction or secure out-of-band verification channel.

### 18. Same-Session Bidirectional Delivery Receipt Staging
- **Simple Explanation**: Generating delivery receipts inside the **same bidirectional Double Ratchet session** that transported the original message, advancing the ratchet by exactly 2 generations on the receiver side.
- **Why Kami Uses It**: Eliminates out-of-band receipt forgery and avoids maintaining two separate unilateral cryptographic sessions between the same pair of devices.
- **Source Evidence**:
  - `crates/kami-e2ee/src/authenticated/receipts.rs`: `prepare_delivery` executes `prepare_receive` ($\Delta\text{gen}=1$) and immediately stages `DeviceDelivered` reply in the reverse direction ($\Delta\text{gen}=1$), totaling 2 generations.
- **Implementation Mechanism**: Combined inbound message processing and outbound receipt encryption within `QuarantinedSession`.
- **Relationships**: Integrates `kami-e2ee`, `kami-receiver`, and `kami-storage`.
- **Status**: IMPLEMENTED AND VERIFIED.
- **Tradeoffs / Limitations**: Receiver must commit receipt artifacts to storage before returning `DeliveryArtifact`.

### 19. Keyed Request Binding
- **Simple Explanation**: Hashing outbound message submission parameters with a database-private HMAC secret key stored only in SQLCipher.
- **Why Kami Uses It**: Guarantees that an attacker with local memory access cannot alter a reserved sequence number, target recipient, or conversation ID while holding a reservation token.
- **Source Evidence**:
  - `crates/kami-storage/migrations/004_sender_state.sql`: `sender_binding_key` singleton table.
  - `crates/kami-storage/src/sender_state.rs`: `request_mac` computed via HMAC-SHA256; any parameter tampering triggers `StorageError::Conflict`.
- **Implementation Mechanism**: `hmac::Hmac<sha2::Sha256>`.
- **Relationships**: Protects Lane Fencing and Sequence Reservations.
- **Status**: IMPLEMENTED AND VERIFIED.
- **Tradeoffs / Limitations**: Database must remain open to verify reservations.

---

## Category E: Messaging, Ordering, and Deduplication Algorithms

### 20. Exact Wire-Frame Deduplication Algorithm
- **Simple Explanation**: When an inbound message frame arrives, checking if an identical frame exists in storage; if identical, returning the previously generated receipt without advancing the Double Ratchet key.
- **Why Kami Uses It**: Network retransmissions must not desynchronize Double Ratchet keys or generate duplicate chat messages.
- **Source Evidence**:
  - `crates/kami-storage/src/receiver_state.rs`: `recover()` compares `stored_frame == frame` byte-for-byte.
  - `crates/kami-receiver/src/receive.rs`: Replays return `ReceiveOutcome::Duplicate` with original `DeliveryArtifact`.
- **Implementation Mechanism**: Byte-exact comparison against stored `e2ee_frame`.
- **Relationships**: Directly protects Double Ratchet Ratchet Monotonicity.
- **Status**: IMPLEMENTED AND VERIFIED.
- **Tradeoffs / Limitations**: Submitting alternate ciphertext for an existing ID triggers a hard `Conflict` error.

### 21. Deterministic Exponential Backoff with Jitter
- **Simple Explanation**: Gradually increasing the delay between failed transmission attempts according to a power-of-two curve, capped at a maximum ceiling.
- **Why Kami Uses It**: Prevents retry storms from overwhelming failing peers or congested network relays.
- **Source Evidence**:
  - `crates/kami-storage/src/sender_state.rs`: `backoff_ms(attempt)` implements $1000 \times 2^{\text{attempt}-1}$, capped at 300,000 ms (5 minutes), terminating after 8 attempts (`MAX_ATTEMPTS`).
- **Implementation Mechanism**: Pure functional calculation in `backoff_ms()`.
- **Relationships**: Governs `sender_attempt` deadlines in `kami-storage`.
- **Status**: IMPLEMENTED AND VERIFIED.
- **Tradeoffs / Limitations**: Messages will reach terminal `Exhausted` state after 8 consecutive failures.

### 22. Presentation Ordering & Aging Gap Recovery Algorithm
- **Simple Explanation**: Holding out-of-order incoming messages in a gap ledger for up to 60 seconds to allow missing predecessors to arrive; if predecessors do not arrive within 60s, promoting the gap to permanent and presenting subsequent messages with gap indicators.
- **Why Kami Uses It**: Network packet reordering should not display jumbled conversations, but lost messages must not freeze the chat UI indefinitely.
- **Source Evidence**:
  - `crates/kami-storage/src/ordering.rs` & `migrations/005_receiver_state.sql`: `receiver_gap` records `state=0` (`Unresolved`), aging to `state=1` (`Permanent`) at $T+60\text{s}$.
- **Implementation Mechanism**: Monotonic sequence tracking in `ordered_history()`.
- **Relationships**: Works with Sequence Reservations.
- **Status**: IMPLEMENTED AND VERIFIED.
- **Tradeoffs / Limitations**: Out-of-order messages experience up to 60 seconds of presentation delay if preceding messages are delayed.

### 23. Terminal Attempt Lane with Late Receipt Precedence
- **Simple Explanation**: When an authenticated delivery receipt arrives for an outbound message that was previously marked as failed or canceled, updating the canonical message to Delivered and terminating the attempt lane.
- **Why Kami Uses It**: Network delays may deliver a message successfully even if the local sender timed out; the true remote device receipt is the authoritative truth.
- **Source Evidence**:
  - `crates/kami-storage/src/receiver_state.rs`: `consume()` sets `message.state = 1` (`Delivered`) and `sender_attempt.state = 3` (`Canceled`, clearing `token` and `deadline`).
- **Implementation Mechanism**: SQL updates in `consume()` transaction.
- **Relationships**: Works with Exponential Backoff and Attempt Leases.
- **Status**: IMPLEMENTED AND VERIFIED.
- **Tradeoffs / Limitations**: Only an authenticated receipt with matching frame digest can revive a failed attempt.

---

## Category F: Error Handling, Resource Management, and Testing Patterns

### 24. Zeroization / Memory Scrubbing Pattern
- **Simple Explanation**: Actively overwriting memory containing sensitive secrets (private keys, plaintext, account pickles) with zeroes before deallocating.
- **Why Kami Uses It**: Prevents secrets from lingering in freed heap memory, RAM dumps, or swap files where other processes or attackers could extract them.
- **Source Evidence**:
  - `crates/kami-key-provider/src/lib.rs`: `DatabaseKey` implements `zeroize::ZeroizeOnDrop`.
  - `crates/kami-storage/src/sender_state.rs`: `SendRequest` payload implements `ZeroizeOnDrop`.
  - `crates/kami-e2ee/src/persistence.rs`: `SnapshotTransfer` zeroizes memory buffers.
- **Implementation Mechanism**: The `zeroize` crate using compiler memory fences.
- **Relationships**: Core to Defensive Security Architecture.
- **Status**: IMPLEMENTED AND VERIFIED.
- **Tradeoffs / Limitations**: Does not guarantee memory scrubbing across OS core dumps or native VM page clones.

### 25. Bounded Allocation & Wire Pre-Scanning
- **Simple Explanation**: Inspecting length headers and allocating buffers only after confirming they fall within strict, hardcoded size limits.
- **Why Kami Uses It**: Prevents Out-Of-Memory (OOM) Denial-of-Service attacks from malicious network peers sending deceptive 4 GB length headers.
- **Source Evidence**:
  - `crates/kami-protocol/src/lib.rs`: `decode_frame` limits wire frames to 65,536 bytes (64 KiB); rejects invalid payloads without allocating inner vectors.
- **Implementation Mechanism**: Frame header pre-scanners and bounded buffer sizing.
- **Relationships**: Shields Tokio Event Loop and Storage Engines.
- **Status**: IMPLEMENTED AND VERIFIED.
- **Tradeoffs / Limitations**: Payloads larger than 64 KiB must be chunked or rejected.

### 26. External Privacy Probing & Boundary Qualification
- **Simple Explanation**: Dedicated integration test crates that attempt to access internal private fields, forge capability tokens, or construct invalid cryptographic states from outside the crate boundary.
- **Why Kami Uses It**: Proves that compiler visibility restrictions (`pub(crate)`) and capability tokens cannot be bypassed by external third-party crates.
- **Source Evidence**:
  - `crates/kami-e2ee/tests/privacy.rs`: Attempts to forge `AuthenticatedCommit` or `ReplayPermit` via rustc compilation probes.
  - `crates/kami-runtime/tests/privacy.rs`: Tests that production API has no public capability factory or unauthenticated submission path.
- **Implementation Mechanism**: Subprocess compilation tests (`trybuild` or manual rustc invocation).
- **Relationships**: Validates Type-State & Capability Pattern.
- **Status**: IMPLEMENTED AND VERIFIED.
- **Tradeoffs / Limitations**: Increases test build execution time.

### 27. Scoped POSIX Directory Isolation Test Pattern
- **Simple Explanation**: Constructing test directories in scoped, private user paths with strict permissions (`0700`), avoiding reliance on system-wide shared `/tmp` or environment variables like `TMPDIR`.
- **Why Kami Uses It**: Ensures automated test suites run deterministically in sandboxed, multi-tenant, or containerized environments with read-only root filesystems.
- **Source Evidence**:
  - `crates/kami-runtime/src/tests.rs`: `directory()` uses `HOME` or `USERPROFILE` with fallback to working directory, applying `0o700` permissions.
- **Implementation Mechanism**: `tempfile::Builder::new().tempdir_in(base)`.
- **Relationships**: Ensures Repeatable Continuous Integration.
- **Status**: IMPLEMENTED AND VERIFIED.
- **Tradeoffs / Limitations**: Test directories must be explicitly cleaned up after test completion.

### 28. Revocable Linear Connection Capability Pattern (KAMI-27)
- **Simple Explanation**: Issuing a single-ownership, non-cloneable capability struct (`AuthenticatedConnection`) representing an established cryptographic connection, which can be instantaneously invalidated across all threads via an atomic revocation flag.
- **Why Kami Uses It**: Guarantees that callers cannot manufacture fake network connection capabilities from strings or forge connection states, and allows the runtime to instantly revoke network capabilities if a profile lease expires or trust is revoked.
- **Source Evidence**:
  - `crates/kami-transport/src/connection.rs`: `AuthenticatedConnection` does not implement `Clone` or `Default`. It wraps an atomic `Revocation` handle, connection live-flag, and expiration `Instant`.
  - `crates/kami-transport/src/connection.rs`: Compile-fail doctests verify that cloning or default-constructing the capability fails compilation.
- **Implementation Mechanism**: Rust move semantics, `Arc<AtomicBool>`, and monotonic deadlines.
- **Relationships**: Consumed by `kami-runtime`; anchored by `kami-identity`.
- **Status**: IMPLEMENTED AND VERIFIED.
- **Tradeoffs / Limitations**: Capabilities cannot be shared across threads without explicit synchronization channels.

### 29. Expected-Peer Swarm Dial Binding Pattern (KAMI-27)
- **Simple Explanation**: Binding outbound network dials to an expected cryptographic `PeerId` before initiating a connection, ensuring the transport layer automatically aborts the handshake if a different entity answers at the target IP/port.
- **Why Kami Uses It**: Prevents Man-In-The-Middle (MITM) attacks and IP socket spoofing. If an attacker controls the target IP address or port, libp2p verifies the peer's private key during the cryptographic handshake and immediately rejects the connection before user data or protocol negotiation occurs.
- **Source Evidence**:
  - `crates/kami-transport/src/driver.rs`: `Driver` dials using `DialOpts::peer_id(expected_peer).addresses(candidates).build()`.
  - `crates/kami-transport/src/driver.rs`: Swarm events verify `send_decision(...) == SendDecision::Allow` against fresh `ContactTrust` records upon `ConnectionEstablished`.
- **Implementation Mechanism**: libp2p `DialOpts`, TLS 1.3 / Noise XX mutual handshake verification.
- **Relationships**: Bridges `kami-transport` and `kami-identity`.
- **Status**: IMPLEMENTED AND VERIFIED.
- **Tradeoffs / Limitations**: Outbound dials require prior out-of-band knowledge of the target's public `PeerId`.

### 30. Canonical Inner Plaintext Envelope with Outer Correspondence Commitment Pattern (KAMI-51 WP-A)
- **Simple Explanation**: Encrypting and authenticating a structured canonical binary envelope (`InnerEnvelopeV1`) inside the cryptographic session, while using an outer SHA-256 hash strictly as a correspondence commitment to verify that outer framing matches the inner plaintext.
- **Why Kami Uses It**: Matrix's Olm v1 lacks native Additional Authenticated Data (AAD) support. By embedding the entire message context (conversation ID, sender, recipient, message ID, sequence, timestamp, and Peer-ID proofs) inside the canonical plaintext envelope *before* Olm encryption, the Double Ratchet authenticates the entire context, while outer SHA-256 checks guarantee alignment between wire frames and decrypted payloads without functioning as a separate MAC.
- **Source Evidence**:
  - `crates/kami-e2ee/src/envelope.rs`: `InnerEnvelopeV1` canonical binary layout; outer `authentication_context` calculated as SHA-256 of inner bytes.
  - `crates/kami-e2ee/src/authenticated.rs`: Post-decryption context checks verify conversation ID, device IDs, message kind, and outer correspondence.
- **Implementation Mechanism**: Deterministic binary serialization + Olm v1 AES-256-CBC/HMAC-SHA-256 + SHA-256 commitment check.
- **Relationships**: Bridges `kami-protocol`, `kami-e2ee`, and `kami-storage`.
- **Status**: IMPLEMENTED AND VERIFIED (WP-A).
- **Tradeoffs / Limitations**: Requires allocating inner envelope buffers and performing post-decryption field comparisons.

### 31. Two-Process Owner-Operated Interactive Qualification Pattern (KAMI-27)
- **Simple Explanation**: Running two distinct OS processes in separate terminal sessions where a human operator explicitly compares cryptographic public-key fingerprints out-of-band, enters verification decisions, and exercises disconnect, reconnect, and rejection negative paths.
- **Why Kami Uses It**: Proves that the platform cannot be tricked by silent trust-on-first-use (TOFU) or automated mocks, establishing real-world human verification and Linux native D-Bus/GNOME Keyring integration.
- **Source Evidence**:
  - `scripts/kami27-manual/setup.sh`, `start-a.sh`, `start-b.sh`, `reject-negative.sh`.
  - `crates/kami-runtime/src/transport_tests.rs`: `manual_endpoint` interactive test harness.
- **Implementation Mechanism**: Bash harness scripts driving interactive `cargo test` runs with stdin/stdout terminal prompts.
- **Relationships**: Bridges `kami-identity`, `kami-transport`, and `kami-runtime`.
- **Status**: IMPLEMENTED AND VERIFIED.
- **Tradeoffs / Limitations**: Requires manual operator interaction; cannot be automated in headless continuous integration pipelines.

---

## Beginner-Friendly Guide: The 5 Key Patterns that Make Kami Secure & Reliable

For developers new to Kami, here is the plain-English explanation of the five most important patterns protecting user privacy:

1. **The Reverse Dependency Security Shield**:
   Most apps put the database at the bottom and crypto at the top. Kami flips this: storage imports crypto. This means storage is forbidden from writing any message to disk unless it receives an unforgeable cryptographic "receipt" (`AuthenticatedCommit`) proving the message was verified.

2. **The Double Ratchet Session Engine**:
   Every message has a brand-new cryptographic key. If an attacker intercepts your phone and steals the key for message #10, they still cannot read message #9 (past) or message #11 (future).

3. **Same-Session Delivery Receipts**:
   When you send a message, your friend's device decrypts it and stages a delivery confirmation inside the *exact same encrypted channel*, moving the encryption ratchet forward by two steps. It never uses unencrypted metadata for receipts.

4. **The Bounded Single-Owner Actor**:
   Kami never lets multiple threads fight over the database or cryptographic keys. One central engine handles commands in order, while a dedicated background worker talks to the encrypted database, guaranteeing zero lock-ups or corrupted files.

5. **Exact-Frame Crash Recovery**:
   If your phone runs out of battery while sending a message, Kami never guesses or re-encrypts when it powers back on. It re-emits the exact stored ciphertext from disk, keeping the encryption ratchet in perfect sync with your peer.

