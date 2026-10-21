# kami-storage

SQLCipher-only encrypted local persistence for Kami's background service. One
exclusive profile lease and one dedicated worker own the writable connection.
This crate accepts already validated message/security facts: it does not parse
Protobuf, authenticate identities/receipts, perform E2EE, or send network traffic.

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

`delete_history` removes the local display body without canceling an outstanding
outbox. `cancel` stops retry without deleting history or recalling delivery. A valid
late receipt may record actual delivery after cancellation. Terminal outbox payloads
are cleared; compact immutable dedup/receipt evidence remains encrypted. This is
logical deletion, **not forensic erasure**.

## Contact trust (KAMI-50)

`migrations/002_contact_trust.sql` adds encrypted neutral contact rows; existing
v1 databases upgrade atomically to v2 on open, and fresh create applies both
migrations in one transaction. Existing profile namespace is checked before schema
migration, so a wrong namespace cannot rewrite a v1 database. A failed upgrade leaves the old schema/version/data
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
identity is a **dev dependency only**, never a production storage dependency.
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

Dependencies: `kami-core`, injected `kami-key-provider`, `rusqlite = 0.40.2`
with **only** `bundled-sqlcipher-vendored-openssl`/`limits` (not plain `bundled`),
`zeroize = 1.9.1`, Unix `rustix = 1.1.5`; test-only `tempfile` and the explicit
`test-provider` feature. Native keying and SQLite/OpenSSL memory copies/crash dumps
remain residual risks despite zeroized Rust keying/payload buffers. No backup,
rekey, restore or ratchet/session-state integration is claimed. Identity enrollment's
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
shared system temporary directories. No actual network/E2EE integration is implied
by persistence seams; all protocol validation remains outside this crate.
