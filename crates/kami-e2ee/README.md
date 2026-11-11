# kami-e2ee

KAMI-51 provider/profile foundation (WP1), the accepted-with-limitations WP-A
authenticated-context/provider-contract correction, quarantined WP2-A2 authenticated
inner envelopes, bounded WP3 snapshots, and **WP2-B typed storage composition**, integrating the
[kami-identity WP2-A1 signed account/prekey binding](../kami-identity/README.md).
**Production messaging is not enabled; the general provider's encryption/decryption
remain fail-closed.** The separate public `QuarantinedSession` API is for bounded
in-process staging/qualification, not public/production messaging. These boundaries do not establish KAMI-51/M1 completion,
Signal/Matrix interoperability, or production security qualification.

## Implementation and qualification status

The KAMI-51 WP1 -> WP2-A1 -> WP2-A2 -> WP3 -> quarantined WP2-B -> WP-A Rust boundaries are
implemented in this checkout. WP3 is accepted as a bounded private storage
foundation. WP2-B has conditional owner acceptance only within its quarantined
scope; its implementation and independent QA/Evaluator reviews reported PASS. WP-A
is accepted with limitations after implementation run 187, independent QA run 188,
and final security evaluation run 189; its acceptance checkpoint is Kanban comment
176. This does not enable production messaging, establish live network provenance,
or complete KAMI-51/M1. Jira status is authoritative for the issue record and is not
changed by this documentation.

## WP-A provider and API boundary

The pinned `vodozemac = 0.11.1` adapter implements the explicit `NativeAadProvider`
contract and remains compatible with the broader `E2eeProvider` interface. Because this
provider does not supply native application AAD, `supports_aad()` remains `false` and
public provider encryption/decryption returns `AadUnsupported` after applicable input
validation. The quarantined authenticated composition is therefore a separate contract;
it must not be described as native provider AAD or used to enable the general production
API.

WP-A's authenticated path encrypts a fixed-order canonical `InnerEnvelopeV1` inside
Olm, then performs post-decryption checks for the exact context, identities, direction,
proof digests, message kind, sequence/receipt target, timestamp, and outer frame
correspondence. The outer SHA-256 is only a correspondence commitment; authenticity is
provided by the verified Olm session and anchored checks. Staged results, proposed state,
and exact-frame retry are distinct from durable commit or delivery authority. Private
unbound helpers exist only for test fixtures and negative-security tests; they are not
exported production bypasses.

The authenticated API checks the supplied trust, end-peer, proof, and clock context, but
the pure boundary cannot prove that a supplied snapshot is live or fresh. Runtime/storage
must provide live end-peer provenance, serialize trust decisions with use, and enforce
durable state custody. The accepted in-process snapshot custodian can technically consume
full provider pickles, including private prekeys; this is a trusted-custody limitation,
not protection from a malicious custodian, native/allocator copies, dumps, or endpoint
compromise. `ProfileRestore` remains fail-closed because no external rollback anchor is
available.

## Modules and interfaces

- `types`: bounded, validated canonical `ApplicationContext`, reusing `kami-core`
  `MessageId`, `ConversationId`, and `DeviceId`; no duplicate domain IDs.
- `profile`: versioned context metadata. The previous `max_skipped_keys = 64` and
  `max_prekey_batch = 100` settings/constants were removed because this adapter
  never enforced them. Profile encoding does not include those fields.
- `state`: `StateEnvelope` with V1 version, generation and opaque provider bytes.
  Decode requires **exact** expected generation, rejecting stale and future values;
  malformed lengths, trailing bytes, unsupported versions and oversized state fail.
  `StateRollback` names either direction of generation mismatch. The provider-byte
  ceiling is 1 MiB, excluding the header.
- `provider`: explicit `NativeAadProvider` contract and the `E2eeProvider` compatibility
  alias. A staged value is not a commit; the native provider capability remains
  unavailable for vodozemac.
- `vodozemac`: pinned `vodozemac = 0.11.1` adapter; the local session supplied to
  `new`/`with_profile` anchors generation zero. Before restoring a directly
  constructed envelope, it checks version, exact generation, size and exact
  canonical session bytes. Same-counter alternate session/ratchet states are
  rejected. Empty provider bytes are only an initial generation-zero sentinel.
  There is **no public durable-state import, advancement/commit or raw
  `into_session` API**. Higher-generation anchors in unit tests are test-only
  simulations, not implemented persistence integration.
- `envelope` (**private**): canonical bounded `InnerEnvelopeV1`, exact outer
  commitment, and correspondence with existing `kami-protocol` frames.
- `authenticated` (**private**): `Endpoint`/`Anchor`, `BoundSession`, and
  `SendStage`/`ReceiveStage`/`InboundStage` qualification contracts. Checks WP2-A1
  proofs, trust and end-peer identity, then stages upstream Olm state effects.
  The module stays private; `Endpoint`, `QuarantinedSession`, `AuthenticatedCommit`
  and opaque consuming custodian types are selectively re-exported for WP2-B.
  Internal stages, anchors, snapshots and raw-session access remain private;
  this does not enable the general provider API.
- `persistence` (**private**): versioned bounded account/prekey and session
  snapshots, in-process digest checks and unsupported profile-restore refusal.
- `errors`: stable redacted error categories; provider/library diagnostics are
  not forwarded. The authenticated boundary uses uniform `AuthenticationRejected`.

Dependencies are `kami-core`, `kami-identity`, `kami-protocol`, `libp2p-identity`
(identity encodings only), `sha2`, `zeroize`, exact-version `vodozemac`,
`serde`/`serde_json`, and `thiserror`. There is no storage, network runtime,
transport, relay or IPC implementation here.

## Public mechanical fail-closed boundary (WP1)

Olm in this version has no native application AAD interface. Deterministic context
encoding does **not** authenticate logical IDs, conversation or participants.
Consequently **every public `VodozemacProvider` encrypt/decrypt call rejects with
`AadUnsupported` after applicable validation**; invalid input may return a
validation error first. `supports_aad()` is false. No public call releases unbound
ciphertext, plaintext or staged state; there is no caller-controlled opt-out.
Merely changing a context field cannot turn a rejection into success. WP2-A2 does
not flip this flag or activate the public API.

Private unbound staging helpers preserve the foundation's crypto code and allow
fixed-fixture tests, but are compiled only for test builds and are not exported.
They restore a copy, leaving the anchor unchanged, and return a checked next
generation. Feeding that candidate back to the old anchor is rejected; only tests
simulate installing a committed anchor.
This is **not** crash safety, durable anti-rollback, atomic CAS or a multi-writer
commit mechanism. A caller-selected expected generation alone cannot authenticate
a stored envelope; trusted durable state and rollback policy remain future work.

## WP-A accepted provider and context contract

WP-A retains vodozemac `0.11.1` and makes the provider boundary explicit without
claiming native application AAD. `NativeAadProvider` is the named native-AAD
contract; `E2eeProvider` remains a compatibility alias for existing callers. The
vodozemac implementation reports `supports_aad() == false` and returns
`AadUnsupported` for valid public native-AAD operations. It does not fall back to
unauthenticated context handling.

The application-bound qualification path serializes canonical `InnerEnvelopeV1` as
the exact plaintext authenticated by the Olm session. After decryption, Kami
canonical-decodes the inner envelope and checks its context, session, direction,
devices, Peer IDs, signed-binding digests, and duplicated outer fields. The outer
SHA-256 is recomputed as a correspondence commitment; it is not a standalone MAC
and must not be described as one.

The public production provider remains fail-closed. `BoundSession`, envelope
internals, staging anchors, snapshots, and raw-session operations remain private or
restricted. Unbound provider helpers are test-only. Staged operations clone state
and do not advance the source until finalization; rejected operations preserve the
source state, and retry reuses the exact previously prepared frame rather than
re-encrypting or ratcheting.

The focused WP-A tests cover context and outer/inner substitution, identity/device/
Peer-ID/binding mismatches, cross-session substitution, replay, malformed and
noncanonical input, frame limits, stale trust at finalization/retry, state
immutability, provider AAD behavior, and public API privacy. They do not establish
live network provenance or production qualification.

Values such as `ExpectedContext`, `ContactTrust`, timestamps, and Peer IDs supplied
to this E2EE boundary are structural assertions, not independently authenticated
runtime facts. The E2EE crate does not yet consume the producer-owned KAMI-27
authenticated transport capability, so runtime integration must provide
nonforgeable connection provenance and serialized trust/freshness checks before
production activation.

The existing trusted in-process custodian can consume full provider/account/session
snapshots. WP-A adds no new export, but this remains a custody limitation: no total
private-material non-exportability or protection from a malicious legitimate
custodian is claimed. Prekey lifecycle, durable recovery and rollback, integrated
qualification, application delivery, and production E2EE remain disabled.

The WP1 ciphertext JSON input ceiling is 65,536 bytes **before provider parsing**
in both public entry and private helper; its private plaintext staging ceiling is
also 65,536 bytes. Those are not full network-frame guarantees or usable text
budgets. WP2-A2 uses a separate canonical binary provider-message representation
and full encoded-frame accounting, described below; the old JSON contract is
unchanged.

## WP2-A2 canonical authenticated inner envelope

[`src/envelope.rs`](src/envelope.rs) defines a fixed-order plaintext encrypted
inside Olm. The exact encoding is:

```text
b"KAMI-INNER-V1\0" || envelope_version:u8=1 || olm_profile:u16be=1 ||
kind:u8 || direction:u8 || conversation_id[16] || message_id[16] ||
sender_device[32] || recipient_device[32] || session_id[43] ||
(sequence:u64be for text OR receipt_target[16] for receipt) || created_at_ms:u64be ||
sender_peer_len:u16be || sender_PeerId_bytes ||
recipient_peer_len:u16be || recipient_PeerId_bytes ||
sender_binding_digest[32] || recipient_binding_digest[32] ||
payload_len:u32be || payload
```

Conversation/message/receipt IDs use the existing big-endian ID encodings.
`Text = 1`, `DeviceDelivered = 2`; directions are initiator-to-responder `1` and
responder-to-initiator `2`. Session ID is the canonical 43-byte unpadded base64
provider session ID, including canonical unused bits. PeerIds must round-trip
canonically and each is at most 128 bytes. Each binding digest is SHA-256 of the
**exact signed WP2-A1 proof bytes**, including its signature, for that endpoint.
No recipient-local trust generation is sent as a shared wire counter.

There are no optional/repeated/unknown fields or trailing bytes. Decode checks
bounds and exact canonical re-encoding. Text requires a positive sender sequence,
nonempty UTF-8 payload and no receipt target. A receipt requires a nonzero target,
no sequence and either the historical empty qualification payload or the dedicated
bounded KAMI-25 receipt payload described below. Empty historical receipts cannot
finalize through the dedicated receipt commit path. Context IDs/devices must be nonzero, sender and
recipient distinct, binding digests nonzero, and timestamp within the existing
protocol range (at most `253_402_300_799_999` milliseconds). A timestamp is not a
freshness or delivery oracle.

Outer `authentication_context` is **exactly 32 bytes**:

```text
SHA-256(b"KAMI-INNER-CONTEXT-V1\0" || canonical_inner_bytes)
```

This hash is a correspondence commitment, **not standalone authentication**.
Authenticity comes from the verified Olm session and subsequent anchored checks.
After staged decryption, acceptance checks message ID, conversation, sender,
recipient, kind, sequence or receipt target, timestamp and this exact commitment
against the outer frame. Separate anchor checks enforce session/profile,
direction, both devices, both PeerIds and both signed-proof digests. A valid Olm
MAC alone cannot bypass these context checks.

The protected provider payload is one type byte (`0` prekey, `1` normal), followed
by the binary bytes from upstream `OlmMessage::to_parts`. Parsing via `from_parts`
and exact re-encoding rejects ambiguous provider encodings. This wraps upstream
cryptography unchanged; it is neither a custom primitive nor a Signal/Matrix
wire-compatibility promise.

## WP2-A1 trust integration and staged state

[`src/authenticated.rs`](src/authenticated.rs) verifies both endpoints' signed
WP2-A1 proofs against their exact retained trusted selections and Peer-ID proofs,
checks the local account's public keys, and uses `verify_olm_binding` for the
remote endpoint. Every operation requires the anchored recipient-local
`ContactTrust` generation, verified intended device, actual authenticated
**end-peer** PeerId and currently valid signed bundles. Publisher binding/bundle
generations and recipient-local trust generations remain separate concepts.
A relay identity, indirectly obtained signed prekey, or parsed invitation is not
live end-peer authentication.

Send/receive clone upstream Session pickles in memory. First-message inbound
handling clones the Account too, checks the selected sender account and recipient
prekey, and stages account/session/prekey effects together. The actual stored
one-time-key inventory delta, including published keys, must match the selected
kind: one key consumed for `OneTime`, none for `Fallback`. Repeated PreKey messages
must match the existing anchored session's `session_keys`, not create another
inbound account session. Selection/reference freshness and prekey publication,
rotation and reuse policy remain trusted caller responsibilities.

Only fully authenticated, validated results return candidate state and accepted
inner plaintext. Any rejection returns no plaintext or candidate state and leaves
the source account/session unchanged. WP2-A2 tests adopt candidates only in memory;
WP2-B below adds typed durable composition, not a runtime sender. The service must
supply fresh trust, serialize its checks with use, authenticate transport provenance,
and confirm durable state before transmission or delivery ACKs. A caller can pass a
stale snapshot matching an old anchor; the pure boundary cannot prove freshness
or implement a service race guard by itself.

Authenticated failures collapse to `AuthenticationRejected` (display:
`authenticated envelope rejected`), with no raw provider diagnostics. Inner Debug
is redacted; staged key/session containers have no Debug/serialization interface.
Temporary decrypted/canonical buffers use zeroizing wrappers; inner payloads
zeroize on drop. This does not guarantee erasure of every upstream/native copy,
allocator artifact, dump or compromised endpoint memory.

## Size limits

The entire canonical inner envelope is bounded to **65,536 bytes**, not just its
text. Independently, existing `kami-protocol` construction/encoding enforces
**65,536 bytes for the complete encoded Protobuf frame**, including ciphertext,
provider overhead, commitment and metadata. Receive scans the full encoded size
before decode. A valid inner can still fail the outer budget; usable text is
smaller and depends on actual encoding/overhead, not a fixed 64-KiB text allowance.
Qualification tests construct exact 65,536-byte accepted and 65,537-byte rejected
frames for both prekey and normal messages, on send and receive.

## Verification and fixtures

`tests/qualification/envelope.rs` and `authenticated.rs` are included as crate
unit-test modules to exercise private APIs. The latter includes `adversarial.rs`
and `frame_limits.rs`; these are **not** standalone Cargo integration-test targets.
Tests cover canonical roundtrips/commitment, malformed and hostile lengths,
outer/inner substitutions, trust/peer/bundle/account mismatch, staged rejection
without source mutation, one-time/fallback and published-key handling, repeated
prekey routing, in-memory replay rejection, receipts and exact frame limits.
Tests named direct/relay supply end-peer identities at the API boundary; they do
**not** execute real direct or relayed network connections. Receipt tests prove
context binding; the dedicated KAMI-25 tests below additionally exercise receipt
staging. Neither set alone proves durable delivery or ACK eligibility.

`tests/fixtures/olm-v1.json` contains **synthetic test-only secrets**, fixed
sender/receiver pickles before/after a message, ciphertext, plaintext, session ID
and complete state-envelope bytes. It was captured once from a local Olm v1
exchange using vodozemac 0.11.1, not recomputed during tests. It is a regression
fixture, **not an independent interoperability/security vector**. Foundation tests
compare exact ciphertext/state bytes, staged immutability, simulated commit/replay
rejection, stale/future/fork rejection, overflow, malformed/tampered/oversized
input and public fail-closed context behavior. Compile-fail tests cover removed
bypass and unsupported profile knobs. Context/profile tests assert a fixed layout.
Dynamic upstream prekey/reorder smoke tests are supplemental, not limit proofs.

Run from repository root (verified on this Linux checkout):

```sh
cargo test --locked -p kami-identity --test olm_binding
cargo test --locked -p kami-e2ee
cargo test --locked -p kami-e2ee --lib envelope_tests
cargo test --locked -p kami-e2ee --lib authenticated_tests
cargo test --locked -p kami-e2ee -- --list
cargo fmt --all -- --check
cargo check --locked --workspace
cargo clippy --workspace --all-targets --locked -- -D warnings
git diff --check
```

## WP3 private persistence boundary

`persistence` adds non-exported versioned account/session snapshots. Account
pickles include upstream one-time/fallback prekey inventory; session snapshots
retain the initiator role, exact session/profile, account/session generations and
an anchored context digest. The binary header is 66 bytes: `KAMIWP3\0`, version 1,
account/session kind, two positive i64-range generations, 32-byte context digest,
role, u32 pickle length and three zero reserved bytes. Provider pickle bytes are
bounded to 1 MiB before JSON parsing; header, length, version, kind and trailing
bytes are checked. SQL storage independently caps the **whole blob** at 1 MiB,
so a candidate near that provider ceiling may be rejected by storage.

Snapshots and temporary serialized buffers are zeroizing and have no Debug or
serde interface. WP2-B permits consuming full-snapshot export only from a legitimate
opaque authenticated candidate (see custodian limits below). Restore checks a separately retained
in-process SHA-256 digest, exact generations/context, local account keys, Olm-v1
profile, session ID and fresh trust/proofs/end-peer before yielding a session.
A digest authenticates a candidate only relative to that retained authority.
**DB-only counters/digests cannot detect coherent old DB/WAL rollback.**
`RestoreAuthority::ProfileRestore` explicitly returns `RollbackAnchorUnsupported`;
there is no supported persistent external rollback anchor or automatic restart
activation. A copied database's own digest must never be used as in-process
restore authority.

The [storage seam](../kami-storage/README.md#wp2-b-typed-authenticated-commit)
implements single-worker atomic account/session/message/frame writes, CAS, trust
checks and exact retry reconciliation. WP2-B composes real authenticated stages
with it; the older WP3 subprocess crash fixtures still use neutral synthetic
bytes. No runtime or production adapter is enabled. Provider tests round-trip
account/prekey consumption and progressed sessions, reject replay after restore,
malformed headers, stale generations and same-counter substitutions, and prove
coherent old snapshots still fail profile restoration. Run:

```sh
cargo test --locked -p kami-e2ee --lib wp3_
cargo test --locked -p kami-storage --lib wp3_
cargo test --locked -p kami-storage -p kami-e2ee
```

## WP2-B bounded typed composition

The acyclic graph is `kami-storage -> kami-e2ee`; E2EE never imports storage.
`Endpoint::new` accepts **untrusted** proofs and retained selections, not a trusted
tag. `QuarantinedSession::outbound` and `inbound` validate initial accounts;
`prepare_text` and `prepare_receive` return opaque `AuthenticatedCommit` plus
proposed in-memory state. Each operation verifies current supplied `ContactTrust`,
actual supplied end-peer and clock. `prepare_retry` revalidates retained stage-owned
evidence and yields the same candidate bytes without encryption or ratcheting.
Stage finalization revalidates trust/proofs/peer and source branch; facts and
snapshots derive only from authenticated inner/session/account state. Original
inbound frame bytes and generated outbound bytes are retained exactly.

Only `Storage::commit_authenticated` accepts a candidate. It returns exact bytes
only after durable commit or exact-frame reconciliation. Install proposed state
only after confirmation; on failure discard tentative advancement, retaining its
stage evidence solely for an exact retry if appropriate. On an ambiguous reply,
keep the proposed value and use `prepare_retry` with fresh trust/end-peer to
reconcile through the same typed storage operation, never re-encrypt. Retain the
relevant proposal while its retry is outstanding; each proposal retains evidence
for just its own last stage, not an unbounded retry history. No separate public
neutral retry, account-load or raw snapshot-import path exists. First account bootstrap and OTK
consumption are in the same IMMEDIATE transaction as session/message/frame;
fallback does not advance the account. Initial account generation is 1; the bounded
API does not import existing accounts or replenish/publish/rotate prekeys.

`AuthenticatedCommit`, `CommitMaterial` and `SnapshotTransfer` have private fields,
no neutral constructors, Clone/Default/Debug/serde. Consuming
`into_material().into_snapshots().consume(...)` is an explicitly accepted trusted
in-process custodian transfer: bounded `Zeroizing<Vec<u8>>` pickles include private
provider/prekey material. A legitimate holder **can capture that material**;
callbacks/ownership/redaction are not access control against a malicious custodian.
There are no individual private-key getters. Do not send these values to IPC,
CLI/adapters or logs; do not claim comprehensive zeroization of provider, native,
allocator or error-path copies. SQLCipher/key-provider protects durable snapshots.

Ordinary text finalization rejects receipt kinds; KAMI-25 adds a separate typed
receipt path below. E2EE alone emits no ACK, changes no durable Delivered state,
and supplies no live transport provenance. KAMI-26 must enforce connection
provenance, retained bundle freshness and serialized trust-to-send/re-ACK checks.
`ProfileRestore` remains fail-closed. Public/production messaging is disabled.

`tests/privacy.rs` uses a positive external fixture and rustc diagnostic assertions
for private stages/fields, absent constructors/traits, and forbidden Debug/serde.
Storage's external fixture runs real outbound/first-inbound/normal-receive commits,
OTK/fallback semantics, exact retry after advancement/reopen, stale trust/CAS/quota,
write-phase faults and artifact scans. It uses Cargo-selected existing artifacts
without adding test dependencies. Run:

```sh
cargo test --locked -p kami-e2ee --test privacy
cargo test --locked -p kami-storage --test authenticated_boundary -- --nocapture
```

## KAMI-25 dedicated delivery and receipt boundary

[`src/authenticated/receipts.rs`](src/authenticated/receipts.rs) selectively exports
`AuthenticatedDelivery`, `AuthenticatedReceiptCommit`, and `ReplayPermit` through
the crate root; private fields prevent neutral construction. They are distinct
from ordinary text `AuthenticatedCommit` finalization.

- `QuarantinedSession::inbound_delivery` and `prepare_delivery` return an opaque
  `AuthenticatedDelivery` and proposed session. They stage authenticated inbound
  text and an outbound DeviceDelivered reply in the same bidirectional Olm
  session, consuming two session generations. `facts()` and `receipt_frame()`
  expose candidate data; `into_parts()` transfers the two typed stages. A staged
  frame is not ACK authority: use storage's atomic `commit_delivery` and durable
  readback before emission or installing advancement.
- `QuarantinedSession::prepare_receipt` consumes one session generation and returns
  `AuthenticatedReceiptCommit` plus a proposal. It authenticates the expected
  reverse-direction session, target ID/conversation/devices, complete target
  `MessageFacts`, and SHA-256 digest of the exact original text frame. Its payload
  uses `KAMI-DEVICE-DELIVERED-V1\0` plus the target facts, frame digest and issuer's
  positive local trust generation. Trust counters are local, not synchronized
  between endpoints. `facts()`, `target_frame_digest()`, and consuming
  `into_material()` are custodian interfaces, not neutral evidence constructors.
- `ReplayPermit::new(trust, end_peer, remote, local, conversation)` validates
  supplied current trust/Peer-ID admission context. It authorizes only lookup of
  **exact previously authenticated durable material**; it does not authenticate
  arbitrary new ciphertext. Storage rechecks durable trust/pin and retained
  frame/facts/account/session evidence. `into_context()` transfers admission
  fields, not a mutable cryptographic session.

The [receiver](../kami-receiver/README.md) composes text/receipt persistence;
the [sender](../kami-sender/README.md) validates and commits incoming receipts.
Exact replay can recover retained ACK/Delivered evidence after reopen without
re-ratcheting. Alternate ciphertext for the same logical content is rejected.
This does not enable fresh mutable profile restoration: `ProfileRestore` still
fails closed without a qualified external rollback anchor. Receipt/dedup evidence
is bounded, with no active cleanup/compaction; admission fails closed at capacity.
Live end-peer provenance remains caller/future-runtime supplied, not established
by a detached binding or `ReplayPermit`.

`tests/qualification/receipts.rs`, included by the authenticated unit-test module,
checks same-session bidirectional receipt staging, wrong target binding, replay
rejection, and continued reverse-direction text. It is not a standalone test target.
Commands from repository root (not rerun for this documentation-only update):

```sh
cargo test --locked -p kami-e2ee --lib authenticated_tests
cargo test --locked -p kami-receiver --test receive
```

No production provider activation, real-network/cross-platform qualification,
runtime ACK emission, or exactly-once terminal rendering is implied.

## Deferred work and qualification (WP4/WP5)

WP2-B supplies bounded typed composition, not production account custody/import,
publication/prekey lifecycle, trusted external rollback authority or retained
bundle-freshness policy. Opening storage is not permission to restore cryptography.

Also deferred: runtime/transport/relay integration and real end-peer provenance,
service trust-change race guards, production orchestration of the bounded
sender/receiver inbox/outbox, exact-frame dedup/retry/order/receipt and Delivered
commits now implemented by KAMI-24/KAMI-25, and integrated Linux/Windows/macOS and
production security qualification. Private storage tests do not activate these
capabilities. Staged candidates and receipt frames are neither commit nor delivery
evidence.

Only upstream Olm v1 is used (including its 8-byte truncated MAC), not experimental
v2. Skipped-key retention/gap limits belong to upstream, not configurable Kami
profile promises: the pinned receiver chain has a 40-key retention cap and a
2,000-message gap ceiling. These finite windows are not qualified against Kami's
durable retry/no-loss requirements; expiry/recovery policy remains unresolved.
No Kami prekey-batch provisioning limit is implemented. No Signal/Matrix
interoperability, network delivery, OS portability, production security or M1
completion is established by these local tests.
