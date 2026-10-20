# kami-storage

## Overview

- **Crate Name**: `kami-storage`
- **Simple Purpose**: The secure encrypted filing cabinet for Kami. It saves your message history, pending outbox messages, and delivery receipts in a password-protected SQLite database (encrypted with SQLCipher) so that your data is safe even if someone steals your computer's hard drive.
- **Problem it Solves in Kami**: Without encryption and strict queue rules, messages could be read in plain text by unauthorized software, and messages could be duplicated, lost during power failures, or delivered out of order. `kami-storage` provides crash-safe transactions, gap-ordering for messages, and locks the database to prevent two programs from accessing it simultaneously.
- **Dependencies**: Depends on [[kami-core]] (for message facts and IDs) and [[kami-key-provider]] (for the 32-byte encryption key). Externally uses `rusqlite` (v0.40.2, compiled with bundled SQLCipher and vendored OpenSSL), `zeroize`, and `rustix` (for safe Unix filesystem permissions).
- **Requirements & Jira Traceability**: Implements Milestone 1 encrypted local state requirements (SQLCipher v4, atomic outbox/inbox persistence, gap-ordering, profile locking).
- **Git Context**: Branch `protocol-storage`, commit `732156f6f13cbbe4d897f663c1f2546182619704`.

---

## Source Code Explanation

Re-exported publicly from [lib.rs](file:///home/phu/dev/kami/crates/kami-storage/src/lib.rs).

### 1. Module `lease` ([crates/kami-storage/src/lease.rs](file:///home/phu/dev/kami/crates/kami-storage/src/lease.rs))

Protects Kami profile directories from conflicting processes.

- **Struct**: `ProfileLease`
  - **What it represents**: An exclusive file lock on the user's Kami data directory.
  - **Why Kami needs it**: Only ONE Kami service instance may run on a profile at a time. If two instances ran concurrently, they could corrupt the database.
  - **Methods**:
    - `pub fn acquire(path: &Path) -> Result<Self, StorageError>`
      - **What it does**: Checks directory ownership (must be owned by the current user with strict `0700` permissions on Unix) and places an advisory OS lock on `kami.lock`.
      - **Errors**: Returns `StorageError::Locked` if another instance is already running, or `PermissionDenied` if permissions are too loose (e.g., world-writable `/tmp`).
      - **Status**: Implemented ([lease.rs#L30-L95](file:///home/phu/dev/kami/crates/kami-storage/src/lease.rs#L30-L95)).

---

### 2. Module `open` ([crates/kami-storage/src/open.rs](file:///home/phu/dev/kami/crates/kami-storage/src/open.rs))

Controls opening, creating, and configuring the encrypted database.

- **Struct**: `Config`
  - **What it represents**: Tunable settings for storage: max queued requests (default 32), max pending outbox items (default 4,096), max database pages (default 16,384 pages = 64 MiB), busy timeout (250 ms).

- **Struct**: `Storage`
  - **What it represents**: An active, keyed SQLCipher connection handle.
  - **Methods**:
    - `pub fn create(dir: &Path, provider: &dyn KeyProvider, profile: &ProfileKeyId, config: Config) -> Result<Self, StorageError>`
      - **What it does**: Initializes a brand-new encrypted database. Writes a durable enrollment marker file, loads the key, enables SQLCipher encryption with 4,096-byte pages, runs migrations, and verifies that the database header is encrypted.
      - **Security Sensitivity**: Rejects unencrypted SQLite builds. If someone tries to open the database without SQLCipher, it fails closed immediately with `StorageError::CipherUnavailable`.
      - **Status**: Implemented ([open.rs#L40-L110](file:///home/phu/dev/kami/crates/kami-storage/src/open.rs#L40-L110)).
    - `pub fn open(dir: &Path, provider: &dyn KeyProvider, profile: &ProfileKeyId, config: Config) -> Result<Self, StorageError>`
      - **What it does**: Opens an existing database. If the database file is missing, it refuses to silently create an empty one (preventing accidental key replacement or loss of history).

---

### 3. Module `message` ([crates/kami-storage/src/message.rs](file:///home/phu/dev/kami/crates/kami-storage/src/message.rs))

- **Struct**: `StoredMessage`
  - **What it represents**: A complete message ready to be saved, containing [[kami-core]] `MessageFacts` and the encrypted/protected payload bytes (1 to 65,536 bytes).
  - **Security Sensitivity**: Payload memory is wrapped in a zeroizing buffer and redacted in `Debug` prints.

---

### 4. Modules `outbox` & `inbox` ([crates/kami-storage/src/outbox.rs](file:///home/phu/dev/kami/crates/kami-storage/src/outbox.rs), [crates/kami-storage/src/inbox.rs](file:///home/phu/dev/kami/crates/kami-storage/src/inbox.rs))

- **Outbox**:
  - `enqueue`: Atomically verifies sequence numbers and commits an outbound message to disk before network sending begins.
  - `pending(limit)`: Retrieves queued messages that need to be transmitted or retried.
- **Inbox**:
  - `accept`: Atomically records an incoming message. If it is already in the database, it returns `Acceptance::Duplicate` so Kami can re-send the delivery receipt without showing the user two copies of the same chat message.

---

### 5. Module `ordering` ([crates/kami-storage/src/ordering.rs](file:///home/phu/dev/kami/crates/kami-storage/src/ordering.rs))

- **Function**: `ordered_history`
  - **What it does**: Solves the "out-of-order arrival" problem. If message #1 arrives, but message #3 arrives before message #2, this module waits (up to a configurable 60-second gap timeout) before showing message #3 to the user, ensuring the conversation flows naturally.

---

### 6. Module `worker` ([crates/kami-storage/src/worker.rs](file:///home/phu/dev/kami/crates/kami-storage/src/worker.rs))

- **Struct**: `StorageWorker` & `WorkerHandle`
  - **What it represents**: An actor pattern wrapper that runs SQLite queries on a single dedicated background thread.
  - **Why Kami needs it**: SQLite can run into thread-locking issues if accessed directly from multiple async Tokio tasks. The worker channel ensures all database commands execute sequentially in a safe, queue-bounded environment.

---

## Completion and Verification

- **Milestone 1 Completion Estimate**: **90%**
- **Whole-Project Completion Estimate**: **Not estimable** (cloud sync and distributed replicas belong to later milestones).
- **Status Breakdown**:
  - **Implemented Behavior**: Complete SQLCipher v4 encrypted storage, profile directory leasing, schema migrations (001_initial.sql), atomic outbox/inbox queues, duplicate filtering, gap ordering, and actor worker facade.
  - **Tested Behavior**: Linux is fully verified via automated tests covering schema rollbacks, kill/restart persistence, database page limits, WAL file inspections, and negative plaintext checks.
  - **Missing Behavior**: Windows and macOS filesystem locking and ACL validation (currently Linux Unix-only in this branch).
  - **Known Limitations**: Does not perform network transmission, receipt signing, or end-to-end payload encryption (handled by protocol and networking crates).

### Exact Verification Commands

```sh
# Run all storage tests
cargo test -p kami-storage

# Run negative plaintext probe (proves database refuses to open without SQLCipher)
CARGO_TARGET_DIR="$PWD/target/plaintext-negative" cargo run --locked \
  --manifest-path crates/kami-storage/tests/plaintext_probe/Cargo.toml
```
