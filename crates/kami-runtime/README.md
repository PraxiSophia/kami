# kami-runtime

`kami-runtime` is the bounded Tokio orchestration library for profile enrollment, direct transport supervision, and the existing synchronous sender/receiver and SQLCipher seams. It is a reusable runtime library, not a complete messaging service.

## Scope and status

KAMI-26 provides one supervised engine owner, bounded command/event and blocking-work queues, serialized blocking storage work, profile ownership, restart reconciliation, cancellation, bounded shutdown reporting, and a transport-neutral relay reservation-renewal scheduler.

KAMI-27 adds a supervised direct-only connection substrate: the runtime retains the profile lease, revalidates enrollment/custody, and owns the transport child and revocable capabilities. The source and focused tests were independently QA/Evaluator verified on Linux. This is not production message delivery, relay support, KAMI-32 IPC/CLI, a service executable, or cross-platform/real-network qualification.

Live authenticated end-peer evidence is minted only within the KAMI-27 transport/runtime boundary. The runtime does not accept caller-supplied `PeerId`, booleans, DTOs, claimed signatures, hashes, or database generations as proof of a live connection.

## Architecture

The runtime owns mutable lifecycle state in one Tokio task. Child work reports typed completions and faults; it does not mutate engine state directly.

- `engine`: public `Engine`, `Profile`, `Config`, lifecycle snapshots, commands, cancellation, and shutdown coordination.
- `worker`: one persistent `spawn_blocking` owner for synchronous SQLCipher/storage and key-provider work. Blocking admission is bounded.
- `ingress`: private authenticated-evidence and integration boundary. Production live ingress is absent; the test-only mock is not exported.
- `relay`: reservation scheduling only. It owns reserve/renew/release timing, operation timeouts, expiry, capped retry backoff, cancellation, and diagnostic events; it does not implement relay networking or delivery.
- `integration_tests`, `tests`, and `tests/ownership`: deterministic runtime, fault, privacy, integration, and cross-process ownership coverage.

The engine uses bounded Tokio `mpsc` channels for commands, events, blocking jobs, and completions. Diagnostic events are best-effort and dropped when their bounded receiver is full; snapshots and command replies are authoritative.

## Public interfaces

`Engine::start` and `Engine::start_with_relay` must be called inside a Tokio runtime and return an engine plus a bounded event receiver. `Profile` identifies the existing profile directory, `ProfileKeyId`, injected `KeyProvider`, explicit-create choice, and storage configuration.

`Config::default()` uses command/event capacity 32, blocking capacity 8, a 10-second shutdown timeout, and a one-second retry-eligibility scan. Capacities and timing are configurable within validated bounds.

Important methods include:

- `Engine::wait_running`, `health`, `snapshot`, and `recovered` for lifecycle and durable-reconciliation state;
- `cancel` for durable sender cancellation;
- `request_shutdown`, `shutdown`, and `wait_stopped` for cancellation and drain coordination;
- `enqueue`, which currently returns `Error::LiveTransportUnavailable` rather than accepting asserted authentication authority.

A minimal example using the real public API:

```rust,no_run
use kami_runtime::{Config, Engine, Profile};

# async fn run(profile: Profile) -> Result<(), kami_runtime::Error> {
let (engine, mut events) = Engine::start(profile, Config::default())?;
engine.wait_running().await?;
let _snapshot = engine.snapshot();
let _ = events.try_recv(); // diagnostics are bounded and best-effort
let _outcome = engine.shutdown().await;
# Ok(())
# }
```

`Engine::enqueue` remains intentionally fail-closed: the direct transport provides only connection control and the prekey-only exchange below, not application payload/receipt/Delivered or production E2EE message integration.

## Prekey lifecycle (KAMI-51 WP-B)

`refresh_prekeys` uses a live `AuthenticatedConnection`, a random durable request
challenge, same-swarm typed exchange, retain-before-ACK, and committed ACK-result.
`retry_prekeys` reconciles only the exact stored query/response bytes. Each
operation has a five-second outer deadline; transport evidence is additionally
fenced by capability TTL and connection identity. Fresh SQLCipher contact/pin,
native enrollment/custody and driver evidence are checked on the blocking owner.
No caller-supplied Peer ID or cached signed proof can mint live evidence.

The same authenticated QUIC/TCP swarm carries one signed selection per response,
not a batch/aggregate proof. The strict protocol caps frames at 16 KiB including
the four-byte prefix. Transport admits one exchange per peer/direction, 16 global,
with a separate reliable inbound queue of 16 (driver commands/events 64/32).
Runtime default command/event/blocking capacities remain 32/32/8.

The worker stages opaque E2EE account custody and identity-owned independent
per-selection signatures, then atomically installs SQLCipher lifecycle inventory.
Initial inventory is 32 OTKs plus reusable fallback, capped at 50 retained OTKs;
replenishment never evicts or relabels exposed keys. `replenish_prekeys` and
`rotate_prekey_fallback` stage exact-prior-account CAS. At most current/previous
fallback slots are retained; an unexpired previous fallback cannot be overwritten.
Fallback issuance/use is explicitly bounded at 16 and never counts as OTK
consumption. Signed selection validity is at most five minutes.

Schema v7 freezes exact signed response bytes and global exclusive OTK reservation
before escape. Receiver retains bytes, trust and monotonic generation floors
before publication ACK; duplicate matching ACKs reconcile and conflicts reject.
Per-peer ACKs never call the provider's global mark-published operation. OTK
consumption joins the existing session/message/frame transaction; stale account
candidates cannot overwrite replenished inventory. Revocation/retirement and a
local clock floor fail closed. Durable history is capped at 1,024 rows per ledger
and 32 remote authority records, with no deletion of exposure/tombstone evidence.
Capacity exhaustion blocks rather than silently discarding records.

After restart, already-frozen exchanges can reconcile while valid, but new
publication from an existing account fails closed: database-only mutable account/
session restore is not authorized (WP-C). SQLCipher floors are not an external
rollback anchor. `enqueue` stays unavailable; this does not activate public E2EE,
KAMI-28 messaging, WP-D qualification, offline publication, relay, or CLI/IPC.

Focused Linux native QUIC/TCP exchange and exact retry check:

```sh
cargo test --locked -p kami-runtime transport_tests::native::supervised_native_transport_preserves_lease_and_never_delivers_payloads -- --exact --nocapture
cargo test --locked -p kami-storage --test authenticated_boundary -- --nocapture
cargo test --locked -p kami-e2ee --test prekeys
```

Bounded accepted with conditions: implementation `t_d364301b` run 204; independent
QA `t_0dcc13ac` run 205 PASS; final Evaluator `t_250e6416` run 206
PASS-WITH-CONDITIONS on the uncommitted tree at baseline
`438d466603f180efcd02816ed8a69be6aaa17391`. This is not production/M1 acceptance.
WP-D cross-network, Windows/macOS, interoperability, sustained fuzz/DoS and native
load qualification remain outstanding.

### Owner-operated interface is not yet available for WP-B

`transport_tests::native::manual_endpoint`/`manual_session` is the existing KAMI-27
two-process Linux loopback harness. It displays invitations/fingerprints and
requires independent comparison plus explicit VERIFY/REJECT, then offers only
quic/tcp/disconnect/quit. It never calls `Engine::{refresh_prekeys,retry_prekeys}`
and has no public selection/generation/freshness/ACK evidence display. Adding
those commands and a bounded public-only diagnostic view to
`src/transport_tests.rs::manual_session` requires separate Rust test-interface
authorization; no such source change is made by this documentation task.

The [WP-B fixture runner](../../scripts/kami51-wpb-manual/README.md) invokes the
existing real native automated runtime test instead. That fixture starts two
engines within a private child process and simulates trust decisions; it is not
two owner-operated endpoints and cannot count as independent human verification.
Manual WP-B qualification remains blocked/pending, including selection/ACK display,
duplicate retry, TCP exchange and explicit prekey rejection outcomes. KAMI-27's
manual suite is unchanged; its acceptance does not transfer to WP-B.

## Blocking work, ownership, and recovery

Storage initialization, SQLCipher transactions, key-provider calls, sender/receiver composition, and reconciliation remain on the bounded blocking owner. No SQLCipher transaction or mutable authenticated-session borrow crosses an async `await`. Delayed storage/key-provider work therefore does not own the Tokio event loop or prevent relay timer and shutdown coordination from progressing.

Profile ownership reuses `kami-storage::ProfileLease`. The OS advisory lock is authoritative. Diagnostic owner metadata cannot steal a live profile, and the lease remains held while blocking work or storage shutdown is unfinished. A shutdown timeout reports `ShutdownOutcome::TimedOut`; it does not claim that an OS operation stopped or release ownership early. `wait_stopped` can wait for actual worker termination.

Startup reconciles durable sender/receiver state before entering `Lifecycle::Running`. Lost replies and ambiguous outcomes are reconciled from stable IDs and exact durable frames; the runtime never re-encrypts uncertain work or treats a transport write as `Delivered`.

## Relay-renewal seam

`relay::RelayAdapter` exposes cancellation-safe, nonblocking `execute(Action)` futures for `Reserve`, `Renew`, and `Release`. `RelayConfig` bounds operation timeouts, renewal margin, and retry backoff. `RelayEvent` reports attempts, activation, failures, expiry, release, and task failure. The adapter returns a reservation lifetime, not message delivery, and no real relay adapter ships in KAMI-26.

## Integration and security boundary

The KAMI-27 worker starts the connection child only under runtime ownership, validates actual authenticated end-peer connections against fresh trust/pin decisions, applies bounded direct dialing, and revokes capabilities on shutdown, worker loss, expiry, or generation change. The ProfileLease-to-connection relationship is enforced by runtime control flow, not a compile-time type proof. The underlying driver/verifier library primitives are not by themselves production orchestration.

The private test integration exercises existing `kami-sender::Coordinator`, `kami-receiver::Receiver`, KAMI-51 E2EE stages, and SQLCipher persistence on the blocking owner. It verifies exact-frame recovery, durable inbox-before-ACK behavior, deduplication, forged-ACK rejection, and Delivered only after authenticated receipt validation.

The runtime's production ingress and worker modules are private. `AuthenticatedPeer` has private fields and no production public constructor, `Default`, `From`, serde rehydration, or public trust trait. Only `cfg(test)` code can mint the mock capability. This protects the runtime API boundary; it does not retroactively seal older public Peer-ID-taking APIs in other crates, so KAMI-26 makes no global no-bypass claim.

## KAMI-27 verification

From the repository root (focused Linux tests reported passing by QA/Evaluator):

```sh
cargo test --locked -p kami-transport --test direct real_native_quic_tcp_and_fallback -- --exact --nocapture
cargo test --locked -p kami-runtime --lib transport_tests:: -- --nocapture
cargo test --locked -p kami-runtime --lib ingress:: -- --nocapture
cargo test --locked -p kami-runtime --test privacy -- --nocapture
cargo test --locked -p kami-identity --lib verified_enrollment::tests::
cargo test --locked -p kami-storage --test enrollment -- --nocapture
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets -- -D warnings
```

These checks establish local Linux behavior only. Same-host loopback is not
cross-network or cross-OS qualification.

## Verification

From the repository root:

```sh
cargo fmt --all -- --check
cargo check --locked --workspace --all-targets
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked -p kami-runtime --quiet
cargo test --locked --workspace --quiet
git diff --check
```

The completed KAMI-26 implementation and independent QA/Evaluator runs recorded all of these checks passing. The focused runtime suite recorded 21 passing tests; the workspace suite recorded 236 passing tests with two pre-existing native secure-store tests ignored because they require an isolated logged-in D-Bus/gnome-keyring environment.

## Limitations and deferred work

- No application payload, application ACK/receipt/Delivered state, production E2EE/session restoration, relay, NAT traversal, or IPC activation. Direct transport is bounded to exact-IP candidates, authenticated connections and the prekey-only protocol.
- No functioning relay network; only reservation-renewal scheduling and deterministic adapter tests.
- No KAMI-32 IPC, named pipes, Unix sockets, CLI, or service packaging.
- No production sender/receiver activation or fresh mutable session restoration. Native anchor checks are file-only/cross-profile checks, not rollback resistance.
- No global sealing of legacy public APIs, external rollback anchor, production encryption, interoperability, Windows/macOS qualification, or M1 qualification.
- Shutdown timeout bounds reporting, not termination of an already-running blocking OS operation.
