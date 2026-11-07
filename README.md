# Kami

Kami is a self-hostable, communication-first Rust platform for private communication between verified human endpoints and, later, AI-agent adapters. Kami is not an agent runtime, model router, shell executor, mailbox service, or task planner.

## Current development status

The repository contains foundations for M1 identity/trust, bounded protocol validation, encrypted local storage, quarantined KAMI-51 E2EE-to-storage composition, the runtime-neutral KAMI-24 sender seam, and the bounded KAMI-25 receiver/receipt path. KAMI-22, KAMI-23, KAMI-24, and bounded KAMI-25 behavior are functionally implemented; native Windows/macOS and broader platform/network qualification remain incomplete. This documentation synchronization inspects source/tests; it does not constitute new Rust test execution, independent review, owner acceptance, or Jira closure.

Status terms used in this repository:

- Functionally implemented: the Rust behavior exists in the current tree.
- Tested: the named automated or focused tests passed on the stated environment.
- Independently verified: a separate QA/Evaluator review inspected the implementation and evidence.
- Owner accepted: the owner explicitly accepted that bounded scope; this is not inferred from Jira status.
- Qualification pending: platform, network, interoperability, rollback, or security evidence is still required.
- Production ready: not claimed unless the complete M1 gates pass.

Current status is not production ready. Public/production encryption, network messaging, automatic startup, and relay delivery remain disabled or unimplemented.

See the complete implementation sequence and evidence map in [KAMI_DEVELOPMENT_ROADMAP.md](docs/KAMI_DEVELOPMENT_ROADMAP.md).

## M1 target

M1 targets two separately enrolled, independently verified, human-operated CLI endpoints exchanging one-to-one private UTF-8 text on Linux, Windows, and macOS. The intended path is:

1. protected identity and explicit contact verification;
2. authenticated Signal-style session handling with stable logical message identity;
3. encrypted sender outbox and recipient inbox/history;
4. authenticated direct P2P first, then an authorized live relay when direct delivery fails;
5. device-delivered acknowledgment only after recipient persistence and validation;
6. bounded retry, restart recovery, deduplication, ordering, and visible failure state.

M1 does not include offline mailboxes, groups, encrypted artifacts, federation, multi-device recovery, or agent task execution.

## Architecture and crate ownership

```text
kami-cli / local IPC
        |
kami-core + kami-protocol
        |
kami-identity + kami-e2ee
        |
kami-sender / kami-receiver -> kami-storage -> kami-key-provider
        |
background engine -> direct P2P / authorized live relay
```

- `kami-core`: dependency-free IDs, immutable message facts, delivery projections, and queue accounting.
- `kami-protocol`: bounded structural Protobuf frame codec and validators. It does not authenticate or encrypt.
- `kami-key-provider`: injected, load-only database-key custody contract with zeroizing key handling.
- `kami-identity`: protected device identity, enrollment contracts, fingerprints, contact trust, device-to-Peer-ID binding, and WP2-A1 identity-to-Olm binding.
- `kami-e2ee`: KAMI-51 WP1/WP2-A2/WP3 foundations and quarantined typed text/delivery/receipt composition. It has no transport, IPC, runtime, or production encryption activation.
- `kami-storage`: SQLCipher persistence, profile lease, worker serialization, schema-v5 migrations, outbox/inbox/history, trust CAS, private WP3 state, typed authenticated commits, canonical receipt artifacts and receiver gap observations.
- [`kami-sender`](crates/kami-sender/README.md): runtime-neutral durable KAMI-24 enqueue/recovery, identity holds, cancellation and retry eligibility, plus KAMI-25 `Coordinator::receive_receipt` for authenticated durable Delivered projection. No socket, timer, or transport.
- [`kami-receiver`](crates/kami-receiver/README.md): runtime-neutral KAMI-25 authenticated inbox admission, exact-frame dedup/recovery, atomic same-session DeviceDelivered receipt persistence and ordering/gap integration; no live receive loop or renderer.
- `kami-cli`: planned CLI and local IPC surface; final service integration remains downstream work.

The approved WP2-B dependency is `kami-storage -> kami-e2ee`; E2EE must not depend on storage. The authenticated storage seam accepts opaque `AuthenticatedCommit`, `AuthenticatedDelivery`, and `AuthenticatedReceiptCommit` stages; legacy neutral storage facts do not become authenticated delivery authority.

## Implemented foundations

- KAMI-19 workspace scaffold and package boundaries.
- KAMI-20 immutable logical-message facts, deterministic delivery reducer, retry identity, cancellation, identity holds, queue limits, property tests, and vectors.
- KAMI-21 bounded versioned Protobuf structural codec, strict parser, 65,536-byte encoded-frame cap, vectors, negative corpus, and allocation checks.
- KAMI-22 encrypted SQLCipher profile lease, worker, migrations, outbox/inbox/history/receipt/order seams, crash/restart/fault tests, quotas, and fail-closed key handling. Functionally implemented; Linux verification is present; Windows/macOS qualification remains pending.
- KAMI-23 protected identity/key-custody contracts and native adapters, plus explicit trust/binding interfaces. Functionally implemented; current tests are Linux/local unless stated otherwise; native cross-platform qualification remains pending.
- KAMI-50 explicit fingerprint verification, trust transitions, changed-identity holds, and device-to-Peer-ID verification.
- KAMI-51 WP1, WP2-A1, WP2-A2, WP3, and quarantined WP2-B typed E2EE-to-storage composition, implemented in that order. WP3 is accepted as a bounded private storage foundation; WP2-B has conditional owner acceptance within its quarantined scope, with implementation and independent QA/Evaluator evidence. The full KAMI-51 issue remains incomplete and production activation is a separate gate.
- KAMI-24 runtime-neutral sender coordination: durable authenticated enqueue/recovery, exact-frame attempt leases, bounded retry state, cancellation, and changed-identity holds. It was tested and independently QA/Evaluator reviewed on Linux; it does not activate production messaging or E2EE.
- KAMI-25 bounded receiver/receipt implementation: authenticated inbox admission, atomic text-plus-same-session receipt persistence, exact replay after reopen without a live ratchet, dedup/conflict rejection, ordering/gap reconciliation, quotas, and sender receipt consumption with durable Delivered projection. Source and local test coverage are described in the receiver, E2EE, storage and sender READMEs; this is not a complete runtime or M1 delivery claim.

The recorded Rust workspace gates for the KAMI-51 foundation passed on the tested Linux checkout, including locked tests, workspace tests, all-target checking, formatting, Clippy, diff checks, and authenticated boundary fixtures. Two existing native secure-store probes were ignored because their external keyring/DBus prerequisites were unavailable.

## Current limitations

- The general provider encryption API remains fail-closed; Olm-v1 staging is quarantined development behavior.
- No live end-peer provenance from the supervised runtime exists yet.
- No production prekey publication, rotation, freshness, or replenishment service exists.
- No external rollback anchor exists; coherent old database/WAL copies are not detected by database-only generations or digests.
- The accepted in-process snapshot custodian can technically consume full provider pickles, including private prekeys; this is not protection against a malicious custodian, native copies, dumps, or endpoint compromise.
- Comprehensive zeroization, cross-platform secure-store behavior, interoperability, and real-network qualification remain open.
- No sender/receiver runtime, direct transport, relay service, automatic fallback, local IPC, or production receipt/Delivered integration is enabled.
- KAMI-24 uses a finite eight-attempt retry state and retains preparation tombstones under a bounded quota. The longer-term automatic retry/until-delivery and tombstone/compaction policy remain follow-up work.
- KAMI-25 exact-frame retry rejects alternate ciphertext even for the same logical content. Receipt/dedup evidence remains bounded, with no active cleanup/compaction; capacity exhaustion fails closed rather than dropping evidence.
- Fresh mutable cryptographic session restoration remains restricted without a qualified external rollback anchor. Exact durable ACK/Delivered recovery is not profile ratchet restoration or permission to resume arbitrary new cryptographic work.
- Durable presentation eligibility does not guarantee exactly-once terminal rendering. No production, real-network, or cross-platform qualification is established by the bounded receiver/receipt path.

## Canonical documentation

- [Project requirements](docs/Kami_Project_Requirements.md)
- [Communication design](docs/Kami_Communication_Design.md)
- [Identity and encryption architecture](docs/architecture/Identity_And_Encryption.md)
- [Messaging protocol architecture](docs/architecture/Messaging_Protocol.md)
- [Complete development roadmap](docs/KAMI_DEVELOPMENT_ROADMAP.md)
