# kami-runtime

`kami-runtime` is the bounded KAMI-26 Tokio orchestration library for composing the existing synchronous KAMI-24 sender, KAMI-25 receiver/receipt, SQLCipher storage, and injected key-provider seams. It is a reusable runtime library, not a complete messaging service.

## Scope and status

KAMI-26 provides one supervised engine owner, bounded command/event and blocking-work queues, serialized blocking storage work, profile ownership, restart reconciliation, cancellation, bounded shutdown reporting, and a transport-neutral relay reservation-renewal scheduler.

The implementation is verified on the Linux checkout for the bounded supervisor/private-mock scope. It does not implement KAMI-27 transport, a functioning relay network, KAMI-32 IPC/CLI, a service executable, production encryption activation, or cross-platform/real-network qualification.

Live authenticated end-peer evidence is deliberately not accepted from mock-only interfaces. The runtime has no production authenticated connection producer. A future KAMI-27 producer must create opaque evidence within the authenticated end-peer boundary (or own the opaque evidence in a producer crate consumed by the runtime); caller-supplied `PeerId`, booleans, DTOs, claimed signatures, hashes, or database generations do not establish live provenance.

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

`Engine::enqueue` is intentionally fail-closed until KAMI-27 supplies a real authenticated end-peer producer. It is not a production send API.

## Blocking work, ownership, and recovery

Storage initialization, SQLCipher transactions, key-provider calls, sender/receiver composition, and reconciliation remain on the bounded blocking owner. No SQLCipher transaction or mutable authenticated-session borrow crosses an async `await`. Delayed storage/key-provider work therefore does not own the Tokio event loop or prevent relay timer and shutdown coordination from progressing.

Profile ownership reuses `kami-storage::ProfileLease`. The OS advisory lock is authoritative. Diagnostic owner metadata cannot steal a live profile, and the lease remains held while blocking work or storage shutdown is unfinished. A shutdown timeout reports `ShutdownOutcome::TimedOut`; it does not claim that an OS operation stopped or release ownership early. `wait_stopped` can wait for actual worker termination.

Startup reconciles durable sender/receiver state before entering `Lifecycle::Running`. Lost replies and ambiguous outcomes are reconciled from stable IDs and exact durable frames; the runtime never re-encrypts uncertain work or treats a transport write as `Delivered`.

## Relay-renewal seam

`relay::RelayAdapter` exposes cancellation-safe, nonblocking `execute(Action)` futures for `Reserve`, `Renew`, and `Release`. `RelayConfig` bounds operation timeouts, renewal margin, and retry backoff. `RelayEvent` reports attempts, activation, failures, expiry, release, and task failure. The adapter returns a reservation lifetime, not message delivery, and no real relay adapter ships in KAMI-26.

## Integration and security boundary

The private test integration exercises existing `kami-sender::Coordinator`, `kami-receiver::Receiver`, KAMI-51 E2EE stages, and SQLCipher persistence on the blocking owner. It verifies exact-frame recovery, durable inbox-before-ACK behavior, deduplication, forged-ACK rejection, and Delivered only after authenticated receipt validation.

The runtime's production ingress and worker modules are private. `AuthenticatedPeer` has private fields and no production public constructor, `Default`, `From`, serde rehydration, or public trust trait. Only `cfg(test)` code can mint the mock capability. This protects the runtime API boundary; it does not retroactively seal older public Peer-ID-taking APIs in other crates, so KAMI-26 makes no global no-bypass claim.

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

- No KAMI-27 direct transport, authenticated live Peer-ID validation, NAT traversal, or real network.
- No functioning relay network; only reservation-renewal scheduling and deterministic adapter tests.
- No KAMI-32 IPC, named pipes, Unix sockets, CLI, or service packaging.
- No production sender/receiver activation or fresh mutable session restoration.
- No global sealing of legacy public APIs, external rollback anchor, production encryption, interoperability, Windows/macOS qualification, or M1 qualification.
- Shutdown timeout bounds reporting, not termination of an already-running blocking OS operation.
