# kami-storage

## Overview

- **Crate Name**: `kami-storage`
- **Simple Purpose**: The secure, encrypted filing cabinet for Kami. It saves message history, pending outbox queues, delivery receipts, verified contact trust states, and Double Ratchet session keys in a password-protected SQLite database (encrypted with SQLCipher) so that data remains safe even if the physical drive is stolen.
- **Problem it Solves in Kami**: Without encryption and strict queue rules, messages could be read in plain text by unauthorized software, and messages could be duplicated, lost during power failures, or delivered out of order. `kami-storage` provides crash-safe transactions, gap-ordering for messages, directory leasing to prevent concurrent process corruption, and atomic commits for Double Ratchet E2EE sessions.
- **Dependencies**: Depends internally on [[kami-core]] (domain facts and IDs), [[kami-key-provider]] (for the 32-byte encryption key), and [[kami-e2ee]] (for authenticated commit types). Externally uses `rusqlite` (v0.40.2, compiled with bundled SQLCipher and vendored OpenSSL), `zeroize`, and `rustix` (for safe Unix filesystem permissions).
- **Architectural Reverse Dependency (`kami-storage → kami-e2ee`)**:
  - In traditional layered architectures, encryption crates often sit below or wrap storage. In Kami, **`kami-storage` imports `kami-e2ee`**, while `kami-e2ee` never imports storage.
  - **Why this design?**
    1. Keeps `kami-e2ee` pure: E2EE contains zero database code, file I/O, or connection logic.
    2. Centralizes all persistence guarantees: `kami-storage` owns the single-threaded SQLite worker, table migrations (`001` to `003`), transaction atomicity (`BEGIN IMMEDIATE`), quotas, filesystem locking, and Compare-And-Swap (CAS) state updates.
    3. Prevents unauthenticated writes: The public storage API `commit_authenticated` **only accepts an opaque `kami_e2ee::AuthenticatedCommit`**. Callers cannot forge commits, fabricate neutral database records, or inject raw unauthenticated ciphertext into the database.
- **Requirements & Jira Traceability**:
  - **KAMI-22** (Profile lease, SQLite worker, and migrations): **Functionally implemented in the Rust codebase.** Jira reports its workflow status as *In Progress* because native Windows and macOS filesystem qualification (ACLs and reparse points) remain pending.
  - **KAMI-50**: Migration `002_contact_trust.sql` and durable Compare-And-Swap contact trust management.
  - **KAMI-51 (WP3 & WP2-B)**:
    - **WP3** (Migration `003_e2ee_state.sql` and private session state snapshots): Implemented and **owner accepted** as a bounded private storage foundation (`t_fd283614`, QA `t_2d834cf1`, Evaluator `t_82d8776a`).
    - **WP2-B** (Typed authenticated transaction commits): Implemented and **owner accepted with conditions** for quarantined scope only (task `t_b8cb3f89` run 126, QA `t_48119cc5` run 127, Evaluator `t_21f65076` run 128). Does not grant ACK emission authority or mark overall KAMI-51 complete.
- **Git Context**: Branch `identity-security`, commit `eedbb744565cb8af6c80c78b1b76b431c7bf0562`.

---

## Source Code Explanation

Re-exported publicly from [lib.rs](file:///home/phu/dev/kami/crates/kami-storage/src/lib.rs).

### 1. Module `lease` ([crates/kami-storage/src/lease.rs](file:///home/phu/dev/kami/crates/kami-storage/src/lease.rs))

Protects Kami profile directories from conflicting processes.

- **Struct**: `ProfileLease`
  - **What it represents**: An exclusive advisory file lock on the user's Kami profile directory (`kami.lock`).
  - **Why Kami needs it**: Only ONE Kami service instance may run on a profile at a time to prevent database corruption.
  - **Methods**:
    - `pub fn acquire(path: &Path) -> Result<Self, StorageError>`: Validates that the directory is owned by the current user with strict `0700` permissions (Unix) and acquires an exclusive OS lock. Fails closed on shared/world-writable directories.

---

### 2. Module `open` & `migrations` ([crates/kami-storage/src/open.rs](file:///home/phu/dev/kami/crates/kami-storage/src/open.rs), [crates/kami-storage/src/migrations.rs](file:///home/phu/dev/kami/crates/kami-storage/src/migrations.rs))

Controls opening, creating, and upgrading the encrypted database.

- **Struct**: `Config`
  - **What it represents**: Tunable storage settings: `queue_capacity` (1..=1024), `max_pending_messages` (4,096), `max_pending_bytes` (16 MiB), `max_database_pages` (16,384 pages = 64 MiB), busy timeout (250 ms).

- **Struct**: `Storage`
  - **What it represents**: An active, keyed SQLCipher connection handle.
  - **Methods**:
    - `pub fn create(...) -> Result<Self, StorageError>`: Explicitly creates a brand-new encrypted database. Writes a durable enrollment marker file, key-loads via `KeyProvider`, configures SQLCipher v4 (4,096-byte pages, `temp_store=MEMORY`, WAL enabled, `synchronous=FULL`), and runs migrations.
    - `pub fn open(...) -> Result<Self, StorageError>`: Opens an existing database. Refuses to silently create an empty database if files are missing.
  - **Schema Migrations**:
    - `001_initial.sql`: Outbox, inbox, sequence tracking, message history.
    - `002_contact_trust.sql` (KAMI-50): Neutral encrypted contact trust rows with monotonic generations.
    - `003_e2ee_state.sql` (KAMI-51 WP3): Encrypted Olm account (`e2ee_account`), session (`e2ee_session`), and frame (`e2ee_frame`) tables.

---

### 3. Module `contact_trust` ([crates/kami-storage/src/contact_trust.rs](file:///home/phu/dev/kami/crates/kami-storage/src/contact_trust.rs))

Persists contact verification states (KAMI-50).

- **Struct**: `ContactTrustRow`
  - **What it represents**: An encrypted database row storing a peer's 32-byte contact ID, trust state (`TrustState::{Unverified, Verified, Blocked, ChangedIdentity}`), positive monotonic generation counter, and descriptor bytes.
- **Methods on `Storage`**:
  - `pub fn load_contact(&self, contact: [u8; 32]) -> Result<Option<ContactTrustRow>, StorageError>`
  - `pub fn compare_and_swap_contact(&self, expected_generation: Option<u64>, row: ContactTrustRow) -> Result<ContactTrustRow, StorageError>`: Atomic Compare-And-Swap (CAS). Rejects stale writes if another process updated trust concurrently.

---

### 4. Module `session_state` (Private, [crates/kami-storage/src/session_state.rs](file:///home/phu/dev/kami/crates/kami-storage/src/session_state.rs))

Internal persistence engine for Double Ratchet sessions (KAMI-51 WP3 & WP2-B).

- **Structs**: `AccountRow` & `SessionRow`
  - **What they represent**: Zeroizing encrypted records storing Olm account prekey inventories and ratchet session states.
- **Function**: `pub(crate) fn commit_authenticated(db: &mut Connection, stage: AuthenticatedCommit, quota: u64, count: u32) -> Result<Vec<u8>, StorageError>`
  - **What it does**: Executes a single atomic `BEGIN IMMEDIATE` transaction that:
    1. Verifies the expected account and session digests against stored rows.
    2. Checks the live durable `Verified` contact trust generation.
    3. Writes the next session state and optional consumed one-time prekeys.
    4. Commits message facts, sender sequence, history, and exact frame bytes.
  - **Exact Retransmission Reconciliation**: If a message has already been committed, it reconciles facts and returns original bytes *without* updating the Double Ratchet state again.

---

### 5. Module `worker` ([crates/kami-storage/src/worker.rs](file:///home/phu/dev/kami/crates/kami-storage/src/worker.rs))

- **Struct**: `StorageWorker` & `WorkerHandle`
  - **What it represents**: Actor-pattern worker executing SQLite operations sequentially on a dedicated background thread.
- **New Method (WP2-B)**:
  - `pub fn commit_authenticated(&self, commit: AuthenticatedCommit) -> Result<Vec<u8>, StorageError>`: Public entry point to atomically commit an authenticated E2EE message. Returning bytes confirms durable disk persistence (not authorization to emit an ACK).

---

### 6. Modules `outbox`, `inbox`, `message`, `ordering`

- `StoredMessage`: Redacted payload wrapper (1..=65,536 bytes) with `MessageFacts`.
- `outbox`: Atomically proposes and commits sequential outbound messages before network transmission.
- `inbox`: Deduplicates received messages and records receipt facts for ACK generation.
- `ordering`: Implements `ordered_history` with bounded gap timeouts (up to 60 seconds) so out-of-order packets are displayed cleanly.

---

## Completion and Verification

- **Milestone 1 Completion Estimate**: **Not estimable**
  - *Milestone Status*: **Functionally Implemented** (KAMI-22 storage engine, KAMI-50 contact trust migration 002, KAMI-51 WP3 migration 003, and WP2-B authenticated commits pass on Linux; Windows/macOS native filesystem qualification, power-loss durability certification, and external rollback anchors remain release gates).
- **Whole-Project Completion Estimate**: **Not estimable**
- **Status Breakdown**:
  - **Implemented Behavior**: Full SQLCipher v4 encrypted persistence, profile directory leasing, numbered migrations 001-003, atomic outbox/inbox queues, duplicate filtering, gap ordering, contact trust CAS, and E2EE session transactions.
  - **Tested Behavior**: Linux is fully verified via unit tests, migration rollback tests, WAL file inspections, 12 subprocess kill checkpoints across all transaction phases, contact trust tests, and negative plaintext probes.
  - **Missing Behavior**: Native Windows and macOS profile leasing qualification.
  - **Known Limitations**: Database digests protect against corrupted data, but cannot detect full-database external rollback (e.g., restoring an entire old VM snapshot).

### Exact Verification Commands

```sh
# Run all unit tests in kami-storage
cargo test --locked -p kami-storage

# Run KAMI-50 contact trust and identity trust tests
cargo test --locked -p kami-storage --test contact_trust --test identity_trust

# Run KAMI-51 WP3 private state and migration 003 tests
cargo test --locked -p kami-storage --lib wp3_
cargo test --locked -p kami-storage --test e2ee_state

# Run WP2-B authenticated boundary integration tests
cargo test --locked -p kami-storage --test authenticated_boundary -- --nocapture

# Run negative plaintext probe (proves database refuses to open without SQLCipher)
CARGO_TARGET_DIR="$PWD/target/plaintext-negative" cargo run --locked \
  --manifest-path crates/kami-storage/tests/plaintext_probe/Cargo.toml
```
