# kami-storage

SQLCipher-only encrypted local persistence for Kami's background service. One
exclusive profile lease and one dedicated worker own the writable connection.
This crate accepts validated facts and opaque E2EE-authenticated stages. Its
KAMI-25 recovery path checks bounded Protobuf correspondence against retained
authenticated evidence; storage does not itself cryptographically authenticate
identities/receipts, perform E2EE, or send network traffic.

## Implementation and qualification status

KAMI-22 is functionally implemented in the current Rust tree: the profile lease,
SQLCipher worker, migrations, encrypted outbox/inbox/history seams, receipt/order
facts, quotas, and fault/restart handling are present. Focused and workspace Rust
tests passed on the tested Linux checkout. Jira last reported KAMI-22 as **In
Progress** on 2026-11-03 because Windows/macOS native storage and broader platform
qualification remain incomplete, not because the core storage implementation is
unfinished. Independent verification and owner acceptance are separate labels.

The WP3 private state is implemented and accepted as a bounded private storage
foundation; its implementation and independent review evidence are separate from
the later WP2-B evidence. The WP2-B typed authenticated commit consumer is also
present and conditionally owner-accepted only within its quarantined scope.
KAMI-24/KAMI-25 add bounded sender/receiver and receipt/Delivered persistence below.
KAMI-26 now provides the verified bounded runtime owner that serializes this
synchronous storage boundary, isolates key-provider/SQLCipher work from Tokio
coordination, retains the profile lease through unfinished blocking operations,
and reconciles durable outcomes after restart. Production network, receipt/Delivered
activation, external rollback anchoring, and cross-platform qualification remain
downstream work.

## Modules and interfaces

- `lease`: `ProfileLease::acquire(&Path)` requires an existing absolute private
  profile directory. Unix checks ownership, restrictive modes, unsafe ancestors,
  symlinks and hard-linked lockfiles; a persistent OS lockfile is never deleted.
  Keep the lease across identity enrollment, then transfer it into storage.
  Acquisition is blocking; run it on a blocking initialization task.
- `open`, `migrations`, `migrations/001_initial.sql`: load-only injected
  `kami_key_provider::KeyProvider`, key-before-page-access, SQLCipher v4 format,
  profile namespace binding, transactional numbered schema migration and newer
  schema refusal. `Storage::create` is explicit, never used as an open fallback.
  It writes/syncs conservative enrollment evidence before creating a DB. An
  interrupted create or missing DB requires explicit recovery; this crate never
  replaces keys, deletes the evidence or automatically starts blank history.
- `message`: `StoredMessage` wraps `kami_core::MessageFacts` and 1..=65,536 payload
  bytes in zeroizing storage with redacted Debug. Sequence range is 1..=i64::MAX;
  the service supplies verified facts and safely prepared retransmission bytes.
- `transactions`, `outbox`: enqueue atomically checks/allocates the proposed next
  per-conversation/sender sequence and saves immutable outbox/history.  It exposes `next_sender_sequence` as a proposal, **not a reservation**; the
  service must serialize crypto/session preparation and enqueue checks that value
  atomically. After restart it reads the retained counter even if history is deleted.
  A successful
  return grants persistence eligibility, **not permission to bypass current trust
  checks**. `pending(limit)` replays identical durable material; the service must
  recheck identity/authorization before every attempt. Authenticated
  DEVICE_DELIVERED assertions must bind all message facts and the recipient.
- `inbox`: `accept` atomically persists message/dedup, history, ordering timestamp
  and receipt facts before returning `Acceptance::{New,Duplicate}` ACK eligibility.
  Neither variant authenticates the caller's input. `receipt` retrieves retained
  receipt facts for authenticated ACK regeneration. Conflicting IDs/sequences fail.
- `ordering`: `ordered_history` durably releases per-sender presentation after
  contiguous arrival or a bounded configurable gap wait (0..=60,000 ms). It returns
  explicit gap indicators; retained facts reconcile late arrivals without duplicate
  history, including after deletion. Once visible, clock rollback does not hide it.
- `worker`: blocking synchronous facade with owned concrete commands, bounded
  `try_send` admission and safe typed `StorageError`. No connection, transaction,
  user callback or async future escapes the worker. Networking code must use its
  blocking service boundary, **never call this facade on a network executor**.
  Shutdown drains accepted requests and closes SQLite before releasing the lease.
  KAMI-26 uses this boundary from one persistent bounded `spawn_blocking` owner;
  a runtime shutdown timeout reports status but does not release the lease while
  an admitted operation may still mutate profile state.

`delete_history` removes the local display body without canceling an outstanding
outbox. `cancel` stops retry without deleting history or recalling delivery. A valid
late receipt may record actual delivery after cancellation. Terminal outbox payloads
are cleared; compact immutable dedup/receipt evidence remains encrypted. This is
logical deletion, **not forensic erasure**.

## Contact trust (KAMI-50)

`migrations/002_contact_trust.sql` adds encrypted neutral contact rows; existing
v1/v2 databases originally upgraded through schema v3; current open upgrades through
v5 and fresh create applies all five numbered migrations in one transaction. Each migration has its own
version guard, so opening v2 never reruns migration 002. Existing profile namespace
is checked before schema migration, so a wrong namespace cannot rewrite a v1 database. A failed upgrade leaves the old schema/version/data
intact. No sidecar, second database, plaintext trust store or message-payload reuse.

`contact_trust` exports `ContactTrustRow`, `TrustState` and the 512-byte descriptor
bound. A row has a stable service-owned 32-byte contact ID, format version, state,
positive checked generation, optional prior pinned descriptor and separate candidate.
Bounds/state-shape constraints are checked in Rust, SQL and on load (lengths before
copying). `Storage::load_contact` and `compare_and_swap_contact` use the existing
bounded worker/SQLCipher connection. None inserts generation 1; Some(n) commits
only generation n+1 against the current generation, rejecting stale block/change/
verification writers. Results return only after durable commit. The existing page
and command queue limits apply; there is no unbounded contact-list API.

These neutral public-descriptor blobs are NOT cryptographically authenticated by
storage. Trusted service composition must enforce identity transitions and map
rows through `kami_identity::ContactTrust::from_persisted` to recheck signatures,
versions and invariants. `tests/identity_trust.rs` is an exercised adapter example;
identity remains a direct **dev dependency**; WP2-B also brings it transitively
through the normal `kami-e2ee` dependency. Storage still does not own identity policy.
Do not expose the neutral row constructor/CAS or trusted restoration path as IPC
"set verified". Current enqueue/pending behavior remains unchanged: callers must
check current durable trust before enqueue/retry and serialize that check with
sending; no runtime sender enforcement or E2EE is claimed.

Focused executed commands (from repository root):

```sh
cargo test --locked -p kami-storage --test contact_trust --test identity_trust
cargo test --locked -p kami-storage --test crash_and_artifacts
cargo test --locked -p kami-storage --lib
```

Coverage includes encrypted v1 upgrade preserving outbox facts, all trust-state
restarts, stale/concurrent CAS writers, malformed durable descriptors/oversized
rows, checked generations, redacted Debug, process-kill recovery of blocked trust,
and absence of fixture trust/payload/key bytes in inspected live DB/WAL/sidecars.

## WP3 private storage-only seam

`migrations/003_e2ee_state.sql` adds one account row (including provider prekeys),
context-bound sessions and exact immutable frame evidence in the existing
SQLCipher database. `session_state` and the concrete `E2ee*` worker requests are
**private and quarantined**; no public exporter, raw SQL, second connection or
runtime adapter is exposed. WP2-B adds a normal E2EE dependency and typed consumer
below; the neutral row/commit constructors remain private, never caller-provided
authentication evidence.

Account/session blobs are 1..=1,048,576 bytes, context 1..=2,048 bytes, pins
1..=512 bytes, digests exactly 32 bytes, generations 1..=i64::MAX, and frames
1..=65,536 bytes. Rust, SQL and load-time length checks bound these fields.
A single `BEGIN IMMEDIATE` compares the exact previous account/session bytes and
digests, checks the live durable Verified contact generation/pin, then commits the
next session, optional account/prekey consumption, message, sequence, history,
replay/receipt facts and immutable frame together. Failure grants no send/ACK
eligibility. Reusing an existing message requires reconciliation, not another
ratchet commit or promotion of legacy records.

Reconciliation compares exact frame and logical facts, session branch evidence,
account generation/digest and current durable trust. It can recover an abandoned
worker reply after reopen and an old frame after later session advancement. It
never re-encrypts a retry. Inbound eligibility follows committed replay/receipt
facts, retained even after local history deletion. This is not an unauthenticated
ACK oracle: fresh authenticated end-peer/proofs remain a trusted caller duty.
Legacy enqueue/accept/receipt mutation cannot bypass these rows; legacy `pending`
excludes quarantined frames. Cancel still prevents outbound reconciliation.

**No database generation/digest is an external rollback anchor.** A coherent old
DB/WAL copy is not detectable by these SQL checks. E2EE profile restoration fails
with `RollbackAnchorUnsupported`; only separately retained in-process anchors
support private snapshot qualification. Opening encrypted storage and recovering
committed bytes is not permission to resume production cryptography. KAMI-25 adds
the dedicated receipt ratchet/Delivered transactions below; runtime send/ACK
activation remains deferred.

`worker/tests.rs` covers v1/v2 upgrades and v3 reopen, v3 migration failure rollback,
state bounds/corruption, stale/concurrent CAS, digest/byte forks, quota/fault
rollback, replay conflicts, legacy isolation, exact retry, lost replies, trust
holds and history deletion. Twelve real subprocess kill checkpoints cover before,
account, session, message, frame and after-commit phases for outbound and
inbound-prekey-shaped transactions; DB/WAL/sidecars are scanned for synthetic
state/frame bytes. These storage fixtures are neutral synthetic bytes, not an
integrated cryptographic exchange. `tests/e2ee_state.rs` additionally exercises
keyed SQLCipher v2 upgrades/reopen and failed-upgrade schema/data preservation.
Run from the repository root:

```sh
cargo test --locked -p kami-storage --lib wp3_
cargo test --locked -p kami-storage --test e2ee_state
```

## WP2-B typed authenticated commit

`Storage::commit_authenticated(AuthenticatedCommit)` is the WP2-B ordinary-text
write/reconciliation operation; KAMI-25 delivery/receipt operations are separate.
The acyclic normal dependency is
`kami-storage -> kami-e2ee`, never the reverse. E2EE owns authentication, stage
finalization, immutable facts/frame and bounded snapshots; storage retains the
KAMI-22 lease, SQLCipher/key-provider, single worker, quotas, CAS and transaction.
There is no public neutral commit/account constructor or raw database API.

The worker consumes the opaque candidate and converts it privately to WP3 rows.
Initial account bootstrap, optional OTK consumption, next session, inbox/outbox,
sequence and exact frame commit in one IMMEDIATE transaction. Fallback preserves
the account generation. Existing-frame candidates reconcile exact authenticated
facts/frame/branch with current durable trust and return original bytes without
another ratchet update. Returned bytes confirm persistence, **not ACK authority**;
receipt ratchet and Delivered transitions require the dedicated KAMI-25 types,
not this ordinary text API. `received_at` for
this bounded API uses the E2EE finalization's supplied local millisecond clock,
not the sender's display timestamp or a live network arrival attestation.

The E2EE proposal retains its last authenticated stage evidence;
`QuarantinedSession::prepare_retry` requires fresh trust/end-peer/validity checks
and produces an exact candidate without encryption. Retain that proposal on an
ambiguous worker reply, reconcile, and install advancement only after confirmation.
Profile reopening can reconcile retained in-process evidence; it cannot reconstruct
trusted ratchets from DB-only digests. No automatic profile restore, network send,
ACK emission or runtime end-peer source is introduced. KAMI-26 remains the live
provenance and trust-to-transmission gate.

The consuming opaque snapshot custodian is explicitly trusted: valid stage holders
can technically capture full provider pickles, including private prekeys. Opaque
containers have no public fields, Clone/Default/Debug/serde or neutral constructors;
zeroizing bounded transfer and SQLCipher do not protect against a malicious
custodian, native memory copies or endpoint compromise. No raw individual-key
getters or comprehensive memory-erasure claim are made.

`tests/authenticated_boundary.rs` uses rustc with Cargo-selected existing rlibs
(no new test dependency). Its positive external fixture exercises real Olm sends,
first inbound and normal receives, OTK/fallback account effects, exact retry after
progress/reopen, stale branch/trust rejection, quota rollback, insert faults at
account/session/message/frame phases and encrypted artifact scans. Negative
external compilation asserts neutral `StoredMessage` is rejected with E0308;
E2EE's privacy probes pin diagnostics and include a positive fixture. Older WP3
process-kill tests cover the shared transaction's six checkpoints in both
directions using synthetic neutral bytes; they are not real-network tests.

```sh
cargo test --locked -p kami-storage --test authenticated_boundary -- --nocapture
cargo test --locked -p kami-e2ee --test privacy
cargo test --locked -p kami-storage -p kami-e2ee
```

Olm-v1 remains quarantined bounded-development only. Live transport/ACK emission,
prekey publication/rotation/freshness policy, external rollback anchor,
interoperability, native macOS/Windows and public/production qualification remain
separate gates. See [E2EE limits](../kami-e2ee/README.md).

## KAMI-24 sender persistence seam

Migration `004_sender_state.sql` advances the SQLCipher schema to v4. It creates
private sender reservation and attempt-state rows plus a per-database request-binding
key. Migration setup requires the existing SQLCipher/key-provider opening path and
working OS entropy: the worker generates the 32-byte binding key inside the same
immediate migration transaction, and failure rolls the migration back. It is not a
plaintext or separately managed sender-state store.

`Storage::prepare_send` reserves one per-conversation/sender sequence lane before
the quarantined E2EE stage is prepared. Its request binding makes a same-ID
resubmission recoverable only when immutable request fields match. `commit_prepared`
atomically consumes that reservation with the authenticated E2EE commit, durable
outbox/history facts, exact frame, session advancement, and initial attempt row.
`cleanup_prepared` expires uncommitted reservations; callers of the low-level API
must invoke it explicitly. Expired reservations become retained tombstones rather
than reusable IDs.

`recover_sends` returns a bounded ordered durable projection, while `claim_send`
leases exact persisted frame bytes, `record_send_failure` records retry eligibility,
`hold_send`/`resume_send` implement current-trust holds and original-pin
reverification, and `cancel_send` stops future attempts without discarding the
authenticated immutable material. A crash or lost worker reply is reconciled from
the stored exact frame; retry never asks this storage seam to re-encrypt it.

Attempt leases are fenced for 30 seconds. Failure uses the shared one-second
exponential backoff capped at five minutes and reaches `Exhausted` after eight
attempts. This finite retry policy and the count quota consumed by retained
preparation tombstones are intentionally bounded follow-up work; this crate does
not itself provide automatic retry scheduling, transport, or live end-peer
provenance. KAMI-25 adds authenticated receipt consumption and the durable
`Delivered` projection through a separate typed path below.

Focused KAMI-24 evidence was recorded on Linux:

```sh
cargo test --locked -p kami-sender -p kami-storage
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
```

Those checks covered sender/storage behavior and passed in the recorded QA and
Evaluator runs. They do not qualify Windows/macOS, real networks, public/production
encryption, end-peer authentication, authenticated receipts, or an external
rollback anchor.

## KAMI-25 receiver and receipt persistence

`migrations/005_receiver_state.sql` advances the existing SQLCipher database to
schema v5. Migration creates only `delivery_artifact` and `receiver_gap`; it does
not create a second inbox, duplicate facts/payload, or another mutable session
authority. `delivery_artifact` references the canonical `e2ee_frame` by sender/ID
and retains the exact receipt frame, SHA-256 artifact ID, direction and post-receipt
session generation/digest. `receiver_gap` references canonical `message` rows and
owns only Unresolved/Permanent presentation observations. Existing account/session,
message/history/receipt and sender rows remain their respective authorities.
Upgrading v4 preserves canonical rows and the sender binding key; migration and
version update are atomic. Failed migration leaves the old schema/version intact.

The single bounded worker exposes four dedicated typed operations:

- `commit_delivery(AuthenticatedDelivery)` commits authenticated text reception
  and same-session outbound DeviceDelivered staging together. One IMMEDIATE
  transaction checks current Verified trust/pin and exact source account/session,
  performs optional account bootstrap/OTK consumption, then persists inbox,
  dedup/history/receipt facts, exact text, both session-generation transitions,
  receipt artifact and gap observations. A fallback prekey preserves the account
  generation. No separate receipt commit can leave a partially accepted inbox.
- `recover_delivery(ReplayPermit, frame)` returns `Option<DurableDelivery>` only
  for exact retained authenticated text. It rechecks current trust, original pin,
  canonical facts, exact frame, receipt facts, artifact digest and account/session
  generation links. Legacy neutral inbox rows cannot be promoted into authenticated
  ACK authority. `DurableDelivery::{facts, acceptance, receipt}` exposes durable
  evidence; recovery returns Duplicate without ratcheting or recreating history.
- `commit_delivery_receipt(AuthenticatedReceiptCommit)` validates the target's
  exact stored frame/facts and same-session source, then atomically commits the
  incoming receipt ratchet/artifact/facts, canonical Delivered message state and
  terminal attempt lane. A genuine late receipt may supersede cancellation.
- `recover_delivery_receipt(ReplayPermit, frame)` returns retained `MessageFacts`
  only for an exact committed receipt and coherent Delivered projection, with
  current trust/original pin checks. Alternate receipt ciphertext conflicts.

Recovery is readback of committed evidence, not fresh ciphertext authentication or
mutable profile/session restoration. A lost worker reply or ACK after commit is
recoverable after reopen with the original exact frame; before commit, failure
grants no ACK eligibility. Local history deletion retains message/dedup/receipt
evidence. Caller/future-runtime supplied live end-peer provenance and serialization
of trust checks with actual emission remain required. No qualified external
rollback anchor exists; coherent old DB/WAL rollback is not detected by DB-only
counters or digests, and fresh mutable cryptographic restoration remains restricted.

`ordered_history` retains presentation eligibility per sender/conversation, waits
0..=60,000 ms as requested, and returns `gap_before` plus
`GapState::{Unresolved,Permanent}`. Querying ordering marks observations Permanent
at 60,000 ms from local receipt; late predecessors still reconcile them. Deleted
history's retained visible facts do not become false network gaps. Eligibility
and gap metadata do not implement exactly-once terminal rendering.

Receiver admission caps retained inbound messages at 1,024 per sender/contact and
512 per sender/conversation, forward sequence distance at 4,096 from the retained
high sequence, and outstanding gap observations at 128 per sender (including
Permanent until resolved). Net gap resolution can admit a predecessor at the gap
cap. Shared `Config::outbox_messages` and `outbox_bytes` also bound the retained
authenticated-frame count and total text-frame/receipt-artifact/history bytes.
Incoming receipt commits check the additional artifact bytes against that budget;
database pages and worker queue remain independently bounded. `Full` rolls back
without dropping prior messages or granting ACK eligibility. No active
dedup/receipt cleanup or compaction exists: deleting history does not reclaim all
retained evidence/count capacity, and exhaustion fails closed.

Source coverage: `tests/receiver_migration.rs` checks encrypted v4-to-v5 upgrade,
preserved key/history and failed-migration rollback. The
[receiver integration target](../kami-receiver/tests/receive.rs), including its
`failures.rs` module, covers real staged Olm-to-SQLCipher receipt flow, forged ACKs,
concurrent duplicates/trust changes, insertion faults, process kills before/after
commit, exact recovery after reopen/deletion, gap reconciliation, corruption,
legacy isolation and capacity rejection. These are bounded local fixtures, not
production, real-network, cross-platform or power-loss qualification. Commands
from repository root (not rerun in this documentation-only task):

```sh
cargo test --locked -p kami-storage --test receiver_migration
cargo test --locked -p kami-receiver --test receive
```

## Bounds and local protection

Default `Config`: 32 queued requests (permitted range 1..=1024), 4,096 pending outbox
messages, 16 MiB pending payload and 16,384 database pages. Format-v4 pages are
4,096 bytes. Configurable `max_database_pages` limits the main SQLite page space;
WAL/SHM, filesystem overhead, accepted caller/response buffers and crash artifacts
are additional resources, not a total filesystem quota. SQLite busy waits are
bounded to 250 ms. Read APIs bound each response to 1..=1024 messages. Queue-full
rejects new admission without dropping accepted commands; logical quotas and
SQLITE_FULL return `Full` without granting send/ACK eligibility or consuming a
sequence. An accepted command can commit even if its caller disappears; retry
using the same logical ID/facts, never invent a replacement ID.

All database contents use SQLCipher: no plaintext fallback, unkeyed ATTACH, SQL
export or automatic backup API. Open verifies cipher version, v4 compatibility,
4,096-byte cipher pages, authenticated pages, no plaintext header, native memory
TEMP_STORE support, `temp_store=MEMORY`, `foreign_keys=ON`, WAL and
`synchronous=FULL`, plus integrity/foreign-key checks. ATTACH is disabled. Native
cipher logs are disabled; public errors retain no backend SQL/path/source text.
WAL content is encrypted; SHM, filenames, sizes and timing still expose metadata.
Profile directories must be owner-private (0700 on Unix), with owner-only files
(0600). Group/world-writable ancestors, including ordinary shared `/tmp`, are
rejected. Removable/network filesystems and external concurrent writers are not
qualified. Same-user endpoint compromise is outside this protection boundary.

Dependencies include `kami-core`, `kami-e2ee`, `kami-protocol` (bounded recovery
correspondence), `sha2`, injected `kami-key-provider`, `rusqlite = 0.40.2`
with **only** `bundled-sqlcipher-vendored-openssl`/`limits` (not plain `bundled`),
`zeroize = 1.9.1`, Unix `rustix = 1.1.5`; test-only `tempfile` and the explicit
`test-provider` feature. Native keying and SQLite/OpenSSL memory copies/crash dumps
remain residual risks despite zeroized Rust keying/payload buffers. No backup,
rekey, production profile restore or runtime ratchet/session integration is claimed.
Identity enrollment's
`EnrollmentJournal` and this crate's conservative marker still require trusted
service composition; the marker is not a replacement for identity enrollment.

## Verification and platform gates

Executed on Linux x86_64 with Rust 1.98.1: SQLCipher **4.14.0 community**, SQLite
**3.51.3**, vendored **OpenSSL 3.6.3**. Storage tests exercise migrations/rollback,
newer-schema refusal, sequence/outbox replay, inbox-before-ACK, dedup conflicts,
receipt binding, cancellation/deletion, gap waiting/reconciliation, bounded queue
saturation, injected key errors, wrong keys/plaintext/corruption, SQLITE_FULL,
SQLite/profile lock contention, and real subprocess kill/restart. Artifact tests
inspect live WAL/SHM, a rollback journal and a memory temporary table, verify file
permissions, and find neither fixture payload nor fixture key in inspected files.
They verify no automatic backup files, not the security of an unimplemented backup
facility. Process kill tests are not a power-loss/filesystem durability certification.

Windows and macOS are **not qualified**. Non-Unix profile opening deliberately
fails closed as Unsupported pending native ACL/reparse-point implementation and
qualification; macOS Unix code has not run on a native host. Only a Linux Rust
target was available. Do not claim M1 three-OS completion or infer SQLCipher/native
crypto build success from Rust portability. Native host/SDK and OS filesystem
protection qualification remain release gates, not a reduction of approved targets.

Reproduce from repository root (all listed standard workspace gates ran successfully):

```sh
cargo check --locked --workspace --all-targets
cargo build --locked --workspace
cargo test --locked --workspace
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked -p kami-storage
```

A separate dependency graph deliberately links **ordinary SQLite** and imports
production `open.rs`; it proves a cipher-disabled binary returns CipherUnavailable
before any schema/data writes (executed successfully on Linux):

```sh
CARGO_TARGET_DIR="$PWD/target/plaintext-negative" cargo run --locked \
  --manifest-path crates/kami-storage/tests/plaintext_probe/Cargo.toml
cargo fmt --manifest-path crates/kami-storage/tests/plaintext_probe/Cargo.toml -- --check
```

The nested probe's lockfile is separate to prevent Cargo feature unification from
accidentally turning this negative test into another SQLCipher-positive binary.
Tests use disposable private directories beside their build outputs rather than
shared system temporary directories. No actual network integration is implied
by persistence seams; cryptographic protocol validation remains in `kami-e2ee`.
