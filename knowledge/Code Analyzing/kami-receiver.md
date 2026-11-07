# Kami Receiver (`kami-receiver`)

Analysis Baseline: Git branch `protocol-storage`, HEAD commit `7bffc6fe8523161e2af3fd8d87b6fb3aadb92bc8` ("Merge KAMI-24 into main") with uncommitted KAMI-25 working-tree implementation.

---

## 1. Overview & Purpose

`kami-receiver` is a synchronous, runtime-neutral coordinator crate that accepts inbound encrypted frames, drives atomic double-ratchet decryption via `kami-e2ee`, verifies presentation ordering and duplicate deduplication, coordinates atomic commits into SQLCipher via `kami-storage`, stages corresponding `DeviceDelivered` receipt frames in the same bidirectional session, and returns durable presentation artifacts to callers.

Like `kami-sender`, `kami-receiver` contains **no** network transport, event loops, async runtimes, or background workers. It is an inbound coordinator invoked synchronously whenever an encrypted payload arrives from an authenticated transport boundary.

```
       +---------------------------------------------+
       |             Transport / Boundary            |
       +---------------------------------------------+
                              | Inbound EncryptedFrame
                              v
                 +--------------------------+
                 |  kami_receiver::Receiver |
                 +--------------------------+
                  /                        \
    (Inbound Delivery &                    (Atomic Commit &
     Same-Session Receipt)                  Deduplication Check)
                /                            \
               v                              v
     +-------------------+          +-------------------+
     |     kami-e2ee     |          |    kami-storage   |
     | (QuarantinedSess) |          |  (SQLCipher DB)   |
     +-------------------+          +-------------------+
```

### Interactive Architecture Diagrams
- [KAMI-VIS-002: Rust Crate Dependency Graph](file:///home/phu/dev/kami/knowledge/Code%20Analyzing/Visualizations/01-System-Architecture/KAMI-VIS-002/crate-dependencies.html) — Explorable workspace dependency graph including `kami-receiver` and `kami-sender`.
- [KAMI-VIS-025: End-to-End Messaging Lifecycle & Receipt Flow](file:///home/phu/dev/kami/knowledge/Code%20Analyzing/Visualizations/01-System-Architecture/KAMI-VIS-025/messaging-lifecycle.html) — Sequence diagram tracing sender preparation, inbound verification, same-session receipt staging, and storage commits.
- [KAMI-VIS-026: Receiver Inbound Reliability & Recovery Flow](file:///home/phu/dev/kami/knowledge/Code%20Analyzing/Visualizations/01-System-Architecture/KAMI-VIS-026/receiver-reliability.html) — Workflow diagram covering inbound ingress, exact duplicate re-ACK, conflict errors, gap ordering, and crash recovery.

---

## 2. Public API & Core Types

### 2.1 `ReceiveContext`

Defined in [`crates/kami-receiver/src/context.rs`](file:///home/phu/dev/kami/crates/kami-receiver/src/context.rs):

```rust
pub struct ReceiveContext {
    pub local: AccountId,
    pub remote: AccountId,
    pub conversation: ConversationId,
    pub trust: TrustState,
    pub end_peer: EndPeerContext,
    pub now_ms: i64,
}
```

- **Admission Verification**: Caller verifies that `local` is the intended recipient, `remote` is the peer identity, and `conversation` matches.
- **`end_peer` (`EndPeerContext`)**: Cryptographically binds the authenticated origin identity. A network relay or gateway cannot forge this context; it must represent the verified end-to-end peer.
- **`ReplayPermit` creation**: `ReceiveContext::replay_permit()` constructs an internal capability (`kami_e2ee::ReplayPermit`) permitting `kami-storage` to look up previously authenticated durable material for replay handling.

### 2.2 `Receiver` Entry Points

Defined in [`crates/kami-receiver/src/receive.rs`](file:///home/phu/dev/kami/crates/kami-receiver/src/receive.rs):

```rust
pub struct Receiver<'a, S> {
    storage: &'a S,
}
```

Two main receiver operations are provided:
1. **`Receiver::receive`**:
   Processes an inbound frame for an **already established** bidirectional session.
   ```rust
   pub fn receive(
       &self,
       context: &ReceiveContext,
       frame: &EncryptedFrame,
       session: &QuarantinedSession,
   ) -> Result<ReceiveOutcome, Error>
   ```
2. **`Receiver::receive_first`**:
   Processes an inbound prekey bootstrap frame when establishing a **new inbound session**, consuming a prekey and initializing the session using `Account` and `Endpoint` pairs.
   ```rust
   pub fn receive_first(
       &self,
       context: &ReceiveContext,
       frame: &EncryptedFrame,
       account: &Account,
       local_endpoint: &Endpoint,
       remote_endpoint: &Endpoint,
   ) -> Result<ReceiveOutcome, Error>
   ```

### 2.3 `ReceiveOutcome`

```rust
pub enum ReceiveOutcome {
    New {
        delivery: DeliveryArtifact,
        proposed_session: QuarantinedSession,
    },
    Duplicate {
        delivery: DeliveryArtifact,
    },
}
```

- **`ReceiveOutcome::New`**: Inbound frame was novel and decryptable. Contains the decrypted `DeliveryArtifact` (plaintext, message facts, ordering metadata, and staged `DeviceDelivered` receipt frame) and the newly advanced `proposed_session`.
- **`ReceiveOutcome::Duplicate`**: The frame is an exact cryptographic replay of a previously processed message. Returns the original `DeliveryArtifact` without ratcheting the session.

### 2.4 `DeliveryArtifact`

```rust
pub struct DeliveryArtifact {
    pub artifact_id: [u8; 32],
    pub sender: AccountId,
    pub message_id: MessageId,
    pub conversation: ConversationId,
    pub plaintext: Vec<u8>,
    pub received_at: i64,
    pub receipt_frame: Option<EncryptedFrame>,
    pub presentation_order: Option<i64>,
}
```

---

## 3. Inbound Lifecycle & Security Invariants

### 3.1 Flow of Inbound Frame Processing

1. **Context & Identity Validation**:
   - `ReceiveContext` checks that the caller context is valid.
   - Trust state is checked (`TrustState::Trusted`); untrusted or quarantined peer states reject admission before decryption.
2. **Exact Replay & Deduplication Check**:
   - `kami-storage` is queried via `storage.recover_delivery(ctx.permit()?, frame.to_vec())`.
   - The method consumes the `ReplayPermit` and requires the full wire `frame: Vec<u8>`.
   - If a recorded delivery already exists for this sender and message ID:
     - Storage verifies `stored_frame == frame` byte-for-byte against the stored `e2ee_frame`.
     - If identical, the coordinator returns `ReceiveOutcome::Duplicate { delivery }` without ratcheting the session.
     - If the wire frame differs by even a single byte or conflicts with stored facts/pin, `StorageError::Conflict` is returned.
3. **Double-Ratchet Inbound Decryption & Staging (`prepare_delivery`)**:
   - `kami_e2ee::prepare_delivery` is invoked inside `kami-e2ee`.
   - This validates the wire format, performs skipped-key or ratcheted key derivation, and verifies AEAD authentication over the ciphertext.
   - Simultaneously, inside the **same bidirectional session**, `prepare_delivery` stages an outbound `DeviceDelivered` receipt frame targeting the sender.
   - This advances the session generation count by 2 (1 inbound message step + 1 outbound receipt step), maintaining lockstep ratchet continuity between sender and receiver.
4. **Ordering & Gap Evaluation**:
   - `kami-storage` evaluates sequence continuity using `ordered_history`.
   - If an ordering gap exists (preceding messages missing), a gap observation is recorded in `receiver_gap`.
   - Gap observation states: `0 = Unresolved` (grace period active), `1 = Permanent` (aged past 60-second local threshold).
5. **Atomic SQLCipher Commit**:
   - `storage.commit_delivery(...)` executes in a single SQL transaction:
     - Writes inbound payload to message and history tables.
     - Saves exact `delivery_artifact` containing receipt frame and digest.
     - Writes updated ratchet session generation state with CAS protection.
     - Consumes one-time prekeys if `receive_first` was invoked.
     - Updates conversation sequence tracking and gap tables.
6. **Delivery Readback & Outcome Return**:
   - If the commit succeeds, returns `ReceiveOutcome::New`.
   - If a transient or ambiguous worker conflict occurs, storage readback resolves whether the commit succeeded, preventing uncommitted ratchet advances.

---

## 4. Key Guarantees & Non-Bypass Properties

- **Replay Protection**: Identical frames return previously stored receipt frames without stepping the cryptographic ratchet.
- **Tamper Resistance**: Inbound frames failing AEAD authentication or claiming an identity not matching `end_peer` are rejected prior to session or storage updates.
- **Receipt Binding**: The staged `DeviceDelivered` receipt binds the exact SHA-256 digest of the inbound text frame, preventing receipt swapping or spoofing across messages.
- **Session Monotonicity**: Ratchet advances are conditional on atomic database commit. An uncommitted or failed frame will not advance caller session state.

---

## 5. Scope Boundary & Deferred Functionality

`kami-receiver` completes KAMI-25 bounded receiver coordination. The following capabilities are explicitly deferred:

| Deferred Capability | Status / Tracking | Description |
|---|---|---|
| Live Network Transport | Deferred | Socket I/O, WebSockets, HTTP/2, and TLS listeners are not part of `kami-receiver`. |
| Async Scheduling | Deferred | Background polling, tokio runtimes, and worker threads are excluded. |
| Automatic Receipt Dispatch | Deferred | Outbound receipt dispatch across network hops requires live transport infrastructure. |
| Multi-Device Fanout | Deferred | Receiver coordination is bounded to single pairwise peer sessions. |

---

## 6. Implementation & Verification

- **Crate Root**: [`crates/kami-receiver`](file:///home/phu/dev/kami/crates/kami-receiver)
- **Unit & Integration Tests**:
  - [`crates/kami-receiver/tests/receive.rs`](file:///home/phu/dev/kami/crates/kami-receiver/tests/receive.rs): Tests successful delivery, deduplication, first-message prekey bootstrap, ordering gaps, and receipt staging.
  - [`crates/kami-receiver/tests/failures.rs`](file:///home/phu/dev/kami/crates/kami-receiver/tests/failures.rs): Tests invalid payloads, AEAD authentication failures, identity mismatch, and conflicting replays.
- **Verification Command**:
  ```bash
  cargo test --locked -p kami-receiver --test receive --test failures
  ```

