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

## Completion and Verification

- **Milestone 1 Completion Estimate**: **95%**
- **Whole-Project Completion Estimate**: **Not estimable** (future milestones will add end-to-end encrypted envelope headers and multi-hop relay routing).
- **Status Breakdown**:
  - **Implemented Behavior**: Full M1 Protobuf wire format, zero-allocation pre-scan, strict canonical field ordering, exact 64 KB boundary check, redacted Debug formatting, and safe error classes.
  - **Tested Behavior**: Exhaustive unit tests, golden hex vectors, malformed wire fuzzing, and heap allocation count verification via `resource_probe.py`.
  - **Missing Behavior**: Wire integration with network transports (libp2p/QUIC/TCP), which belongs to transport layers.
  - **Known Limitations**: This codec validates *structure*, not *authenticity*. It does not decrypt payloads or verify cryptographic signatures (handled by the security/identity layer).

### Exact Verification Commands

```sh
# Run all tests in kami-protocol
cargo test -p kami-protocol

# Run strict formatting and Clippy checks
cargo fmt -p kami-protocol -- --check
cargo clippy -p kami-protocol --all-targets --all-features -- -D warnings

# Run allocation and memory probe (verifies zero allocations on malformed inputs)
python3 crates/kami-protocol/tests/resource_probe.py
```
