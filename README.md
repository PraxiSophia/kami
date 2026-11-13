# Kami

Kami is a self-hostable, communication-first Rust platform for private communication between verified human endpoints and, later, AI-agent adapters. Kami is not an agent runtime, model router, shell executor, mailbox service, or task planner.

## Current development status

The repository contains foundations for M1 identity/trust, bounded protocol validation, encrypted local storage, quarantined KAMI-51 E2EE-to-storage composition, KAMI-24 sender and KAMI-25 receiver/receipt seams, KAMI-26 supervised runtime, and KAMI-27 authenticated direct connections. KAMI-27 is owner-accepted within its bounded Linux localhost direct-transport scope; it does not activate production messaging. KAMI-51 WP-A is owner-accepted with limitations for its authenticated-context and provider-contract scope. WP-B is bounded accepted with conditions for prekey lifecycle and authenticated direct prekey-only exchange; independent QA passed and final evaluation returned PASS-WITH-CONDITIONS. WP-C is now owner-accepted only as a bounded recovery foundation with conditions/limitations (implementation `t_e3a1ba25` run 229; QA `t_e022f9b4` run 230 PASS; Evaluator `t_10759b1a` run 231 PASS-WITH-CONDITIONS). The full KAMI-51 issue remains incomplete. The owner completed the two-process manual QUIC/TCP authentication and reconnect test for KAMI-27, not WP-B. Native Windows/macOS, cross-network, relay/NAT, and broader platform qualification remain incomplete.

Status terms used in this repository:

- Functionally implemented: the Rust behavior exists in the current tree.
- Tested: the named automated or focused tests passed on the stated environment.
- Independently verified: a separate QA/Evaluator review inspected the implementation and evidence.
- Owner accepted: the owner explicitly accepted that bounded scope; this is not inferred from Jira status.
- Qualification pending: platform, network, interoperability, rollback, or security evidence is still required.
- Production ready: not claimed unless the complete M1 gates pass.

Current status is not production ready. Public/production encryption, network messaging, automatic startup, and relay delivery remain disabled or unimplemented.

See the complete implementation sequence and evidence map in [KAMI_DEVELOPMENT_ROADMAP.md](docs/KAMI_DEVELOPMENT_ROADMAP.md).

## Accepted bounded milestones

- **KAMI-27:** owner-accepted direct authenticated QUIC/TCP transport on Linux localhost, including durable enrollment, fingerprint/trust verification, Peer-ID binding, supervised lifecycle, disconnect/reconnect, and the owner-operated two-process test. Relay, NAT traversal, cross-network/cross-platform qualification, application delivery, and production E2EE remain outside this acceptance.
- **KAMI-51 WP-A:** owner-accepted with limitations. The retained vodozemac 0.11.1 provider uses the explicit `NativeAadProvider` contract with `E2eeProvider` compatibility, keeps `supports_aad=false`/`AadUnsupported`, and authenticates canonical `InnerEnvelopeV1` context through the Olm-protected plaintext plus exact post-decryption checks. The outer SHA-256 is a correspondence commitment, not an independent MAC. Staged values and exact-frame retry are not durable commit or delivery authority; private unbound helpers are test-only and negative-security tests cover the fail-closed boundary. Supplied trust/end-peer context alone is not live runtime provenance, and the trusted snapshot custodian can consume full provider pickles, including private prekeys.
- **KAMI-51 WP-B:** bounded accepted with conditions. Implementation `t_d364301b`, run 204; independent QA `t_0dcc13ac`, run 205 PASS; final Evaluator `t_250e6416`, run 206 PASS-WITH-CONDITIONS, on the then-uncommitted tree at baseline `438d466603f180efcd02816ed8a69be6aaa17391`. Opaque account lifecycle, identity-owned per-selection signing, SQLCipher v7 inventory/global OTK CAS and exact publication ledgers compose with same-swarm authenticated QUIC/TCP prekey-only exchange. A response carries one signed selection, not a batch. Freeze-before-send and retain-before-publication-ACK protect exact-byte retry/reconciliation; these ACKs are not message-delivery receipts. The [WP-B ignored endpoint and fixture runner](scripts/kami51-wpb-manual/README.md) now expose prekey/status/exact-retry commands; interface implementation and automated review do not establish owner manual acceptance.
- **KAMI-51 WP-C:** owner-accepted only as a bounded recovery foundation with conditions/limitations. Every storage-worker create/open starts with process-local default-deny mutable authority. Native one-use `FreshEnrollment` is consumed through a worker-local handoff bound to the actual encrypted DB and completed/readback-verified before admission. Restart or quarantine cannot reactivate account/prekey/session authority from database rows, counters, digests, or old completion records. Exact committed ciphertext, inbox/frame, receipt/retry and Delivered artifacts can reconcile without re-encryption or ratchet advancement. This refuses unsafe restoration; it does not detect coherent rollback or supply an independent rollback witness. Typed re-establishment preconditions do not implement live fresh-session establishment or transparent mutable-session restoration.

These milestones do not enable public production encryption or application messaging. Remaining gates include WP-C's external freshness/mutable-restoration and live re-establishment limitations, WP-D integrated qualification, and WP-B's residual conditions; KAMI-28 first-message delivery and M1 are not completed. WP-C implementation `t_e3a1ba25` run 229, QA `t_e022f9b4` run 230 PASS and Evaluator `t_10759b1a` run 231 PASS-WITH-CONDITIONS are reused here, not rerun: workspace **284 passed, 0 failed, 3 existing ignored**, focused recovery 6 passed, with formatting/locked all-target check/Clippy/diff gates passing. This is Linux private-D-Bus/native qualification only; Windows/macOS, cross-network, interoperability, sustained fuzz/DoS, load and owner-operated WP-C endpoint evidence remain unqualified.

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
kami-runtime supervised Tokio engine
        |
kami-transport direct authenticated connections (KAMI-27)
```

- `kami-core`: dependency-free IDs, immutable message facts, delivery projections, and queue accounting.
- `kami-protocol`: bounded structural Protobuf frame codec and validators. It does not authenticate or encrypt.
- `kami-key-provider`: injected, load-only database-key custody contract with zeroizing key handling.
- `kami-identity`: protected device identity, enrollment contracts, fingerprints, contact trust, device-to-Peer-ID binding, and WP2-A1 identity-to-Olm binding.
- `kami-e2ee`: KAMI-51 WP1/WP2-A1/WP2-A2/WP3 foundations, quarantined WP2-B typed E2EE-to-storage composition, WP-A authenticated-context/provider-contract correction, and WP-B opaque account/prekey lifecycle staging. It has no transport, IPC, runtime, or production encryption activation.
- `kami-storage`: SQLCipher persistence, profile lease, worker serialization, schema-v7 migrations, outbox/inbox/history, trust CAS, private WP3 state, typed authenticated commits, canonical receipt artifacts, receiver gap observations, immutable enrollment, WP-B inventory/publication/retention CAS and WP-C process-local authority/exact-artifact recovery. WP-C adds no migration 008.
- [`kami-sender`](crates/kami-sender/README.md): runtime-neutral durable KAMI-24 enqueue/recovery, identity holds, cancellation and retry eligibility, plus KAMI-25 `Coordinator::receive_receipt` for authenticated durable Delivered projection. No socket, timer, or transport.
- [`kami-receiver`](crates/kami-receiver/README.md): runtime-neutral KAMI-25 authenticated inbox admission, exact-frame dedup/recovery, atomic same-session DeviceDelivered receipt persistence and ordering/gap integration; no live receive loop or renderer.
- [`kami-transport`](crates/kami-transport/README.md): bounded authenticated direct-connection substrate over QUIC/TCP, identity-bound peer validation, revocable capabilities and fixed prekey-only exchange; no application payload, delivery receipts, relay, or NAT traversal.
- [`kami-runtime`](crates/kami-runtime/README.md): bounded Tokio ownership, blocking storage/key-provider isolation, profile lease ownership, enrollment verification, direct-transport supervision, shutdown/reconciliation, and relay-renewal scheduling seam. No production messaging, relay, IPC, or CLI activation.
- `kami-cli`: planned CLI and local IPC surface; final service integration remains downstream work.

The approved WP2-B dependency is `kami-storage -> kami-e2ee`; E2EE must not depend on storage. The authenticated storage seam accepts opaque `AuthenticatedCommit`, `AuthenticatedDelivery`, and `AuthenticatedReceiptCommit` stages; legacy neutral storage facts do not become authenticated delivery authority.

## Implemented foundations

- KAMI-19 workspace scaffold and package boundaries.
- KAMI-20 immutable logical-message facts, deterministic delivery reducer, retry identity, cancellation, identity holds, queue limits, property tests, and vectors.
- KAMI-21 bounded versioned Protobuf structural codec, strict parser, 65,536-byte encoded-frame cap, vectors, negative corpus, and allocation checks.
- KAMI-22 encrypted SQLCipher profile lease, worker, migrations, outbox/inbox/history/receipt/order seams, crash/restart/fault tests, quotas, and fail-closed key handling. Functionally implemented; Linux verification is present; Windows/macOS qualification remains pending.
- KAMI-23 protected identity/key-custody contracts and native adapters, plus explicit trust/binding interfaces. Functionally implemented; current tests are Linux/local unless stated otherwise; native cross-platform qualification remains pending.
- KAMI-50 explicit fingerprint verification, trust transitions, changed-identity holds, and device-to-Peer-ID verification.
- KAMI-51 WP1, WP2-A1, WP2-A2, WP3, quarantined WP2-B, WP-A authenticated-context/provider-contract correction, WP-B bounded prekey lifecycle/exchange, then WP-C bounded recovery foundation. WP3 is accepted as a bounded private storage foundation; WP2-B has conditional owner acceptance within its quarantined scope; WP-A is accepted with limitations; WP-B and WP-C are bounded accepted with conditions/limitations. The full KAMI-51 issue remains incomplete and production activation is a separate gate.
- KAMI-24 owner-accepted bounded runtime-neutral sender coordination: durable authenticated enqueue/recovery, exact-frame attempt leases, bounded retry state, cancellation, and changed-identity holds. It was tested and independently QA/Evaluator reviewed on Linux; it does not activate production messaging or E2EE.
- KAMI-25 bounded receiver/receipt implementation: authenticated inbox admission, atomic text-plus-same-session receipt persistence, exact replay after reopen without a live ratchet, dedup/conflict rejection, ordering/gap reconciliation, quotas, and sender receipt consumption with durable Delivered projection. Source and local test coverage are described in the receiver, E2EE, storage and sender READMEs; this is not a complete runtime or M1 delivery claim.
- KAMI-26 bounded supervised runtime and KAMI-27 accepted bounded direct connection substrate: the runtime retains profile lease ownership, verifies enrollment/custody before issuing credentials, supervises direct authenticated connections, and revokes runtime capabilities. Independent QA and final Evaluator passed focused Linux checks, and the owner completed the repository manual QUIC/TCP authentication and reconnect workflow. Direct transport is not application delivery: payload/ACK/receipt/Delivered, production E2EE/session restoration, relay/NAT/IPC activation remain absent.

Historical KAMI-51 foundation gates passed on the tested Linux checkout with two native secure-store probes ignored. The current reused WP-C run-230 workspace result is 284 passed, 0 failed, 3 existing ignored (the two opt-in native probes and owner-operated endpoint). Nested qualification probes are checked by their outer tests, not counted twice. No Rust gates were rerun by this documentation synchronization.

## Current limitations

- The general provider encryption API remains fail-closed; Olm-v1 staging is quarantined development behavior.
- No live production messaging ingress from the supervised runtime exists yet; KAMI-27's direct transport now supplies authenticated end-peer evidence only within its bounded connection scope.
- WP-B supplies bounded prekey publication, rotation, freshness and replenishment, not a production-ready service. Under WP-C, after process restart only valid exact frozen exchanges may reconcile; new publication from an existing account remains blocked. WP-C supplies no mutable restore authority. Exposed/expired inventory and capped ledgers are retained, not silently recycled or compacted.
- No external rollback anchor exists; coherent old database/WAL copies are not detected by database-only generations or digests.
- The accepted in-process snapshot custodian can technically consume full provider pickles, including private prekeys; this is not protection against a malicious custodian, native copies, dumps, or endpoint compromise.
- Comprehensive zeroization, cross-platform secure-store behavior, interoperability, and real-network qualification remain open.
- No sender/receiver production runtime, relay service, automatic fallback, local IPC, or production receipt/Delivered integration is enabled. KAMI-27 direct transport is the bounded exception: it is implemented and owner-accepted, but carries no application messages.
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
