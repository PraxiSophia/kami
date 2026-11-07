# kami-receiver

Runtime-neutral KAMI-25 authenticated inbox coordination over the quarantined
[kami-e2ee](../kami-e2ee/README.md) and SQLCipher
[kami-storage](../kami-storage/README.md) boundaries. This synchronous service-layer
crate owns no sockets, timers, transport, relay, CLI, or runtime. Run it on a
blocking service task, not a network executor.

## Modules, dependencies, and public API

- `context`: `ReceiveContext::new(local, remote, conversation, trust, end_peer,
  now_ms)` validates admission context. The trusted caller supplies current
  `ContactTrust` and the authenticated **end-peer**, never a relay identity.
  Context alone does not authenticate a frame.
- `receive`: `Receiver::new(&Storage)` provides `receive(frame, context,
  Option<&QuarantinedSession>, receipt_id)` and `receive_first(frame, context,
  Account, local_endpoint, remote_endpoint, receipt_id)`.
- `ReceiveOutcome::New { delivery, proposed_session }` permits installing the
  proposed in-memory session only after unambiguous durable confirmation.
  `ReceiveOutcome::Duplicate { delivery }` returns retained evidence, not a new
  mutable ratchet. Discard losing or uncertain proposals.
- `DurableDelivery` is re-exported with `facts()`, `acceptance()`, and `receipt()`;
  `GapState` and `OrderedMessage` are also storage re-exports.
- `error`: public `Error::{InvalidInput, Unauthorized, Conflict,
  SessionUnavailable, CryptoRejected, Storage, Indeterminate}`. The last two wrap
  redacted storage errors. `Indeterminate` is not permission to install a proposal
  or emit an unconfirmed receipt.

Normal dependencies are `kami-core`, `kami-protocol`, `kami-storage`, `kami-e2ee`,
`kami-identity`, identity-only `libp2p-identity`, and pinned `vodozemac = 0.11.1`.
Sender integration is exercised through a dev dependency, not a receiver-to-sender
runtime dependency.

## Receive, durable admission, and receipt flow

1. Decode the bounded protocol frame and require Text, expected conversation, and
   expected sender/recipient devices. Compare caller trust with the current durable
   Verified contact generation and exact pin.
2. Attempt exact durable recovery with an E2EE `ReplayPermit` before decrypting.
   An accepted replay returns the original receipt even with no live session.
3. For new text, `receive` needs an existing session; `receive_first` consumes an
   account and endpoint selections for first-inbound prekey authentication.
   E2EE validates inner/outer correspondence, identity/bundle/peer/session binding,
   and stages text reception plus a DeviceDelivered reply in the **same**
   bidirectional session.
4. `Storage::commit_delivery` atomically commits account/bootstrap or one-time-key
   effects, both session-generation transitions, inbox/dedup/history facts, exact
   text and receipt frames, and gap observations. Fallback does not consume a
   one-time key. Failure grants no ACK eligibility.
5. Read back exact durable evidence, including after an ambiguous worker reply.
   Only an unambiguous new commit returns the proposed session. A reconciled
   ambiguous result returns Duplicate without guessing mutable state.

A receipt frame visible during E2EE staging is **not** eligible for transmission.
`DurableDelivery::receipt()` supplies committed exact DeviceDelivered bytes, not
proof of network transmission, human reading, or sender receipt validation. The
future runtime must serialize current trust/provenance checks with actual ACK
emission. The sender's dedicated receipt path durably projects Delivered only
following authenticated receipt validation.

## Deduplication and restart recovery

Authenticated sender plus logical message ID identifies the durable message;
immutable facts, sequence, content binding, original frame, receipt artifact and
account/session links must agree. Same-ID conflicting content or **alternate
ciphertext even for the same plaintext** is rejected: this is an exact-frame retry
policy, not arbitrary re-encryption support. Concurrent duplicate commits retain
one message/artifact and never install a losing ratchet.

A crash before commit yields no eligible ACK. After commit, a lost ACK or worker
reply can be recovered by replaying the exact original frame, including after
storage reopen or local history deletion. `receive(..., None, ...)` supports this
recovery only for retained accepted evidence; new text without a session fails
with `SessionUnavailable`. Reopen does not restore a fresh mutable cryptographic
session: profile restoration remains restricted without a qualified external
rollback anchor. DB-only counters/digests cannot detect coherent old DB/WAL rollback.

## Ordering and capacity

Storage owns per-sender/per-conversation presentation through `ordered_history`,
not the receiver outcome. The wait is configurable from 0 through 60,000 ms;
`OrderedMessage` exposes `gap_before` and `gap_state`. Unresolved observations
become Permanent at 60,000 ms from local receipt when ordering is queried, but
late predecessors still reconcile them. Visibility survives clock rollback/reopen;
retained facts prevent local history deletion from fabricating a network gap.
There is no global ordering or exactly-once terminal rendering guarantee.

Admission bounds include 65,536 encoded frame bytes, 1,024 retained inbound
messages per sender/contact, 512 per sender/conversation, a maximum forward
sequence distance of 4,096 from the retained high sequence, and 128 outstanding
gap observations per sender (including Permanent observations until resolved).
A predecessor resolving a gap can be admitted at the gap cap. Shared configured
count/byte limits count retained authenticated frames, receipt artifacts and
history payloads; worker queue and database-page limits also apply. Capacity
failure returns Full without partially accepting text or granting ACK eligibility.
Dedup/receipt evidence is retained and bounded: no active cleanup/compaction is
implemented, and history deletion does not reclaim all evidence or quota.

## Source coverage and commands

[`tests/receive.rs`](tests/receive.rs) exercises sender enqueue through receiver
persistence, exact re-ACK after reopen without a live ratchet, forged receipts,
and sender Delivered projection including a genuine late ACK after cancellation.
It includes [`tests/failures.rs`](tests/failures.rs), covering insert-fault rollback,
concurrent duplicates/trust changes, conflicts, alternate ciphertext, gap
reconciliation, history deletion, process kills before/after commit, corruption,
legacy isolation, and capacity rejection. `autotests = false`: `receive` is the
explicit integration-test target; `failures` is not a separate Cargo target.

Commands from repository root (source-checked references; not rerun in this
documentation-only synchronization):

```sh
cargo test --locked -p kami-receiver --test receive
cargo test --locked -p kami-sender -p kami-receiver -p kami-e2ee -p kami-storage
cargo test --locked -p kami-storage --test receiver_migration
```

## Qualification limits

Live end-peer provenance remains caller/future-runtime supplied. No supervised
runtime, direct/relayed network path, offline mailbox, automatic receipt scheduler,
CLI rendering, production encryption activation, fresh profile ratchet restoration,
or production/real-network/cross-platform qualification is established here.
Process-kill fixtures do not certify power-loss durability. Olm-v1 and upstream
skipped-key windows remain quarantined; prekey lifecycle, rollback authority and
interoperability require separate qualification. Local receipt persistence and
presentation eligibility do not promise exactly-once external side effects.
