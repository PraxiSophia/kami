# kami-protocol

## Overview

- **Crate Name**: `kami-protocol`
- **Simple Purpose**: The language translator for network communication. It takes Kami's message structures and serializes them into compact binary packets (using Google's Protocol Buffers format) to send over the network, and parses incoming packets back into structured data.
- **Problem it Solves in Kami**: Attackers can send malformed or enormous packets over the internet to crash applications or exhaust memory (Denial of Service). `kami-protocol` scans incoming bytes *before* allocating memory on the heap and enforces strict size limits (maximum 65,536 bytes per frame).
- **Dependencies**: Depends internally on [[kami-core]] and externally on `prost` (version 0.14.4, a fast Rust Protobuf library). Uses `proptest` for test fuzzing.
- **Requirements & Jira Traceability**: Implements Milestone 1 wire format requirements (64 KB maximum frame size, strict ascending field order, version 1 validation, zero-allocation reject on malformed frames).
- **Git Context**: Branch `protocol-storage`, commit `732156f6f13cbbe4d897f663c1f2546182619704`.

---

## Source Code Explanation

Re-exported publicly from [lib.rs](file:///home/phu/dev/kami/crates/kami-protocol/src/lib.rs).

### 1. Module `frame` ([crates/kami-protocol/src/frame.rs](file:///home/phu/dev/kami/crates/kami-protocol/src/frame.rs))

Defines the safe, verified in-memory representation of a network packet.

- **Enum**: `FrameKind`
  - **Variants**: `Text = 1` (ordinary text message), `DeviceDelivered = 2` (delivery confirmation receipt).
  - **What it represents**: The type of message transmitted across the wire.

- **Struct**: `FrameInput<'a>`
  - **What it represents**: A borrowed, temporary structure used when preparing to construct a frame.
  - **Why Kami needs it**: Allows callers to pass slices of data without having to clone them before validation passes.
  - **Fields**: IDs (message, conversation, sender, recipient), sequence number, timestamp, payload slice, context material, optional target message ID.

- **Struct**: `StructurallyValidFrame`
  - **What it represents**: A validated packet ready to be sent or freshly received. Once created, you are guaranteed it conforms to all structural wire rules.
  - **Security Sensitivity**: `Debug` is manually implemented to redact sensitive payload bytes so message contents are never accidentally printed into log files.
  - **Methods**:
    - `pub fn new(input: FrameInput<'_>) -> Result<Self, ProtocolError>`
      - **What it does**: Validates that all fields meet Kami's strict profile (correct lengths, sequence positive for text, target ID present only for receipts, total size <= 65,536 bytes) and clones the verified bytes.
      - **Errors**: Returns `Err(ProtocolError)` describing which constraint failed.
      - **Status**: Implemented ([frame.rs#L55-L115](file:///home/phu/dev/kami/crates/kami-protocol/src/frame.rs#L55-L115)).
    - Read-only accessors: `message_id()`, `conversation_id()`, `sender()`, `recipient()`, `sequence()`, `created_at_ms()`, `protected_payload()`, `auth_context()`, `target_message_id()`.

- **Struct**: `ExpectedContext`
  - **What it represents**: Caller expectations when receiving a frame (e.g., expecting a message for conversation X from sender Y).
  - **Methods**:
    - `pub fn matches(&self, frame: &StructurallyValidFrame) -> bool`: Compares frame IDs against expected values.

---

### 2. Module `codec` ([crates/kami-protocol/src/codec.rs](file:///home/phu/dev/kami/crates/kami-protocol/src/codec.rs))

Contains the encoding and decoding logic.

- **Functions**:
  - `pub fn encode_frame(frame: &StructurallyValidFrame) -> Result<Vec<u8>, ProtocolError>`
    - **What it does**: Encodes a validated frame into a binary Protobuf byte vector. Checks that the total encoded length does not exceed 65,536 bytes before allocating the output buffer.
    - **Inputs & Outputs**: Borrows `StructurallyValidFrame`; returns `Ok(Vec<u8>)` or `Err(ProtocolError)`.
    - **Status**: Implemented ([codec.rs#L12-L21](file:///home/phu/dev/kami/crates/kami-protocol/src/codec.rs#L12-L21)).
  - `pub fn decode_frame(bytes: &[u8]) -> Result<StructurallyValidFrame, ProtocolError>`
    - **What it does**: Decodes raw wire bytes into a typed frame. First calls `scan_frame` (zero-allocation structural scan). If and only if the scan passes, it allocates and decodes with `prost`.
    - **Security Sensitivity**: Protects against memory-exhaustion attacks by verifying bounds and rejecting unexpected or oversized fields before heap allocation.
    - **Status**: Implemented ([codec.rs#L23-L45](file:///home/phu/dev/kami/crates/kami-protocol/src/codec.rs#L23-L45)).
  - `pub fn supported_versions() -> &'static [u32]`
    - **What it does**: Returns `&[1]`. Kami M1 strictly supports protocol version 1 only.

---

### 3. Module `scan` ([crates/kami-protocol/src/scan.rs](file:///home/phu/dev/kami/crates/kami-protocol/src/scan.rs))

A specialized, allocation-free Protobuf stream scanner.

- **Functions**:
  - `pub(crate) fn scan_frame(bytes: &[u8]) -> Result<(), ProtocolError>`
    - **What it does**: Walks through raw Protobuf wire varints and field tags. Ensures tags appear in strictly ascending order, validates field lengths, rejects duplicate fields or unknown tags, and ensures total length <= 65,536.
    - **Status**: Implemented ([scan.rs#L15-L110](file:///home/phu/dev/kami/crates/kami-protocol/src/scan.rs#L15-L110)).

---

### 4. Module `error` ([crates/kami-protocol/src/error.rs](file:///home/phu/dev/kami/crates/kami-protocol/src/error.rs))

- **Enum**: `ProtocolError`
  - **Variants**: `FrameTooLarge`, `Truncated`, `InvalidWireType`, `NonMinimalVarint`, `FieldOutOfOrder`, `DuplicateField`, `UnknownField`, `MissingField(Field)`, `InvalidFieldLength(Field)`, `InvalidFieldValue(Field)`, `UnsupportedVersion`, `UnsupportedFeature`.
  - **Security Sensitivity**: Contains only safe enum discriminants. Never includes raw byte slices, conversation IDs, or decrypted plaintext in error strings, preventing accidental log leaks.

---

---

### 5. Module `prekey_exchange` ([crates/kami-protocol/src/prekey_exchange.rs](file:///home/phu/dev/kami/crates/kami-protocol/src/prekey_exchange.rs))

Introduced in **KAMI-51 WP-B** to define wire framing for prekey exchange over `/kami/prekey-exchange/1`:

- **Protobuf Definition**: [`proto/kami/m1/prekey_exchange.proto`](file:///home/phu/dev/kami/crates/kami-protocol/proto/kami/m1/prekey_exchange.proto)
- **Message `Exchange`**:
  - `version`: Protocol version, strictly `1`.
  - `kind`: Exchange kind discriminator:
    - `1` = `Query` (request challenge + minimum bundle floor).
    - `2` = `Selection` (signed `OlmBindingProof` selection, 1..1024 bytes).
    - `3` = `PublicationAck` (retained confirmation with 32-byte SHA-256 response digest).
    - `4` = `AckResult` (reconciled confirmation from publisher).
    - `5` = `Error` (closed failure enum, absent sender/recipient device keys).
  - `request`: Exactly 32 nonzero challenge bytes.
  - `sender_device` & `recipient_device`: Exactly 32-byte Ed25519 public keys.
  - `bundle_generation`: Monotonic generation counter `<= i64::MAX`.
  - `account_generation`: Positive Olm account generation `<= i64::MAX` (Selection only).
  - `signed_selection`: Exactly one bounded `OlmBindingProof` (1..1024 bytes).
  - `response_digest`: Exactly 32-byte SHA-256 digest of the serialized Selection frame.
  - `failure`: Closed failure code enum: `InvalidVersion` (1), `InvalidKind` (2), `InvalidRequest` (3), `InvalidDevice` (4), `InvalidGeneration` (5), `InvalidSelection` (6), `InvalidDigest` (7), `InvalidError` (8), `Malformed` (9), `CapacityExceeded` (10), `InternalError` (11). Contains zero free-text strings.
- **Strict Validation Profile**:
  - Encoded length must fit within **16,380 bytes** (`MAX_BODY_BYTES`), yielding a 16 KiB total frame limit with its 4-byte length prefix.
  - Field tags must appear in strictly ascending order without unknown tags, duplicate entries, or non-minimal varints.

---

## Completion and Verification

- **Milestone 1 Completion Estimate**: **100% of wire frame scope**
- **Whole-Project Completion Estimate**: **Not estimable**
- **Status Breakdown**:
  - **Implemented Behavior**: Full M1 Protobuf wire format, zero-allocation pre-scan, strict canonical field ordering, exact 64 KB message boundary check, 16 KiB prekey exchange framing, redacted Debug formatting, and safe error classes.
  - **Tested Behavior**: Exhaustive unit tests, golden hex vectors, malformed wire fuzzing, heap allocation probe (`resource_probe.py`), and prekey exchange codec tests (`tests/prekey_exchange.rs`).
  - **Runtime & Transport Status**: Supervised within [[kami-transport]] `/kami/prekey-exchange/1` substream codec and [[kami-runtime]] engine actor.
  - **Missing Behavior**: Application message wire streaming (KAMI-28).

### Exact Verification Commands

```sh
# Run all tests in kami-protocol (including prekey exchange codec)
cargo test -p kami-protocol

# Run strict formatting and Clippy checks
cargo fmt -p kami-protocol -- --check
cargo clippy -p kami-protocol --all-targets --all-features -- -D warnings

# Run allocation and memory probe (verifies zero allocations on malformed inputs)
python3 crates/kami-protocol/tests/resource_probe.py
```

