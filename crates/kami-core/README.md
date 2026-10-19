# kami-core

`kami-core` is a dependency-free Rust domain crate for immutable logical-message facts, deterministic outbox delivery projections, and queue-admission accounting. It is deliberately independent of transport and storage adapters.

## Structure

- `ids` defines opaque `MessageId`, `ConversationId`, `DeviceId`, and checked `SenderSequence` values.
- `message` groups a logical message's immutable identity and `ContentBinding` in `MessageFacts`, and classifies retained versus incoming facts as distinct, duplicate, or conflicting.
- `delivery` reduces explicit `DeliveryEvent`s into a `Delivery` projection. A path (`Direct` or `Relay`) is attempt metadata, while message facts remain unchanged across retries.
- `queue` supplies `QueueLimits` and `QueueUsage` for non-evicting admission checks.

The modules are private; their public types are re-exported from the crate root. `delivery` composes the IDs and message facts, while storage, protocol, authentication, and scheduling layers are expected to apply the resulting domain decisions.

## Public interface and current behavior

- `MessageFacts::new` preserves immutable message ID, conversation, sender, recipient, sequence, and caller-supplied content binding. `classify` requires the same authenticated sender and message ID for duplicate/conflict handling.
- `Delivery::queued` and `Delivery::reduce` model queued, in-flight, failed, delivered, identity- or authorization-blocked, and canceled states without I/O or mutation. Receipt evidence must match the complete immutable facts, expected recipient device, and a durable-storage assertion before it can project `DeviceDelivered`.
- Retries retain the same `MessageFacts`, accumulate a non-wrapping attempt counter, and may change path. Replaying committed events reconstructs the same projection; stale attempt events are rejected.
- An unexpected identity change holds the message. Reverification only accepts the original expected recipient, so queued work is not redirected to a replacement identity. Cancellation stops future attempts but does not invalidate a valid late receipt from an earlier attempt.
- Queue admission rejects a new item when message or byte capacity would be exceeded; it does not evict existing retained work. Queue byte limits are stored-record accounting, not network-frame limits.

## Dependencies

Production code has no internal or external Cargo dependencies. The dev-only `proptest` dependency (default features disabled, `std` enabled) drives the property suite; `cargo tree -p kami-core --edges normal` confirms the production dependency boundary. Consumers supply ID generation, authentication, persistence transactions, retry scheduling, resource accounting, and transport integration.

## Test evidence

Tests remain organized by behavior. Unit and integration suites cover identifiers, message facts, delivery transitions, and queue pressure. KAMI-20 adds two focused delivery suites:

- `tests/delivery_properties.rs` contains five bounded `proptest` properties (512 cases each). Generated histories are limited to 0–24 events or retries and check immutable facts, pure rejection/snapshot preservation, committed-event replay, attempt-counter behavior, stale transport events, receipt binding, independent identity/authorization holds, cancellation, and late logical receipts.
- `tests/delivery_vectors.rs` contains seven named deterministic golden state vectors. At each checkpoint they assert state, attempts, path, and last failure for normal delivery; direct-to-relay retry; lost ACK; identity reverification; independent holds; cancellation; and invalid/stale events. The existing private `delivery` unit test remains the exhaustion-boundary evidence because the public API cannot seed the private counter safely.

The vectors model trusted adapter evidence and domain projections; they are not wire, cryptography, transport, storage, or I/O fixtures.

## Manual verification

From the repository root, the following commands have been verified:

```sh
cargo check --workspace
cargo build --workspace
cargo test --workspace
PROPTEST_RNG_SEED=20 cargo test -p kami-core --test delivery_properties
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo llvm-cov -p kami-core
cargo tree -p kami-core --edges normal
```

`cargo llvm-cov -p kami-core` observed 243/243 lines and 37/37 functions covered, with 251/252 regions (99.6%). LLVM currently reports **no instrumented branch counters**, so this coverage is execution evidence, not a claim of branch completeness.

## Limitations and deferred behavior

This crate does not generate collision-resistant IDs, authenticate identities or receipts, hash/encrypt content, persist data, enforce atomic database transactions, schedule retries, send network traffic, or prove a recipient stored anything. It does not define wire framing, frame-size validation, durable queue storage, ordering presentation, relay policy, or cryptographic/session behavior. Those responsibilities remain with higher-level protocol, security, storage, and transport components.
