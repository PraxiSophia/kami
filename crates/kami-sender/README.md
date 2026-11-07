# kami-sender

Runtime-neutral KAMI-24 coordination and KAMI-25 receipt consumption for durable outbound messages through the
quarantined `kami-e2ee` and SQLCipher `kami-storage` seam. It is a synchronous
service-layer crate: run its `Coordinator` on a blocking service task, not a network
executor.

## Responsibilities and public API

`Coordinator::enqueue` combines a `kami_storage::SendRequest`, a
`kami_e2ee::QuarantinedSession`, independently verified
`kami_identity::ContactTrust`, and caller-supplied `libp2p_identity::PeerId`
evidence. It first cleans expired reservations, reserves durable sequence state,
rechecks the persisted contact generation/pin, prepares the text stage, and commits
the reservation with the authenticated E2EE stage. `EnqueueOutcome::Committed`
returns a proposed in-memory session only after durable confirmation;
`EnqueueOutcome::Recovered` returns the exact stored queued frame without trying to
restore or guess a ratchet.

The crate re-exports the sender storage types used by this boundary:
`SendRequest`, `Preparation`, `PreparedSend`, `QueuedSend`, `SendLease`, and
`SendState`. `Coordinator::claim` performs a fresh durable trust and Peer-ID-binding
check before leasing exact stored bytes. `Coordinator::resume` requires explicit
reverification of the original pinned identity; it never adopts a replacement
identity. `retry` exposes the deterministic storage-backed retry constants and
backoff policy.

## Durable behavior

The storage-backed flow preserves a stable message identity, request binding,
sequence reservation, exact authenticated frame, attempt state, and retry
eligibility across process reopen. A durable reservation prevents competing work in
the same conversation/sender lane. Commit/recovery distinguishes a confirmed
commit, rollback, and indeterminate worker outcome so a losing or ambiguous E2EE
proposal is never installed speculatively.

The durable projection includes `Queued`, `InFlight`, `Exhausted`, `Canceled`,
`HeldIdentity`, and KAMI-25 `Delivered` (projected from the canonical message).
Leases fence concurrent attempts; failed attempts use a one-second exponential
backoff capped at five minutes. A changed or unverified identity holds the message;
cancellation stops future attempts while preserving immutable authenticated material
and original frame evidence. Restart recovery returns bounded ordered queued state
and retries the stored frame rather than re-encrypting it.

The current bound is eight attempts. Expired uncommitted preparation records are
retained tombstones and consume the configured count quota. Automatic retry policy
beyond exhaustion and tombstone compaction/capacity policy remain bounded follow-up
work.

## KAMI-25 authenticated receipt consumption

`Coordinator::receive_receipt(frame, Option<&QuarantinedSession>, end_peer, now)`
is a dedicated receipt-only path, exported with `ReceiptOutcome`. It requires a
DeviceDelivered frame, loads the original durable sender record and current trust,
checks reverse devices/conversation and supplied end-peer, then attempts exact
durable receipt recovery before decryption.

For a new receipt, `QuarantinedSession::prepare_receipt` authenticates it in the
same bidirectional session and binds it to the original immutable facts and exact
text-frame digest. Storage atomically commits the receipt ratchet, exact artifact,
receipt facts and canonical Delivered message state, retiring the attempt lease.
The coordinator always reads back durable evidence, including ambiguous replies.

- `ReceiptOutcome::Delivered { facts, proposed_session }` returns advancement only
  after an unambiguous new durable commit.
- `ReceiptOutcome::Duplicate(facts)` recovers exact committed evidence, including
  with `None` for the session; it does not reconstruct or install a mutable ratchet.
  Alternate receipt ciphertext is rejected, even for the same logical target.

Forged, wrong-device/peer, mismatched or conflicting receipts do not project
Delivered. A genuine late receipt can record delivery after cancellation; cancel
does not recall an already accepted message. Socket writes and relay acceptance
cannot enter this typed path as delivery evidence. See the integrated
[receiver tests](../kami-receiver/tests/receive.rs); run from repository root:

```sh
cargo test --locked -p kami-receiver --test receive
```

This command reference is source-checked, not newly executed in this documentation
task. Fresh profile ratchet restoration remains restricted without a qualified
rollback anchor. Bounded retained receipt/dedup evidence has no active cleanup or
compaction and fails closed at capacity. No exactly-once terminal rendering is
provided.

## Scope boundary

KAMI-24 does **not** activate production messaging, a sender runtime, timers,
network transport, direct/relay selection, live end-peer authentication,
production receipt/Delivered orchestration, end-to-end CLI delivery, profile restore,
or public/production encryption. A caller-provided `PeerId` is evidence for the
quarantined coordinator, not live network provenance. The `kami-e2ee` boundary
remains quarantined and its selected cryptographic/runtime qualification is separate.

Linux-only focused verification was recorded; Windows/macOS, real-network,
cross-platform, public-encryption, and external rollback-anchor qualification remain
deferred. KAMI-24 is not a production-readiness or owner-acceptance claim.

## Focused commands

Run from the repository root:

```sh
cargo test --locked -p kami-sender -p kami-storage
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
```

Recorded QA and Evaluator runs passed these focused sender/storage tests and the
formatting and warnings-denied Clippy checks on Linux. They do not establish the
deferred qualification gates above.