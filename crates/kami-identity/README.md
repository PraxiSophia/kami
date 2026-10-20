# kami-identity

Protected one-device **identity/key-custody boundary**, not contact trust, transport,
E2EE, SQL, or full profile enrollment. Depends on `kami-key-provider` only for the
shared storage-key contract; never depends on `kami-storage` or `kami-protocol`.

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

These Ed25519 descriptors are not a Signal session profile or a Peer-ID binding
proof. Later trust/binding/session components must define and review those
contracts; this crate does not sign arbitrary IPC requests or export seeds.

## Enrollment, restart, and key loss

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

| Platform | Maintained dependencies / behavior | Evidence in this run |
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

From repository root, after the sole integrator resolves Cargo.lock:

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
