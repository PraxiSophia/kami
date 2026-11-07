# kami-e2ee

## Overview

- **Crate Name**: `kami-e2ee`
- **Simple Purpose**: The end-to-end encryption (E2EE) engine for Kami. It encrypts private chat messages before they leave your device and decrypts them only when they arrive on your recipient's device, ensuring that nobody in the middle—not even network relays—can read your messages.
- **Problem it Solves in Kami**: Without end-to-end encryption, network attackers or relay servers could intercept, eavesdrop on, or alter chat messages. `kami-e2ee` wraps messages in an authenticated inner envelope, manages Signal/Matrix-style Double Ratchet sessions using the upstream `vodozemac` library (Olm v1), and produces opaque candidate records that can be safely committed to encrypted storage.
- **Dependencies**: Depends internally on [[kami-core]] (domain IDs and facts), [[kami-identity]] (Olm prekey bindings and device trust), and [[kami-protocol]] (frame wire codec). Externally uses `vodozemac` (v0.11.1, Matrix's pure-Rust cryptographic library for Olm/Megolm), `libp2p-identity` (PeerId encodings), `sha2`, `zeroize`, `serde`/`serde_json`, and `thiserror`.
- **Architectural Dependency Direction**: `kami-e2ee` **never depends on `kami-storage`**. Instead, [[kami-storage]] depends on `kami-e2ee` (the reverse dependency pattern), ensuring that cryptographic invariants and message authentication remain completely independent of database engines.
- **Requirements & Jira Traceability**: Implements the foundation for **KAMI-51** (authenticated session and crash-safe ratchet persistence).
  - **Overall KAMI-51 Status**: **Not complete** (Jira status: *To Do*).
  - **Work Package Acceptance Distinction**:
    - **WP1** (Provider & profile foundation): Implemented and independently reviewed (`t_9da66f08`, QA `t_36a44ee9`, Evaluator `t_aab9a8e3`).
    - **WP2-A2** (Authenticated inner envelope): Implemented and independently reviewed (`t_501a91b6`, QA `t_707f16fb`, Evaluator `t_099f6098`).
    - **WP3** (Private persistence foundation): Implemented, independently reviewed, and **owner accepted** as a bounded private storage foundation (`t_fd283614`, QA `t_2d834cf1`, Evaluator `t_82d8776a`).
    - **WP2-B** (Typed storage composition): Implemented, independently reviewed, and **owner accepted with conditions** for quarantined scope only (task `t_b8cb3f89` run 126, QA `t_48119cc5` run 127, Evaluator `t_21f65076` run 128). Conditions mandate no public activation, no ACK emission authority, no external rollback anchor, and no claim of production qualification.
    - **Downstream WP4 & WP5**: Runtime/transport integration and qualification remain pending.
- **Git Context**: Branch `identity-security`, commit `eedbb744565cb8af6c80c78b1b76b431c7bf0562` (working tree integration).

---

## Source Code Explanation

Public types are exported from [lib.rs](file:///home/phu/dev/kami/crates/kami-e2ee/src/lib.rs). Internal modules (`envelope`, `authenticated`, `persistence`) are kept private and quarantined to prevent accidental exposure of raw unauthenticated cryptographic primitives.

### 1. Module `types` ([crates/kami-e2ee/src/types.rs](file:///home/phu/dev/kami/crates/kami-e2ee/src/types.rs))

- **Struct**: `ApplicationContext`
  - **What it represents**: Metadata identifying which conversation, message, sender, and recipient a cryptographic operation belongs to.
  - **Why Kami needs it**: Prevents "context confusion" attacks where ciphertext meant for one chat room is maliciously replayed in another.
  - **Key fields**: `conversation` (`ConversationId`), `message` (`MessageId`), `sender` (`DeviceId`), `recipient` (`DeviceId`).
  - **Methods**:
    - `pub fn validate(&self) -> Result<(), Error>`: Ensures all IDs are nonzero.
    - `pub fn encode(&self) -> Result<Vec<u8>, Error>`: Serializes the context into a canonical big-endian byte vector (max 512 bytes).

---

### 2. Module `profile` ([crates/kami-e2ee/src/profile.rs](file:///home/phu/dev/kami/crates/kami-e2ee/src/profile.rs))

- **Struct**: `Profile`
  - **What it represents**: Cryptographic profile settings for a session.
  - **Why Kami needs it**: Specifies the protocol version (`ProfileVersion::V1`).
  - **Unenforced Knobs Removed**: Speculative settings (`max_skipped_keys = 64` and `max_prekey_batch = 100`) were explicitly removed because the adapter never enforced them. Compile-fail tests verify that these knobs cannot be set or accessed.

---

### 3. Module `state` ([crates/kami-e2ee/src/state.rs](file:///home/phu/dev/kami/crates/kami-e2ee/src/state.rs))

- **Struct**: `StateEnvelope`
  - **What it represents**: A serialized snapshot of the ratchet session state, labeled with a strict version and a monotonic generation counter.
  - **Header Structure**: Exactly **25 bytes** (`HEADER_LEN`):
    - Magic `b"KAMI-STATE\0"`: 11 bytes
    - Version `u16`: 2 bytes (`StateVersion::V1 = 1`)
    - Generation `u64`: 8 bytes
    - Provider length `u32`: 4 bytes
  - **Ceiling**: Maximum provider state is **1,048,576 bytes (1 MiB)** (`MAX_STATE_BYTES`).
  - **Anti-Rollback**: When restoring state, the generation counter must match the expected generation *exactly*; any discrepancy (stale or future) returns `Error::StateRollback`.

---

### 4. Module `provider` ([crates/kami-e2ee/src/provider.rs](file:///home/phu/dev/kami/crates/kami-e2ee/src/provider.rs))

- **Trait**: `E2eeProvider`
  - **What it is**: The abstract interface for encryption engines.
  - **Methods**: `encrypt`, `decrypt`, and `supports_aad`.
  - **Security Sensitivity**: `supports_aad(&self)` defaults to `false`. If a provider cannot cryptographically bind the application context, it must fail closed rather than releasing unauthenticated plaintext or ciphertext.

---

### 5. Module `vodozemac` ([crates/kami-e2ee/src/vodozemac.rs](file:///home/phu/dev/kami/crates/kami-e2ee/src/vodozemac.rs))

- **Struct**: `VodozemacProvider`
  - **What it represents**: The adapter wrapping Matrix's `vodozemac::olm::Session`.
  - **Why Public Encrypt/Decrypt Fails Closed (WP1)**: Olm v1 does not feature a native Additional Authenticated Data (AAD) interface. Because simple JSON serialization cannot guarantee cryptographic binding of message participants, every public call to `encrypt` or `decrypt` on `VodozemacProvider` unconditionally fails closed with `Error::AadUnsupported`.
  - **Frame & Buffer Limits**:
    - `MAX_CIPHERTEXT_BYTES = 65_536`: Provider JSON input ceiling *before provider parsing* for the WP1 foundation.
    - `MAX_PLAINTEXT_BYTES = 65_536`: Staging plaintext limit.
  - **No Raw Escape Hatches**: There is no public `into_session()` or raw pointer method. Attackers cannot bypass validation by peeling away the wrapper.

---

### 6. Module `envelope` (Private, [crates/kami-e2ee/src/envelope.rs](file:///home/phu/dev/kami/crates/kami-e2ee/src/envelope.rs))

- **Struct**: `InnerEnvelopeV1` (WP2-A2)
  - **What it represents**: The canonical binary byte layout of the plaintext payload *before* Olm encryption.
  - **Binary Layout**:
    ```text
    b"KAMI-INNER-V1\0" || version(1) || olm_profile(1) || kind || direction ||
    conversation_id[16] || message_id[16] || sender_device[32] || recipient_device[32] ||
    session_id[43] || (sequence[8] OR receipt_target[16]) || created_at_ms[8] ||
    sender_peer_len[2] || sender_peer || recipient_peer_len[2] || recipient_peer ||
    sender_binding_digest[32] || recipient_binding_digest[32] || payload_len[4] || payload
    ```
  - **Exact 32-Byte Commitment**: Outer Protobuf frames carry an `authentication_context` field calculated as:
    $$\text{SHA-256}(b"\text{KAMI-INNER-CONTEXT-V1}\backslash 0" \parallel \text{canonical\_inner\_envelope\_bytes})$$
    After staged decryption, the inner envelope must match this commitment and the outer frame *byte-for-byte*. This hash is a correspondence commitment between layers, not standalone authentication; authenticity comes from the verified Olm session and anchored endpoint checks.
  - **Frame Budgets & Plaintext Allowance**:
    - The entire inner envelope is capped at **65,536 bytes** (`MAX_INNER_BYTES`).
    - The outer Protobuf frame is independently capped at **65,536 bytes** (`MAX_ENCODED_FRAME_SIZE` in `kami-protocol`).
    - Because the outer frame must fit envelope fields, provider ciphertext overhead, and the 32-byte commitment within 65,536 bytes, the usable application plaintext text budget is strictly less than 65,536 bytes (it is not a flat 64-KiB text allowance).

---

### 7. Module `authenticated` ([crates/kami-e2ee/src/authenticated.rs](file:///home/phu/dev/kami/crates/kami-e2ee/src/authenticated.rs))

This module implements the quarantined **WP2-B** composition boundary.

- **Struct**: `Endpoint`
  - **What it represents**: A verified participant endpoint combining a `DeviceId`, a `PeerBindingProof` (linking device to libp2p `PeerId`), an `OlmBindingProof` (linking device to Olm keys), and the retained `OlmBindingData` selection.
  - **Methods**:
    - `pub fn new(...) -> Self`: Accepts untrusted inputs and re-verifies them at every stage.

- **Struct**: `QuarantinedSession`
  - **What it represents**: An active, in-memory Double Ratchet session quarantined for controlled staging and qualification.
  - **Key Methods**:
    - `pub fn outbound(account: Account, local: Endpoint, remote: Endpoint, conversation: ConversationId, trust: &ContactTrust, end_peer: &PeerId, now: u64) -> Result<Self, Error>`: Initializes a new outbound Olm session.
    - `pub fn inbound(...) -> Result<(AuthenticatedCommit, Self), Error>`: Processes the very first inbound prekey message, consuming a one-time prekey (OTK) or accepting a fallback prekey.
    - `pub fn prepare_text(&self, message: MessageId, sequence: SenderSequence, created_at_ms: u64, text: &str, trust: &ContactTrust, end_peer: &PeerId, now: u64) -> Result<(AuthenticatedCommit, Self), Error>`: Encrypts text, advances the ratchet, and stages an `AuthenticatedCommit`.
    - `pub fn prepare_receive(&self, frame: &[u8], expected: &ExpectedContext, trust: &ContactTrust, end_peer: &PeerId, now: u64) -> Result<(AuthenticatedCommit, Self), Error>`: Decrypts an incoming frame, validates inner commitments, and stages an `AuthenticatedCommit`.
    - `pub fn prepare_retry(&self, trust: &ContactTrust, end_peer: &PeerId, now: u64) -> Result<AuthenticatedCommit, Error>`: Produces an exact duplicate candidate for network retry without advancing the ratchet or re-encrypting.

- **Struct**: `AuthenticatedCommit`
  - **What it represents**: An opaque, unforgeable token containing verified message facts, wire frame bytes, and encrypted account/session snapshots.
  - **Scope of the No-Bypass Guarantee**:
    - `AuthenticatedCommit` has private fields, no neutral or public constructors, and does not implement `Clone`, `Default`, `Debug`, or `serde`.
    - `Storage::commit_authenticated` is the single public write entry point for authenticated messages, guaranteeing that external callers cannot forge unauthenticated commits or inject raw ciphertext into storage.
    - **In-Process Custodian Limits**: Consuming `into_material().into_snapshots().consume(...)` transfers raw zeroizing Olm pickles (including private keys) to the storage worker. A legitimate in-process caller with access to the type can technically access these pickles. `AuthenticatedCommit` is an encapsulation boundary against caller bypass/forgery, NOT a security boundary against an in-process attacker or malicious custodian with arbitrary code execution in the same memory space. Furthermore, it does not guarantee memory erasure across native copies, allocator residue, or OS core dumps.

- **Struct**: `SnapshotTransfer`
  - **What it represents**: An in-process custodian transfer holding zeroizing memory buffers for Olm account and session pickles.

---

### 8. Module `authenticated/receipts` ([crates/kami-e2ee/src/authenticated/receipts.rs](file:///home/phu/dev/kami/crates/kami-e2ee/src/authenticated/receipts.rs))

Introduced in KAMI-25 to coordinate inbound message delivery and staged `DeviceDelivered` receipt frame generation across double-ratchet boundaries:

- **Struct**: `AuthenticatedDelivery`
  - Encapsulates decrypted inbound message facts, plaintext, delivery artifact, and the staged outbound receipt frame.
  - Generated via `QuarantinedSession::prepare_delivery` (or `QuarantinedSession::inbound_delivery` for first-message bootstrap).
  - Advances session generation by **2** (1 generation for the inbound message decryption + 1 generation for the outbound receipt generation in the reverse direction of the same session).
- **Struct**: `AuthenticatedReceiptCommit`
  - Represents an opaque, unforgeable commitment for an inbound receipt frame.
  - Produced via `QuarantinedSession::prepare_receipt` on the sender side when consuming an inbound `DeviceDelivered` receipt.
  - Advances session generation by **1** (consuming the inbound receipt message in the ratchet).
  - Binds the exact target `MessageId`, target `ConversationId`, sender and recipient device keys, message facts, and SHA-256 text frame digest with a canonical `KAMI-DEVICE-DELIVERED-V1\0` payload.
- **Struct**: `ReplayPermit`
  - An internal non-forgeable capability token produced during context validation (`ReceiveContext::replay_permit` / `Coordinator::replay_permit`).
  - Grants storage workers permission to look up and recover previously committed deliveries or receipts without bypassing authenticated entry points.

---

### 9. Module `persistence` (Private, [crates/kami-e2ee/src/persistence.rs](file:///home/phu/dev/kami/crates/kami-e2ee/src/persistence.rs))

- **Struct**: `AccountSnapshot` & `SessionSnapshot` (WP3)
  - **What they represent**: Bounded, versioned binary snapshots of Olm accounts and sessions.
  - **Header Structure**: Exactly **66 bytes** (`HEADER`):
    - Magic `b"KAMIWP3\0"`: 8 bytes
    - DTO Version: 1 byte (`1`)
    - Provider Kind: 1 byte (`1` for Account, `2` for Session)
    - Session Generation `u64`: 8 bytes
    - Account Generation `u64`: 8 bytes
    - Context Digest `[u8; 32]`: 32 bytes
    - Initiator flag `u8`: 1 byte (`0` or `1`)
    - Pickle Length `u32`: 4 bytes
    - Reserved profile fields `[u8; 3]`: 3 bytes (must remain zero)
  - **Anti-Rollback**: Restoring a session checks an in-process SHA-256 digest. Database records alone cannot serve as an external rollback anchor (`RestoreAuthority::ProfileRestore` returns `RollbackAnchorUnsupported`).

---

### 10. Module `errors` ([crates/kami-e2ee/src/errors.rs](file:///home/phu/dev/kami/crates/kami-e2ee/src/errors.rs))

- **Enum**: `Error`
  - **Variants**: `AadUnsupported`, `AuthenticationRejected`, `StateVersion`, `StateRollback`, `StateEncoding`, `InvalidContext`, `Provider`.
  - **Security Sensitivity**: All failures in the authenticated envelope boundary collapse to `AuthenticationRejected` ("authenticated envelope rejected"), leaking zero cryptographic internals or provider error strings.

---

## Completion and Verification

- **Milestone 1 Completion Estimate**: **Not estimable**
  - *Milestone Status*: **Quarantined Foundation Accepted with Conditions** (WP1, WP2-A2, WP3, WP2-B, and KAMI-25 receipt boundary implemented and owner accepted within quarantined scope; live transport provenance, prekey publication/replenishment lifecycle, and external anti-rollback anchors remain deferred).
- **Whole-Project Completion Estimate**: **Not estimable**
- **Status Breakdown**:
  - **Implemented Behavior**: Vodozemac 0.11.1 adapter, fail-closed AAD, canonical 64 KB inner envelope, staged outbound/inbound text, same-session receipt generation and consumption, OTK and fallback prekey accounting, exact retransmission reconciliation, and opaque commit handoff to storage.
  - **Tested Behavior**: Unit and qualification suites (`envelope.rs`, `authenticated.rs`, `tests/qualification/receipts.rs`), external privacy probe (`tests/privacy.rs`), and cross-crate integration with `kami-storage`, `kami-sender`, and `kami-receiver`.
  - **Missing Behavior**: Production prekey replenishment/publication server integration, live peer transport provenance, and persistent external anti-rollback anchors.
  - **Known Limitations & Provider Constraints**:
    - **Upstream Ratchet Limits**: Upstream `vodozemac = 0.11.1` Olm v1 pins a **40-key retention cap for skipped keys** and a **2,000-message gap ceiling** on the receiver chain. These are upstream implementation constraints, not configurable Kami settings.
    - **Truncated MAC**: Olm v1 uses an 8-byte truncated MAC.
    - **No Wire Interoperability**: Does not support Signal or Matrix wire interoperability.

### Exact Verification Commands

```sh
# Run all unit and integration tests in kami-e2ee
cargo test --locked -p kami-e2ee

# Run internal qualification test modules
cargo test --locked -p kami-e2ee --lib envelope_tests
cargo test --locked -p kami-e2ee --lib authenticated_tests
cargo test --locked -p kami-e2ee --test receipts

# Run the external privacy and boundary probe
cargo test --locked -p kami-e2ee --test privacy

# Run joint storage and E2EE integration tests
cargo test --locked -p kami-storage -p kami-e2ee
```
