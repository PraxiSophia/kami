# kami-protocol

Bounded M1 **structural** Protobuf codec. It depends on `kami-core` and pinned
`prost` 0.14.4 (Apache-2.0, upstream MSRV 1.85). Verified here with Rust/Cargo
1.99.1 on Linux; Windows/macOS and older toolchains are not qualified by this work.
`proptest` is test-only and reuses the core crate's std-only feature selection.

## Interfaces and modules

- `StructurallyValidFrame::new(FrameInput<'_>)` checks borrowed outbound input
  before copying it. Controlled read-only accessors expose core IDs, kind, sequence,
  display timestamp, protected bytes, context material, and optional receipt target.
- `encode_frame(&StructurallyValidFrame) -> Result<Vec<u8>, ProtocolError>` checks
  prost's encoded length before allocating output.
- `decode_frame(&[u8]) -> Result<StructurallyValidFrame, ProtocolError>` checks the
  complete input length, then performs an allocation-free whitelist/structural scan
  before prost decoding can allocate.
- `validate_for_context(&frame, &ExpectedContext)` compares message/conversation,
  sender/recipient and receipt-target IDs to caller-supplied expectations. It is
  **not authentication**, does not verify the opaque context, and does not mint
  `MessageFacts` or `ReceiptEvidence`.
- `supported_versions()` advertises only `[1]`; no implicit downgrade. All
  mandatory feature bits are unsupported (the field must be absent).

`lib.rs` retains the original M1 version/kind and ID/sequence exports. Private
`frame.rs` owns typed construction/access/context comparisons, `codec.rs` the
bounded public codec, `scan.rs` the strict profile scanner and field caps,
`error.rs` safe error classes, and `wire.rs` private prost derives.
`proto/kami/m1/frame.proto` specifies tags, types, kind discriminants and reserved
numbers. The derives are maintained alongside that schema; no codegen or `protoc`
is needed to build. Fixed golden vectors pin the actual representation. Changing
the schema requires updating the derives, scanner, validation and vectors together.

## M1 structural profile

This is a flat endpoint structural representation, not an approved relay-visible
routing envelope or final E2EE format. Do not expose its metadata wholesale to
relays. The security integration must finalize encrypted/private field placement,
exact authenticated bytes/AAD, context format and retransmission stability before
network use. Nothing in this crate proves bytes are encrypted or authentic.

| Tags | Fields | Constraints |
|---|---|---|
| 1, 2 | version, kind | explicit presence; version 1, text=1 or device-delivered=2 |
| 3, 4 | logical message, conversation IDs | exactly 16 bytes, big-endian core u128 |
| 5, 6 | sender, recipient device references | exactly 32 opaque bytes |
| 7 | sender sequence | present and positive for text, forbidden for receipts |
| 8 | created-at Unix milliseconds | explicit presence, 0..253402300799999 (through year 9999); display only |
| 9 | protected payload | nonempty; at most 65536 bytes, further reduced by total frame cap |
| 10 | authentication/context material | 16..1024 opaque bytes; length is a structural profile, not a crypto algorithm |
| 11 | target logical message ID | exactly 16 bytes for receipts, forbidden for text |
| 12 | mandatory features | absent; nonzero values return `UnsupportedFeature`, explicit zero is noncanonical |

All fields except 7/11 (kind-dependent) and 12 are required. Core ID constructors
are used only after wire length checks; these IDs convey no identity verification
or uniqueness/generation guarantees. There are no strings: payload/context remain
arbitrary binary, including invalid UTF-8. UTF-8 text validation belongs **after
authenticated decryption** (and before encryption on the sender), outside this codec.

Canonical Protobuf is stricter than ordinary Protobuf:

- Ascending field tags, explicit presence even for required zero-valued timestamps,
  minimal key/value/length varints, correct wire types and exact lengths.
- Unknown fields (including reserved tags), duplicates, reordering, groups,
  overflows, truncations and nonminimal encodings fail closed. No unknown-field
  discard, repeated/map fields or recursive messages.
- Unsupported versions fail explicitly; unknown security-critical extensions
  require a negotiated new profile/version rather than silent same-version parsing.
- Fixed-order prost encoding and the scanner's minimal-varint/presence rules give
  one accepted representation per typed frame. Golden/property tests enforce exact
  byte equality after decoding and re-encoding; no extra re-encode allocation is
  necessary on ingress.

`ProtocolError` and `Field` contain only fixed safe enum classes, never raw values,
IDs, payload dumps or prost decoder errors/source chains. Frame `Debug` is redacted;
input/expected-context types deliberately do not implement `Debug`. Explicit byte
accessors remain sensitive: callers must not log them. Cloning a validated frame
copies bounded data; this crate does not implement secure erasure or key custody.

## Resource and trust boundaries

`MAX_ENCODED_FRAME_SIZE` is **65,536 total Protobuf bytes**, including envelope
fields; it is not an application plaintext allowance. There is no outer length
prefix in this API. A future transport must reserve any additional prefix/envelope
within the same network cap, and read into a fixed/bounded buffer (at most cap+1)
before growing or allocating from an untrusted length. A slice API cannot undo an
allocation already performed by its caller.

All malformed/oversized inputs are scanned/rejected without heap allocation before
prost. Accepted inputs contain at most seven bounded byte fields; prost 0.14.4's
optional-Vec decode uses temporary Bytes copies. The isolated allocation probe
asserts at most 14 allocations and at most 131072 additional heap bytes (total and
peak) for measured valid decoding, excluding the caller's input/existing frames,
stack and allocator bookkeeping. It asserts zero allocations for oversize,
duplicate and late-invalid full-size input, and outbound oversize rejection.
Measured Linux release results for a 65536-byte text frame:

- Decode: 12 allocations, 131028 bytes allocated in total, 130900 peak live bytes.
- Encode: one allocation, 65536 total/peak bytes.
- Oversize/late-invalid/duplicate ingress and oversize construction: zero allocations.

These are measured cases plus flat-schema bounds, not an exhaustive allocator,
cryptographic, denial-of-service or platform audit. The probe is separate because
`allocation-counter` installs an allocator; it must not affect production binaries
or add non-Protobuf dependencies to the workspace lockfile.

No identity/Peer-ID verification, E2EE/session/Double Ratchet, persistence, transport,
relay, retry, durability, delivery-state transitions or receipts-as-delivery-proof
are implemented. A later security adapter must verify immutable wire bytes and bind
payload/context/participants before making any domain acceptance/delivery decision.

## Manual verification (repository root)

```sh
cargo test -p kami-protocol
cargo check --workspace
cargo build --workspace
cargo test --workspace
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

`tests/vectors.rs` covers fixed text/receipt hex vectors; `malformed.rs` covers
structural/wire failures and safe diagnostics; `context.rs` outbound/context checks;
`resource_bounds.rs` exact cap, cap+1 and truncation/hostile inputs;
`properties.rs` runs 256 bounded cases per property for raw fuzz-oriented no-panic,
valid typed roundtrip and golden-byte mutation. This is not a sustained libFuzzer run.

Allocation measurement (Python 3, Cargo, and cached/network access to
`allocation-counter` 0.8.1; explicitly set `TMPDIR` to an existing scratch directory):

```sh
python3 crates/kami-protocol/tests/resource_probe.py
rustfmt --edition 2021 --check crates/kami-protocol/tests/allocation_probe/probe.rs
```

The Python runner creates/removes an isolated Cargo manifest/lockfile/target under
`TMPDIR`, using the checked-in safe probe source. It exits nonzero on failed bounds
or Cargo failures. No unsafe code is used in this crate or probe; allocator internals
belong to the external measurement library. Cargo's normal cache may be populated.
