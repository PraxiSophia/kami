# kami-storage

## Overview

- **Crate Name**: `kami-storage`
- **Simple Purpose**: The secure, encrypted filing cabinet for Kami. It saves message history, pending outbox queues, delivery receipts, verified contact trust states, Double Ratchet session keys, and outbound sender state in a password-protected SQLite database (encrypted with SQLCipher) so that data remains safe even if the physical drive is stolen.
- **Problem it Solves in Kami**: Without encryption and strict queue rules, messages could be read in plain text by unauthorized software, and messages could be duplicated, lost during power failures, or delivered out of order. `kami-storage` provides crash-safe transactions, gap-ordering for messages, directory leasing to prevent concurrent process corruption, atomic commits for Double Ratchet E2EE sessions, and durable sender preparation/lease tracking.
- **Dependencies**: Depends internally on [[kami-core]] (domain facts and IDs), [[kami-key-provider]] (for the 32-byte database encryption key), and [[kami-e2ee]] (for authenticated commit types). Externally uses `rusqlite` (v0.40.2, compiled with bundled SQLCipher and vendored OpenSSL), `zeroize`, `hmac`, `sha2`, and `rustix` (for safe Unix filesystem permissions).
- **Architectural Reverse Dependency (`kami-storage → kami-e2ee`)**:
  - In traditional layered architectures, encryption crates often sit below or wrap storage. In Kami, **`kami-storage` imports `kami-e2ee`**, while `kami-e2ee` never imports storage.
  - **Why this design?**
    1. Keeps `kami-e2ee` pure: E2EE contains zero database code, file I/O, or connection logic.
    2. Centralizes all persistence guarantees: `kami-storage` owns the single-threaded SQLite worker, table migrations (`001` to `004`), transaction atomicity (`BEGIN IMMEDIATE`), quotas, filesystem locking, and Compare-And-Swap (CAS) state updates.
    3. Prevents unauthenticated writes: The public storage APIs `commit_authenticated` and `commit_prepared` **only accept an opaque `kami_e2ee::AuthenticatedCommit`**. Callers cannot forge commits, fabricate neutral database records, or inject raw unauthenticated ciphertext into the database.
- **Requirements & Jira Traceability**:
  - **KAMI-22** (Profile lease, SQLite worker, and migrations): **Functionally implemented in the Rust codebase.** Jira reports its workflow status as *In Progress* because native Windows and macOS filesystem qualification (ACLs and reparse points) remain pending.
  - **KAMI-50**: Migration `002_contact_trust.sql` and durable Compare-And-Swap contact trust management.
  - **KAMI-51 (WP3 & WP2-B)**:
    - **WP3** (Migration `003_e2ee_state.sql` and private session state snapshots): Implemented and **owner accepted** as a bounded private storage foundation (`t_fd283614`, QA `t_2d834cf1`, Evaluator `t_82d8776a`).
    - **WP2-B** (Typed authenticated transaction commits): Implemented and **owner accepted with conditions** for quarantined scope only (task `t_b8cb3f89` run 126, QA `t_48119cc5` run 127, Evaluator `t_21f65076` run 128). Does not grant ACK emission authority or mark overall KAMI-51 complete.
  - **KAMI-24** (Outbound sender state, Migration `004_sender_state.sql`, sequence reservations, attempt leases, and crash recovery): **Owner-accepted bounded implementation** (Implementation `t_3caec9e9`, QA `t_d1b015bb` PASS, Evaluator `t_4c8b8ad4` PASS). Provides the durable persistence backend for [[kami-sender]].
- **Git Context**: Branch `protocol-storage`, commit `0002406ea6e0c12789df74ff12e04c96a3b8f122`.

---

## Source Code Explanation

Re-exported publicly from [crates/kami-storage/src/lib.rs](file:///home/phu/dev/kami/crates/kami-storage/src/lib.rs).

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
  - **What it represents**: Tunable storage settings: `queue_capacity` (1..=1024), `max_pending_messages` (4,096), `max_pending_bytes` (16 MiB), `max_database_pages` (16,384 pages = 64 MiB), `busy_timeout` (250 ms), `outbox_messages` (configurable queue limit).

- **Struct**: `Storage`
  - **What it represents**: An active, keyed SQLCipher connection handle.
  - **Methods**:
    - `pub fn create(...) -> Result<Self, StorageError>`: Explicitly creates a brand-new encrypted database. Writes a durable enrollment marker file, key-loads via `KeyProvider`, configures SQLCipher v4 (4,096-byte pages, `temp_store=MEMORY`, WAL enabled, `synchronous=FULL`), and runs migrations.
    - `pub fn open(...) -> Result<Self, StorageError>`: Opens an existing database. Refuses to silently create an empty database if files are missing.
  - **Schema Migrations**:
    - `001_initial.sql`: Outbox, inbox, sequence tracking, message history (`profile`, `sender_sequence`, `message`, `history`, `receipt`).
    - `002_contact_trust.sql` (KAMI-50): Neutral encrypted contact trust rows with monotonic generations (`contact_trust`).
    - `003_e2ee_state.sql` (KAMI-51 WP3): Encrypted Olm account (`e2ee_account`), session (`e2ee_session`), and frame (`e2ee_frame`) tables.
    - `004_sender_state.sql` (KAMI-24): Durable sender state tables:
      - `sender_binding_key`: Singleton table holding a private 32-byte HMAC key for request authenticators.
      - `sender_prepare`: Monotonic sequence reservations with 60-second timeouts (`PREPARATION_LIFETIME_MS`) and partial unique index `sender_prepare_lane ON sender_prepare(conversation, sender) WHERE state=0` to serialize preparation in a conversation lane.
      - `sender_attempt`: Outbound transmission attempts, attempt counts, 30-second lease tokens (`ATTEMPT_LEASE_MS`), retry deadlines, and failure reasons (`AckLost`, `ConnectionLost`, `Unavailable`).

---

### 3. Module `contact_trust` ([crates/kami-storage/src/contact_trust.rs](file:///home/phu/dev/kami/crates/kami-storage/src/contact_trust.rs))

Persists contact verification states (KAMI-50).

- **Struct**: `ContactTrustRow`
  - **What it represents**: An encrypted database row storing a peer's 32-byte contact ID, trust state (`TrustState::{Pending, Verified, Changed, Blocked}`), positive monotonic generation counter, and candidate/pinned descriptor bytes.
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
    1. Verifies expected account and session digests against stored rows.
    2. Checks live durable `Verified` contact trust generation.
    3. Writes the next session state and optional consumed one-time prekeys.
    4. Commits message facts, sender sequence, history, and exact frame bytes into `e2ee_frame`.
  - **Exact Retransmission Reconciliation**: If a message has already been committed, it reconciles facts and returns original frame bytes *without* advancing or updating the Double Ratchet state again.

---

### 5. Module `sender_state` ([crates/kami-storage/src/sender_state.rs](file:///home/phu/dev/kami/crates/kami-storage/src/sender_state.rs))

Durable state engine for outbound message preparation, sequence reservations, and transmission leases (KAMI-24).

- **Constants**:
  - `pub const PREPARATION_LIFETIME_MS: i64 = 60_000` (60 seconds)
  - `pub const ATTEMPT_LEASE_MS: i64 = 30_000` (30 seconds)
  - `pub const MAX_ATTEMPTS: u32 = 8`
- **Functions & Types**:
  - `pub fn backoff_ms(attempt: u32) -> Option<i64>`: Exponential backoff with 1s base (`1_000 * 2^(attempt - 1)`), capped at 5 minutes (`300_000 ms`). Returns `None` once attempt >= 8.
  - `SendRequest`: Input struct wrapping submission parameters and zeroizing text payload.
  - `PreparedSend`: Reservation token guaranteeing exclusive access to a sequence number in a conversation lane.
  - `Preparation`: Enum representing `Prepared(PreparedSend)` or `Committed(QueuedSend)`.
  - `QueuedSend`: Immutable readback struct holding exact ciphertext frame, delivery facts, attempt count, `SendState`, and next eligible timestamp.
  - `SendLease`: Active transmission lease containing an opaque 32-byte token and expiration timestamp.
  - `SendState`: Enum with 5 states: `Queued (0)`, `InFlight (1)`, `Exhausted (2)`, `Canceled (3)`, `HeldIdentity (4)`.
- **Key Invariants**:
  - *Keyed HMAC Request Binding*: Request fields are hashed with HMAC-SHA256 using a database-private key from `sender_binding_key`. Any tampering with request parameters causes `StorageError::Conflict`.
  - *Lane Fencing*: The `sender_prepare_lane` index permits only one active (`state=0`) preparation per `(conversation, sender)` lane.
  - *Atomic Commit*: `commit_prepared` validates the preparation token and consumes the reservation, commits E2EE session state, writes `e2ee_frame`, and inserts the `sender_attempt` row in a single atomic transaction.

---

### 6. Module `worker` ([crates/kami-storage/src/worker.rs](file:///home/phu/dev/kami/crates/kami-storage/src/worker.rs))

- **Struct**: `StorageWorker` & `WorkerHandle`
  - Actor-pattern worker executing SQLite operations sequentially on a dedicated background thread.
- **Sender Worker APIs (KAMI-24)**:
  - `pub fn prepare_send(&self, request: SendRequest, now_ms: i64) -> Result<Preparation, StorageError>`
  - `pub fn commit_prepared(&self, reservation: PreparedSend, commit: AuthenticatedCommit, now_ms: i64) -> Result<Vec<u8>, StorageError>`
  - `pub fn cleanup_prepared(&self, now_ms: i64) -> Result<usize, StorageError>`: Cleans expired uncommitted reservations into tombstones.
  - `pub fn claim_send(&self, sender: DeviceId, id: MessageId, now: i64) -> Result<Option<SendLease>, StorageError>`: Leases an outbound frame for 30 seconds.
  - `pub fn fail_send(&self, lease: SendLease, now_ms: i64) -> Result<(), StorageError>`
  - `pub fn record_send_failure(&self, sender: DeviceId, id: MessageId, token: [u8; 32], reason: AttemptFailure, now: i64) -> Result<(), StorageError>`: Records failure and schedules next backoff deadline.
  - `pub fn cancel_send(&self, sender: DeviceId, id: MessageId) -> Result<(), StorageError>`: Marks attempt state as `Canceled`.
  - `pub fn hold_send(&self, sender: DeviceId, id: MessageId) -> Result<(), StorageError>`: Transitions message to `HeldIdentity`.
  - `pub fn resume_send(&self, sender: DeviceId, id: MessageId, expected_trust_generation: u64, now: i64) -> Result<(), StorageError>`: Resumes a held message after explicit reverification of original expected recipient.
  - `pub fn recover_sends(&self, limit: usize) -> Result<Vec<QueuedSend>, StorageError>`: Enumerates queued outbound messages across process reopen.

---

## Completion and Verification

- **Milestone 1 Completion Estimate**: **Not estimable**
  - *Milestone Status*: **Functionally Implemented & Owner-Accepted Bounded Foundations** (KAMI-22 storage engine, KAMI-50 contact trust migration 002, KAMI-51 WP3 migration 003, WP2-B authenticated commits, and KAMI-24 migration 004 sender state).
- **Whole-Project Completion Estimate**: **Not estimable**
- **Status Breakdown**:
  - **Implemented Behavior**: Full SQLCipher v4 encrypted persistence, profile directory leasing, numbered migrations 001-004, atomic outbox/inbox queues, duplicate filtering, gap ordering, contact trust CAS, E2EE session transactions, sequence reservations, attempt leases, and exact-frame crash recovery.
  - **Tested Behavior**: Linux is verified via unit tests, migration rollback tests, WAL file inspections, 12 subprocess kill checkpoints, contact trust tests, `sender_state` durability tests, `sender_input` quota tests, and negative plaintext probes.
  - **Missing Behavior**: Native Windows and macOS profile leasing qualification.
  - **Known Limitations**: Database digests protect against corrupted data, but cannot detect full-database external rollback (e.g., restoring an entire old VM snapshot).

### Exact Verification Commands

```sh
# Run all unit and integration tests in kami-storage
cargo test --locked -p kami-storage

# Run KAMI-24 sender persistence and reservation tests
cargo test --locked -p kami-storage --test sender_state --test sender_input

# Run KAMI-50 contact trust tests
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
