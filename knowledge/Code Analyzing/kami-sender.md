# kami-sender

## Overview

- **Crate Name**: `kami-sender`
- **Simple Purpose**: The outbound message dispatcher and coordinator for Kami. It makes sure that when a user or agent sends a message, the message is assigned an exact sequence number, encrypted using Double Ratchet E2EE in memory, saved durably into SQLCipher storage before any network attempt, and retransmitted with the exact same ciphertext frame if a retry or process restart occurs.
- **Problem it Solves in Kami**: Sending encrypted messages across lossy networks requires solving subtle distributed systems problems:
  1. *Race conditions & sequence gaps*: Competing outbound messages in the same conversation could scramble monotonic sequence numbers.
  2. *Cryptographic state desynchronization*: If an encryption ratchet advances in RAM but the database write fails or the process crashes before transmission, the local ratchet is ahead of the remote recipient, permanently breaking future decryption.
  3. *Uncontrolled retry storms*: Retrying failed messages without backoff or leasing could flood connections, re-encrypt the same message under a different ratchet key (creating fork attacks), or continue sending after a contact's identity has changed.
  4. *`kami-sender` solves this* by providing an atomic, synchronous, storage-backed coordinator that fences the send lane, validates request binding, commits E2EE snapshots atomically with sender queue rows, enforces 30-second attempt leases with exponential backoff (capped at 5 minutes), and recovers exact stored frames without re-encrypting.
- **Dependencies**:
  - [[kami-core]]: Pure domain identifiers (`MessageId`, `ConversationId`, `DeviceId`, `SenderSequence`) and message facts.
  - [[kami-storage]]: SQLCipher persistence engine, Migration `004_sender_state.sql`, sequence reservations, attempt tracking, and leases.
  - [[kami-e2ee]]: Quarantined Double Ratchet engine (`QuarantinedSession`) and `AuthenticatedCommit`.
  - [[kami-identity]]: Contact trust states (`ContactTrust`, `ContactTrustState`), pinned keys, and `send_decision` verification.
  - `libp2p-identity`: `PeerId` type used for recipient transport binding verification.
- **Architectural Position**:
  - `kami-sender` is a **synchronous, runtime-neutral service coordinator**. It contains **no async runtime code, no Tokio tasks, no timers, and no network sockets**.
  - It is designed to be executed on a dedicated blocking service task (or worker thread) by the future supervised background daemon ([[kami-cli]] / KAMI-26).
- **Requirements & Jira Traceability**:
  - **KAMI-24** (Outbox message lifecycle, sequence reservations, retry policies, and crash recovery): **Owner-accepted bounded implementation** (Implementation task `t_3caec9e9`, pre-review `t_903b8e19` PASS, QA `t_d1b015bb` PASS, Evaluator `t_4c8b8ad4` PASS).
  - Consumes the owner-accepted KAMI-51 WP3 storage foundation and WP2-B authenticated boundary.
- **Git Context**: Branch `protocol-storage`, HEAD commit `7bffc6fe8523161e2af3fd8d87b6fb3aadb92bc8` ("Merge KAMI-24 into main").

### Interactive Architecture Diagrams
- [KAMI-VIS-002: Rust Crate Dependency Graph](file:///home/phu/dev/kami/knowledge/Code%20Analyzing/Visualizations/01-System-Architecture/KAMI-VIS-002/crate-dependencies.html) — Explorable workspace dependency graph including `kami-sender` and `kami-receiver`.
- [KAMI-VIS-025: End-to-End Messaging Lifecycle & Receipt Flow](file:///home/phu/dev/kami/knowledge/Code%20Analyzing/Visualizations/01-System-Architecture/KAMI-VIS-025/messaging-lifecycle.html) — Sequence diagram tracing sender preparation, inbound verification, same-session receipt staging, and storage commits.

---

## Source Code Explanation

Re-exported publicly from [crates/kami-sender/src/lib.rs](file:///home/phu/dev/kami/crates/kami-sender/src/lib.rs).

### 1. Module `coordinator` ([crates/kami-sender/src/coordinator.rs](file:///home/phu/dev/kami/crates/kami-sender/src/coordinator.rs))

The primary entry point that coordinates the seam between `kami-storage` and `kami-e2ee`.

- **Struct**: `Coordinator<'a>`
  - **What it represents**: A lightweight, synchronous coordinator borrowing a reference to the active `kami_storage::Storage` instance (`&'a Storage`).
  - **Constructor**:
    - `pub fn new(storage: &'a Storage) -> Self`: Creates a coordinator bound to the keyed SQLCipher storage.

- **Enum**: `EnqueueOutcome`
  - **What it represents**: The two distinct outcomes of enqueuing an outbound message:
    - `Committed { message: QueuedSend, proposed_session: Box<QuarantinedSession> }`: The message and its E2EE commit were durably written to SQLCipher. The caller may now safely install the proposed in-memory session ratchet.
    - `Recovered(QueuedSend)`: The message was already committed previously (or in a concurrent race). Returns the exact stored ciphertext frame and queue metadata. Restoring or guessing an in-memory ratchet is *not* performed.

- **Method**: `pub fn enqueue(&self, request: SendRequest, session: &QuarantinedSession, trust: &ContactTrust, peer: &PeerId, now_ms: i64) -> Result<EnqueueOutcome, Error>`
  - **Step-by-step execution flow**:
    1. *Input Validation & Cleanup*: Verifies `now_ms >= 0` and calls `storage.cleanup_prepared(now_ms)` to transition any expired uncommitted reservations into tombstone state.
    2. *Durable Sequence Reservation*: Calls `storage.prepare_send(request.clone(), now_ms)`.
       - If the request is already committed, immediately returns `EnqueueOutcome::Recovered(message)` without touching the crypto session.
       - Otherwise, acquires a `PreparedSend` reservation token with the next monotonic `SenderSequence`. The partial unique index `sender_prepare_lane` prevents any other message from preparing in the same `(conversation, sender)` lane concurrently.
    3. *Fresh Trust Check*: Reads the recipient's live contact row directly from storage via `self.fresh_trust(request.recipient)`. If the persisted generation or pinned key differs from the caller's snapshot, rejects the send with `Error::IdentityHeld`.
    4. *In-Memory E2EE Staging*: Calls `session.prepare_text(...)` on the quarantined session. This generates the ciphertext frame and an `AuthenticatedCommit` containing cryptographic state digests.
    5. *Request & Reservation Verification*: Asserts that the authenticated message facts match the request (`sender`, `recipient`, `conversation`, `message_id`) and that the sequence equals the reserved `reservation.sequence()`. Rejects mismatches with `Error::RequestMismatch`.
    6. *Atomic Storage Commit*: Calls `storage.commit_prepared(reservation, stage, now_ms)`. In a single `BEGIN IMMEDIATE` SQLite transaction, storage validates the reservation token, writes the new session snapshot, updates the account state, writes the message to `e2ee_frame` and `message`, marks the reservation as committed (`state = 1`), and inserts the attempt row (`sender_attempt` with state `Queued`).
    7. *Readback Verification*: Calls `storage.prepare_send(request, now_ms)` to verify durable commit:
       - If committed: confirms stored bytes match stage frame bytes and returns `EnqueueOutcome::Committed`.
       - If reservation remains uncommitted: returns `Error::RolledBack`.
       - If indeterminate or lost reply: returns `Error::Indeterminate(StorageError::Conflict)` to prevent speculative ratchet advancement.

---

### 2. Module `retry` ([crates/kami-sender/src/retry.rs](file:///home/phu/dev/kami/crates/kami-sender/src/retry.rs))

Enforces attempt leases, fresh identity reverification, and exponential backoff policies.

- **Constants (re-exported from `kami-storage`)**:
  - `pub const ATTEMPT_LEASE_MS: i64 = 30_000` (30 seconds): Maximum lifetime of an active transmission lease.
  - `pub const MAX_ATTEMPTS: u32 = 8`: Bound on transmission attempts before a message is marked `Exhausted`.
  - `pub const PREPARATION_LIFETIME_MS: i64 = 60_000` (60 seconds): Lifetime of an uncommitted sequence reservation.

- **Function**: `pub fn backoff_ms(attempt: u32) -> Option<i64>`
  - Calculates deterministic exponential backoff:
    - Attempt 0: `None` (immediate)
    - Attempt 1: 1,000 ms (1 second)
    - Attempt 2: 2,000 ms (2 seconds)
    - Attempt 3: 4,000 ms (4 seconds)
    - Attempt 4: 8,000 ms (8 seconds)
    - Attempt 5: 16,000 ms (16 seconds)
    - Attempt 6: 32,000 ms (32 seconds)
    - Attempt 7: 64,000 ms (64 seconds)
    - Capped at 300,000 ms (5 minutes).
    - Attempt >= 8 (`MAX_ATTEMPTS`): Returns `None` (attempts exhausted).

- **Methods on `Coordinator`**:
  - `pub fn claim(&self, sender: DeviceId, id: MessageId, peer: &PeerId, now: i64) -> Result<Option<SendLease>, Error>`
    - Re-evaluates contact trust against the caller's supplied `PeerId` using `kami_identity::send_decision`.
    - If contact trust is `Changed` or `Blocked`, immediately transitions the attempt row to `SendState::HeldIdentity` and returns `Err(Error::IdentityHeld)`.
    - If eligible and lease is available, calls `storage.claim_send(...)`, which increments the attempt counter, sets `state = InFlight`, generates an opaque 32-byte lease token, sets `deadline = now + 30_000`, and returns `Some(SendLease)`.
    - If not yet eligible under the backoff schedule, returns `Ok(None)`.
  - `pub fn resume(&self, sender: DeviceId, id: MessageId, peer: &PeerId, now: i64) -> Result<(), Error>`
    - Resumes an outbound message held in `SendState::HeldIdentity`.
    - Requires explicit independent reverification of the **original pinned identity** (matching the recipient's generation).
    - It **never adopts a replacement identity** for an existing queued message.
    - Unblocks the message back to `SendState::Queued` without resetting historical attempt counters.
  - `pub fn receive_receipt(&self, frame: &EncryptedFrame, session: &QuarantinedSession, end_peer: &PeerId, now: i64) -> Result<ReceiptOutcome, Error>` (KAMI-25)
    - Consumes an incoming `DeviceDelivered` delivery receipt frame targeting an outbound message.
    - Checks fresh contact trust and verified `end_peer` context.
    - Checks exact duplicate receipts using `storage.recover_delivery_receipt`. Replays return `ReceiptOutcome::Duplicate` without ratcheting the session.
    - Drives `QuarantinedSession::prepare_receipt` to decrypt and authenticate receipt facts and the text frame SHA-256 digest.
    - Executes `storage.commit_delivery_receipt`, atomically transitioning the target outbound message state to `DeviceDelivered` and marking attempt lanes terminal.
    - Handles late receipts safely: genuine late receipts supersede stale failure or canceled attempt records.

---

### 3. Module `receipt` ([crates/kami-sender/src/receipt.rs](file:///home/phu/dev/kami/crates/kami-sender/src/receipt.rs))

Introduced in KAMI-25 to define delivery receipt processing outcomes:

- **Enum**: `ReceiptOutcome`
  - `Delivered { facts: DeliveryReceiptFacts, proposed_session: QuarantinedSession }`: Receipt was novel, cryptographically verified, and committed. Returns updated session state to caller.
  - `Duplicate(DeliveryReceiptFacts)`: Exact cryptographic replay; returns stored delivery facts without modifying Double Ratchet state.

---

### 4. Module `error` ([crates/kami-sender/src/error.rs](file:///home/phu/dev/kami/crates/kami-sender/src/error.rs))

- **Enum**: `Error`
  - `Storage(StorageError)`: Underlying SQLCipher database error.
  - `CryptoRejected`: Double Ratchet staging rejected the payload or trust proofs.
  - `RequestMismatch`: Staged cryptographic facts did not match the original `SendRequest` or reserved sequence.
  - `IdentityHeld`: Recipient identity trust changed, became unverified, or does not match caller evidence.
  - `RolledBack(StorageError)`: Storage readback proved the sequence reservation is still uncommitted; safe to retry with the same message ID.
  - `Indeterminate(StorageError)`: Storage state is ambiguous (e.g. process was killed during commit); caller must reopen and reconcile by stable ID rather than encrypting speculatively.

---

### 5. Re-Exported Storage Types from `kami-storage`

To maintain a clean boundary, `kami-sender` re-exports the sender types defined in `kami_storage::sender_state`:

- `SendRequest`: Bounded input struct (`message`, `conversation`, `sender`, `recipient`, `created_at_ms`, and zeroizing `text`).
- `PreparedSend`: Opaque sequence reservation token (`sender`, `id`, `sequence`, `token`).
- `Preparation`: Enum representing `Prepared(PreparedSend)` or `Committed(QueuedSend)`.
- `QueuedSend`: Immutable readback struct holding exact ciphertext frame, delivery facts, attempt count, `SendState`, and next eligible timestamp.
- `SendLease`: Opaque active lease handle holding `token` and `expires_at_ms`.
- `SendState`: Enum with 6 states: `Queued (0)`, `InFlight (1)`, `Exhausted (2)`, `Canceled (3)`, `HeldIdentity (4)`, `Delivered (5)`.

---

## Architectural Invariants and Safety Guarantees

```
[Caller / Application]
         │
         │  1. SendRequest
         ▼
[kami-sender::Coordinator]
         │
         ├──► 2. Storage::prepare_send ──────► [SQLCipher: sender_prepare]
         │       (Reserves sequence,               (Unique index locks lane)
         │        checks HMAC binding)
         │
         ├──► 3. Fresh ContactTrust check ────► [SQLCipher: contact_trust]
         │
         ├──► 4. QuarantinedSession::prepare_text
         │       (In-memory Double Ratchet staging)
         │
         ├──► 5. Storage::commit_prepared ───► [SQLCipher: BEGIN IMMEDIATE]
         │       (Atomic E2EE commit +             ├─ Update e2ee_session
         │        commit reservation +             ├─ Insert e2ee_frame
         │        create sender_attempt)           ├─ Mark sender_prepare=1
         │                                         └─ Insert sender_attempt=Queued
         │
         ▼
[EnqueueOutcome::Committed]
(Only now is the in-memory ratchet installed)
```

1. **Exact-Frame Crash Recovery**: If a process crashes or restarts after `commit_prepared`, calling `Coordinator::enqueue` with the same `SendRequest` returns `EnqueueOutcome::Recovered(QueuedSend)`. It re-emits the exact stored ciphertext frame without advancing the Olm ratchet.
2. **Lane Fencing (`sender_prepare_lane`)**: The partial unique index `CREATE UNIQUE INDEX sender_prepare_lane ON sender_prepare(conversation, sender) WHERE state=0` prevents multiple active preparations in the same conversation from interleaving out of order.
3. **Keyed Request Binding (`sender_binding_key`)**: Storage generates a random 32-byte key stored only inside the SQLCipher database. The request parameters (`message`, `conversation`, `sender`, `recipient`, `created_at_ms`, `text`) are hashed with HMAC-SHA256 (`request_mac`). Tampering with request fields while holding a reservation causes immediate `StorageError::Conflict`.
4. **Zero Ratchet Desynchronization on Rollback**: If database commit fails, the proposed in-memory session is discarded and `Error::RolledBack` is returned. The caller's session remains at its prior generation, preventing state desynchronization.
5. **No Speculative Transmission**: A sender lease (`SendLease`) can only be acquired for messages that have already been durably written to `e2ee_frame` and `sender_attempt`.

---

## Scope Boundary & Deferred Functionality

> [!IMPORTANT]
> The bounded KAMI-24 implementation is strictly a **synchronous service coordination layer**. It deliberately defers all asynchronous, network, and transport responsibilities:

- **No Network Transport / Sockets**: `kami-sender` contains no networking code. It does not open sockets, connect to libp2p swarms, or send bytes over the wire (deferred to KAMI-27 / KAMI-31).
- **No Async Runtime / Timers**: Contains no Tokio event loop, sleep timers, or background polling threads. In KAMI-26, the newly implemented [[kami-runtime]] supervisory layer hosts `Coordinator::enqueue`, `Coordinator::claim`, and `storage.cleanup_prepared` sequentially on a dedicated blocking thread worker, and drives periodic 1-second attempt retry sweeps, while preserving `kami-sender`'s synchronous boundaries and fail-closed live transport guarantees.
- **No Direct vs. Relay Selection**: Path selection is not handled here (deferred to KAMI-31).
- **No Live End-Peer Provenance**: Caller-supplied `PeerId` is treated as verification evidence against local pinned trust, not as proof of live remote network connection.
- **Delivery Receipt Handling**: Delivery receipt processing is functionally implemented via `Coordinator::receive_receipt` (KAMI-25), while outbound receipt network dispatch and wire emission remain deferred to live transport (KAMI-26 / KAMI-27).
- **Platform Qualification**: Tested on Linux; native Windows (DPAPI / ACLs) and macOS (Keychain / sandbox) qualification remain separate release gates.

---

## Completion and Verification

- **Milestone 1 Status**: **Owner-Accepted Bounded Implementation**
  - Implementation task `t_3caec9e9`, pre-review `t_903b8e19` PASS, QA `t_d1b015bb` PASS, Evaluator `t_4c8b8ad4` PASS.
- **Status Breakdown**:
  - **Implemented**: Synchronous coordinator, sequence reservation, lane fencing, fresh trust verification, atomic E2EE commit, exact-frame recovery, 30s attempt leases, exponential backoff (1s base, 5min cap, 8 max attempts), cancellation, and identity holds.
  - **Tested**: Verified on Linux via multi-threaded race tests (`tests/coordinator.rs`), retry backoff policy tests (`tests/retry_state.rs`), and storage integration suites.
  - **Deferred**: Supervisor runtime (KAMI-26), direct transport (KAMI-27), relay transport (KAMI-30), automatic path fallback (KAMI-31), Windows/macOS native qualification.

### Exact Verification Commands

```sh
# Run all unit and integration tests in kami-sender
cargo test --locked -p kami-sender

# Run coordinator crash recovery and concurrent race tests
cargo test --locked -p kami-sender --test coordinator

# Run retry policy and backoff curve tests
cargo test --locked -p kami-sender --test retry_state

# Run formatting and warnings-denied Clippy checks across the workspace
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
```
