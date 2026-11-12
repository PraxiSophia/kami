# kami-identity

Protected one-device **identity/key-custody and explicit contact-trust boundary**.
No transport, E2EE, SQL, or full profile enrollment. Production project dependency
is `kami-key-provider`; never depends on `kami-storage` or `kami-protocol`. Binding
uses `libp2p-identity = 0.3.0` (Ed25519/PeerId only); fingerprints use `sha2 = 0.11.1`.
These are cryptographic primitives/identity encodings, not a network runtime.

## Implementation and qualification status

KAMI-23 is functionally implemented in the current Rust tree. Its local identity,
custody, trust, fingerprint, Peer-ID binding, and WP2-A1 binding tests pass on the
tested Linux checkout, and KAMI-50's trust/binding scope has separate implementation
evidence. Jira last reported KAMI-23 as **In Progress** on 2026-11-03 because native
Windows/macOS and broader platform qualification remain incomplete; that workflow
status does not mean the core Rust implementation is unfinished. Independent review
and owner acceptance are separate claims and must not be inferred from Jira status.

Production readiness is not claimed. Native Windows/macOS behavior, Linux logged-in
secure-store qualification, cross-process enrollment/lease integration, live transport
Peer-ID provenance, and full M1 qualification remain pending.

## Modules and interfaces

- `store`: `SecretStore` injection seam; strict, versioned `KeySlot` handles with
  distinct `Identity` and `Database` namespaces. Handles round-trip through
  `to_persistent_string` / `from_persistent_string`; never log them. Adapter reads
  own zeroizing buffers and exact-length validation precedes use.
- `enrollment`: public-only `EnrollmentRecord` (two handles and Ed25519 public key)
  plus `EnrollmentState` and the service-owned `EnrollmentJournal` capability.
  Reconstruct records using checked `from_parts`; no secret is in the descriptor.
- `custody`: `Custody<S>` supplies explicit `initialize`, read-only `open_existing`,
  and the injected `KeyProvider::load_database_key` implementation. Generate an
  Ed25519 seed using OS CSPRNG `getrandom 0.4.3`, derive the public key with
  `ed25519-dalek 3.0.0` (zeroize enabled), and independently generate a random
  32-byte database key. No database key is derived from/reused as an identity key.
- `native`: explicitly target-selected `NativeStore`; no process-global keyring
  builder, in-memory fallback, plaintext credential file, reset or deletion API.

- `fingerprint`: validates canonical nonweak Ed25519 keys through dalek and
  derives `DeviceFingerprint` using SHA-256. `parse_display` accepts only the full
  canonical lowercase grouped comparison value, never an abbreviation.
- `binding`: checked `PeerBindingProof` parsing and `verify_peer_binding` require
  canonical libp2p transport-key/PeerId derivation, strict device signatures and
  the actual authenticated recipient PeerId. `Custody::create_peer_binding` signs
  only this fixed transcript after checking the protected seed against enrollment.
- `olm_binding`: WP2-A1 `OlmBindingData`, `OlmBindingProof`, `OlmPrekeyKind` and
  `verify_olm_binding` associate a device with selected public Olm account/prekey
  data. `Custody::create_olm_binding` and the native verified-enrollment methods
  below sign only this fixed purpose; neither exports the protected identity seed
  nor provides an arbitrary-signing API.
- `descriptor`: `Invitation` checks bounded public descriptors and self-signatures;
  every import remains untrusted. v1 has no names, addresses, relay hints or expiry;
  routing configuration belongs to transport composition and never conveys trust.
- `trust`: checked `ContactTrust` snapshots, `TrustTransition`, snapshot-bound
  `VerificationEvidence`, `ContactTrustRepository` and pure `send_decision`.
  No network assertion or valid self-signature promotes a contact automatically.

## Version-1 fingerprint and binding encoding

Fingerprint transcript is exactly `b"kami-device-fingerprint-v1\0" || key[32]`.
Display the entire SHA-256 digest as eight lowercase eight-hex-digit groups,
separated by one space. The version is carried by the descriptor/type contract.
RFC8032 public key
`d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a`
yields `126dea50 6043ab05 799ddbc6 de4d4e74 3a798401 ad1cfe32 5b4a9106 e6d84b67`.
Both RFC vectors were independently checked with shell byte escapes/sha256sum.

Binding signature transcript is exactly `b"kami-peer-binding-v1\0" ||
u16be(public_key_protobuf_len) || canonical_public_key_protobuf ||
u16be(peer_id_len) || PeerId::to_bytes()`. Wire encoding is byte `1`, those
length-delimited fields, then the 64-byte Ed25519 signature. Only canonical,
nonweak Ed25519 transport keys are supported in v1. Each field is at most 128
bytes, proof at most 325 bytes; trailing bytes and unknown versions fail closed.
`Invitation` wire is `b"KINV" || 1u8 || device_key[32] || u16be(proof_len) || proof`;
input is capped at 512 bytes before allocation. It contains only public data but
Debug/errors still redact it. Detached signature validity is **not** live transport
private-key possession, independent human verification, or E2EE.

## WP2-A1 signed identity-to-Olm binding

[`src/olm_binding.rs`](src/olm_binding.rs) implements a bounded detached device
signature over one selected Olm account/prekey bundle. The exact signed transcript
is fixed order (all integer encodings are big-endian):

```text
b"kami-olm-binding-v1\0" || version:u8=1 || device_key[32] ||
peer_len:u16 || canonical_PeerId_bytes || olm_ed25519[32] ||
olm_curve25519[32] || prekey_kind:u8 || reference_len:u16 || reference ||
prekey_curve25519[32] || binding_generation:u64 || bundle_generation:u64 ||
not_before_ms:u64 || expires_at_ms:u64
```

Wire bytes append the 64-byte Ed25519 signature; domain and version are signed.
There are no optional/repeated/unknown fields or trailing bytes. The canonical
PeerId encoding is bounded to 128 bytes; prekey references are opaque, nonempty
1–64-byte values, not normalized text/base64. `OneTime = 1` and `Fallback = 2`
are distinct signed purposes. `MAX_OLM_BINDING_BYTES` bounds parsing before
allocation. Device and Olm Ed25519 keys must be canonical and nonweak; Curve25519
keys are exact 32-byte provider encodings. DH validation/contribution and key
possession checks remain the provider's responsibility, not the signature's.

`Custody::create_olm_binding(record, peer_binding, data)` first verifies the
separate Peer-ID proof under the enrolled device key and matches its publisher
PeerId. It reads the protected seed, checks it against enrollment, and signs only
this transcript. Missing/locked/unavailable/corrupt custody fails without key
creation or replacement. Run it on the bounded blocking custody worker, outside
SQL transactions and network tasks. Debug output redacts public proof data;
errors do not expose native custody diagnostics. No private identity key is exported.

Publisher-owned `binding_generation` and `bundle_generation` are positive and
at most `i64::MAX`; they are **not** recipient-local `ContactTrust` generations.
Signed timestamps are at most `i64::MAX`; validity is exactly
`[not_before_ms, expires_at_ms)`, with a positive interval of at most seven days
and no implicit clock skew. The caller supplies time and retains a trusted exact
expected bundle selection. Never derive `expected` merely from the received
`proof.data()`: a valid signature does not establish freshness or reject rollback
by itself. `OlmBindingProof::verify` checks the pin, Peer-ID proof, signature,
exact expected fields and validity; parsing alone is not authentication.

For an operation involving a live peer, use `verify_olm_binding` with a **fresh**
verified `ContactTrust`, the immutable intended device key and the actual secure
connection's authenticated **end-peer** PeerId (not a relay identity/assertion).
It reuses the existing no-TOFU `send_decision` contract, so unknown, pending,
changed, blocked, unauthenticated and wrong-peer cases remain held. The service
must still serialize fresh trust/bundle checks with later use; this pure check
cannot prove snapshot freshness or close a concurrent trust-change race.

This is a detached public association, not live DH possession, human verification,
prekey publication/consumption, revocation, durable freshness or E2EE activation.
The private [kami-e2ee WP2-A2 boundary](../kami-e2ee/README.md) consumes these
proofs in in-memory qualification. Durable session/prekey persistence, atomic
CAS/rollback handling, runtime/transport/delivery integration and cross-platform
production qualification remain WP3/WP4/WP5 work. Public encryption stays
fail-closed; production messaging is not enabled. Neither WP2 boundary establishes
Signal/Matrix interoperability or a production security claim.

## WP-B native per-selection signing

`NativeCustody::{create_olm_binding,create_olm_bindings}` require the existing
`VerifiedEnrollment`, exact fresh SQLCipher enrollment row and local peer binding.
The bounded batch accepts 1..=52 selections (50 OTKs plus current/previous fallback),
checks native enrollment/anchor before and after signing, and verifies the protected
seed against the enrolled device and Peer ID. Each selection gets an independent
existing `OlmBindingProof` signature, **not** an aggregate batch signature. Network
responses currently carry exactly one signed selection. Runtime performs these
blocking custody calls outside SQL transactions; no private identity key leaves
identity custody. Missing/locked/changed/unavailable custody fails closed.

Identity does not reserve/consume OTKs or authenticate a socket. WP-B runtime,
storage and transport compose those authorities separately, including current trust,
five-minute selection validity, durable generation/revocation floors and exact ACK
reconciliation. Detached signatures alone remain insufficient. WP-B implementation
`t_d364301b` run 204, QA `t_0dcc13ac` run 205 PASS and final Evaluator `t_250e6416`
run 206 PASS-WITH-CONDITIONS cover this bounded composition, not production E2EE,
external rollback resistance, cross-OS qualification or M1 completion.

## Trust, durability and send integration

Each endpoint independently compares the other endpoint's full fingerprint over
an already trusted external channel and performs an affirmative **local** action.
Pass that independently obtained value to `VerificationEvidence::independent_comparison`;
never auto-fill it from a received invitation/network assertion. An API cannot
prove that a human actually used an independent channel or that the remote human
verified their side. Tests simulate both independent actions rather than claiming
one endpoint can attest the other.

New imports create `Pending`. Fresh explicit comparison/action permits `Verified`;
identity or pinned Peer-ID change becomes `Changed`, preserving prior pin and
candidate separately. `Blocked` survives import/restart. Explicit unblock returns
to pending/changed, never directly verified; fresh verification is still required.
Evidence is single-use and bound to the candidate plus generation. Every successful
transition advances the checked generation; no silent pin replacement or TOFU.

`ContactTrustRepository` is the service composition seam. Use the existing
SQLCipher storage worker's bounded neutral `ContactTrustRow`/`load_contact`/
`compare_and_swap_contact`, mapping state enums explicitly and serializing pinned
and candidate invitations separately. Restore through `ContactTrust::from_persisted`
to revalidate all signatures/bounds/invariants; do not trust a database state tag
alone. Insert generation 1 with expected None, subsequent CAS uses prior generation
and next generation n+1. Publish a transition only after commit. On conflict/error,
reload/recompute, never blind overwrite. `kami-storage/tests/identity_trust.rs`
contains an exercised service-adapter example, including restarts in all states;
there is no production identity/storage dependency or identity-local database.

`send_decision` denies unknown, pending, changed, blocked, unauthenticated or forged
bindings. Pass the immutable queued recipient's original **device public key**,
not the current candidate; maintain the service's authenticated DeviceId-to-key
mapping. After adopting a different device key, old sends remain held, not retargeted.
Authenticated PeerId must come from the actual secure connection's remote endpoint,
including the end-to-end peer on a relay circuit, not the relay or invitation.
Services must observe changes durably, check fresh trust before enqueue **and every
retry/receive**, and serialize trust/check/enqueue/send authorization (or integrate
an atomic generation guard). A pure snapshot decision alone cannot close that race.
**No sender/service or direct/relayed transport path is implemented here.** KAMI-27/
KAMI-30 own actual-path validation; KAMI-51's WP2 boundaries are described above.
No live network, Signal/Matrix interoperability, cross-OS messaging or production
security qualification is claimed.


## Enrollment, restart, and key loss

### KAMI-27 verified profile enrollment and transport credentials

`verified_enrollment` composes identity provisioning with the storage enrollment row. The runtime integration retains a `ProfileLease`, checks fresh enrollment snapshots, verifies the native custody anchor and readback, and only then issues sealed, non-`Clone` transport credentials; lease ownership is enforced at runtime, not encoded as a Rust compile-time guarantee. Storage records are insert-once and immutable. Native anchor checks detect file-only tampering and cross-profile substitution, not coordinated database-plus-anchor rollback or same-user compromise. Native custody is platform-selected and fails closed; Linux local tests do not qualify Windows/macOS.

`transport_credentials` contains the restricted bridge used by the direct transport. It does not expose the identity seed. Live connection validation is performed by `kami-transport` against the authenticated remote PeerId; production orchestration belongs to `kami-runtime`.

`EnrollmentJournal` is a **trusted service integration trait**, not a user-input
DTO. Its implementer MUST retain the actual OS-enforced exclusive canonical
profile lease across the entire call, harden paths/ownership, provide accurate
state/database-presence queries, and durably commit markers before returning
(including directory metadata). This crate deliberately implements no file
journal, lockfile, SQLite transaction, or service worker.

1. `Fresh`: require absent database and BOTH credentials. Any query failure is an
   error, not absence. Persist the public `Pending` record before credential
   writes; create and verify each key by readback, then persist `Enrolled`.
2. `Pending`: require absent database and the original matching identity key.
   Reuse an existing database key; provision only a confirmed absent database key.
   Never regenerate the identity. If interrupted after the marker but before the
   identity write, fail closed with Missing and require explicit re-enrollment.
3. `Enrolled`: read-only verification; missing or malformed keys fail, a changed
   identity fails against the saved public descriptor. No regeneration/adoption.

**Here Enrolled means identity/key provisioning only.** The service/storage
integrator must separately create/verify the encrypted DB and commit the complete
profile-enrollment marker. Ordinary startup must use existing-only DB open;
a missing DB with prior complete enrollment evidence must NOT create blank
history. The service must also authenticate/public-record custody and enforce
profile uniqueness; this trait cannot prove that an arbitrary implementation
actually holds a lease or that its journal is durable.

Key loss requires an explicit new random profile namespace, fresh identity and
contact reverification. Existing namespaces are never overwritten/reset. Old
history is not restored, queued messages must not be redirected, and no automatic
backup/recovery guarantee is made. The storage layer detects a replaced/wrong
otherwise well-sized database key through keyed SQLCipher open.

## Native adapters and limitations

| Platform | Maintained dependencies / behavior | Prior native qualification evidence (not repeated by this documentation task) |
|---|---|---|
| Linux | `secret-service 5.2.0`, `zbus 5.19.0`; encrypted DH session, unlocked persistent default collection only, collection-scoped search with explicit per-item lock checks, no unlock call or session-collection fallback; add with `replace=false`, reject ambiguous results; structured denial/locked mapping | Linux compile/build/tests, real read-only missing-slot probe, invalid-bus headless subprocess, isolated native collection-boundary regression |
| macOS | `apple-native-keyring-store 1.0.2`, `keyring-core 1.0.1`, `security-framework 3.7.0`; login/User Keychain, UI disabled under adapter mutex, add-only native creation rejects duplicates | x86_64-apple-darwin all-target check and strict Clippy only |
| Windows | `windows-native-keyring-store 1.1.0`, `keyring-core 1.0.1`; explicit target handle, explicit **Local** persistence (not default Enterprise); serialized calls, preflight absence under caller lease | x86_64-pc-windows-msvc all-target check and strict Clippy only |

Windows CredWrite is upsert-only: the service's exclusive canonical profile lease
is mandatory across absence-check/write/readback and independent Kami processes
must use that same lease. Arbitrary noncooperating credential writers are not
protected by that lease. The backend hides its numeric PlatformError type:
unclassified native failures (including some denials) remain `Other`, never
`Missing`; no parsing/logging of raw OS messages. No-logon-session is Unavailable.
macOS's no-UI setting is process-global: service composition must not concurrently
change it through another library. Linux `create_item` may request native
additional authorization; locked collections are rejected without calling unlock.
KDE wallets that cannot retain arbitrary binary secrets are not qualified.

Only unit error categories/handles are formatted. Native error strings/sources
are discarded; temporary Rust secret buffers zeroize on drop. Native API copies,
transport-library allocations, dumps and compromised logged-in accounts remain
residual risks. Run blocking calls on a bounded service initialization worker,
never on network tasks or inside SQL transactions.

## Verification

From repository root with the current lockfile:

WP2-A1 canonical transcript/public-verifier checks, byte tampering, malformed and
bounded parsing, account/prekey/generation/validity substitution, custody failures,
redaction, and existing trust/Peer-ID rejection cases:

```sh
cargo test --locked -p kami-identity --lib verified_enrollment::tests::
cargo test --locked -p kami-identity --doc -- --nocapture
cargo test --locked -p kami-identity --test olm_binding
cargo test --locked -p kami-storage --test enrollment -- --nocapture
cargo test --locked -p kami-transport --doc -- --nocapture
cargo test --locked -p kami-runtime --lib ingress:: -- --nocapture
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets -- -D warnings
```

KAMI-27 focused enrollment/credential and compile-fail documentation tests were
reported passing on Linux by QA/Evaluator. The compile-fail doctests intentionally
exercise rejected APIs. These commands are local tests, not cross-OS/native-store
qualification or cross-network transport qualification.

Focused KAMI-50 vectors, import/binding negatives, local bilateral actions,
state histories, redaction and no-redirect decisions:

```sh
cargo test --locked -p kami-identity --test fingerprint --test binding --test invitation --test trust
cargo test --locked -p kami-storage --test contact_trust --test identity_trust
```

Linux tests/build/check/fmt/strict Clippy and the focused storage trust tests were
repeated for this documentation update. Earlier native/cross-target evidence in
the table is historical: both cross-target check commands below currently fail
with `E0463` (target `core`/`std` unavailable in this toolchain), so they were not
requalified. The two ignored native probes remain separate, opt-in qualification
and were not run by this documentation task.

```sh
cargo check --locked -p kami-key-provider -p kami-identity --all-targets --all-features
cargo build --locked -p kami-key-provider -p kami-identity --all-features
cargo test --locked -p kami-key-provider -p kami-identity --all-features
cargo fmt -p kami-key-provider -p kami-identity -- --check
cargo clippy --locked -p kami-key-provider -p kami-identity --all-targets --all-features -- -D warnings
cargo check --locked -p kami-identity --all-targets --target x86_64-pc-windows-msvc
cargo check --locked -p kami-identity --all-targets --target x86_64-apple-darwin
cargo test --locked -p kami-identity --test native native_absent_slot_probe -- --ignored --nocapture
cargo test --locked -p kami-identity --test linux_collection -- --ignored --nocapture
```

Tests cover idempotence/reconstruction, no replacement, typed per-slot failures,
key loss/new-namespace enrollment, partial provisioning, journal failures, malformed
keys/handles, mismatched profiles, identity replacement, redaction, and native
Linux headless behavior. The ignored absent-slot probe is read-only and requires
a logged-in native secure-store session. The separately ignored Linux collection
regression requires `dbus-run-session` and `gnome-keyring-daemon`: it launches a
private bus and disposable keyring with isolated HOME/XDG directories under the
test temporary directory. It uses real DH sessions and native credential writes
**only in that disposable fixture**, never the logged-in user's keyring. It proves
that a session-only match is Missing, a default-collection match succeeds even
with a session collision, duplicate default matches are Corrupt, and a locked
default collection is Locked. It caught the original service-wide-search defect
before the fix. Both daemon and temporary keyring are cleaned up after the run.
Windows/macOS native unlocked/locked/denied/create/reopen/reboot tests, Linux
logged-in-user write/locked/denied tests, cross-process lease+journal integration
and complete profile/service qualification remain required before M1 release.
Cross-checking from Linux is not native runtime qualification.
