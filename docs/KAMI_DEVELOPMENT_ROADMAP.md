# Kami Development Roadmap

Status: documentation synchronized with bounded KAMI-27 and KAMI-51 WP-A owner acceptance; no production or M1 completion claim
Last repository inspection: 2026-11-10 (KAMI-27 and WP-A acceptance, implementation/QA/Evaluator evidence, manual suite, and Git state)
Last Jira evidence read: 2026-11-08 (KAMI-27 through KAMI-32 and KAMI-51; read-only manager path)
Implementation language: Rust only

This roadmap is the default local planning reference for Kami. It summarizes repository evidence, recorded KAMI-51 implementation/review evidence, canonical requirements, and the latest reconciled Jira reads. Jira remains authoritative for Jira issue fields, workflow status, and issue links; this document does not change or replace Jira.

## 1. Project goals and fixed constraints

Kami is a self-hostable, communication-first platform for authenticated private communication between human endpoints and future least-privilege AI-agent adapters. M1 is two separately enrolled, independently verified human-operated CLI endpoints exchanging one-to-one private UTF-8 text on Linux, Windows, and macOS.

M1 requires:

- protected device identity and explicit bilateral fingerprint verification;
- cryptographic binding from the verified device to the authenticated rust-libp2p Peer ID;
- an established authenticated Signal-style/Double-Ratchet implementation, not custom cryptography;
- encrypted local sender outbox and recipient inbox/history;
- stable logical message ID and sender sequence across retries and path changes;
- direct P2P first, then authorized live-relay fallback;
- recipient persistence before `DEVICE_DELIVERED` acknowledgment;
- bounded retries, restart recovery, deduplication, ordering, queue limits, and truthful failure state;
- manually started service and authenticated local IPC;
- qualification on the supported operating systems and real network profiles.

M1 does not include offline server mailboxes, groups, encrypted artifacts, federation, multi-device recovery, automatic startup, or agent task execution.

Fixed architectural constraints:

- Rust-only implementation.
- No custom cryptographic primitives or unreviewed protocols.
- No plaintext key fallback.
- No silent trust-on-first-use or identity replacement.
- Relays forward live ciphertext; they are not durable offline mailboxes.
- Incoming messages never grant command or tool-execution authority.
- Storage owns SQLCipher, leases, migrations, worker serialization, quotas, CAS, and transaction atomicity.
- Identity owns protected identity custody, trust, fingerprints, and device/Peer-ID binding.
- E2EE owns authenticated session composition and opaque authenticated commit construction.
- The approved WP2-B dependency is `kami-storage -> kami-e2ee`; E2EE must not import storage.
- No public neutral trusted-write path, raw database API, raw private-key getter, or public snapshot import path.
- Owner approval is required separately for repository files, Jira, Git, GitHub, Obsidian, Kanban, commits, pushes, merges, and deployment.

## 2. Status vocabulary

The roadmap distinguishes facts that are often conflated:

- Functionally implemented: the behavior exists in the current Rust tree.
- Tested: named automated or focused tests passed on the named environment.
- Independently verified: a separate QA/Evaluator inspected implementation and evidence.
- Owner accepted: the owner explicitly accepted that bounded scope.
- Qualification pending: platform, network, interoperability, rollback, or security evidence remains.
- Production ready: the complete M1 gates pass; no current subsystem is marked production ready.

Jira workflow status is a project-record state, not a direct measurement of core implementation completeness. In particular, KAMI-22 and KAMI-23 are functionally implemented while their Jira status remains In Progress because native Windows/macOS and broader qualification remain incomplete.

## 3. Major subsystem ownership

| Subsystem | Crate/task owner | Current boundary |
|---|---|---|
| Domain IDs, facts, delivery projection, queue accounting | `kami-core`, KAMI-20 | Pure Rust; no storage, crypto, runtime, or transport. |
| Structural wire codec | `kami-protocol`, KAMI-21 | Strict bounded Protobuf structure; not authentication or encryption. |
| Database key contract | `kami-key-provider` | Injected, synchronous, load-only key provider; no OS implementation in the trait. |
| Identity and custody | `kami-identity`, KAMI-23 | Protected identity, enrollment contracts, native adapters, fingerprints, trust, Peer-ID and Olm bindings. |
| Contact verification | KAMI-50 | Explicit bilateral fingerprint verification, durable trust transitions, changed-identity holds, device/Peer-ID proof. |
| E2EE/session composition | `kami-e2ee`, KAMI-51 | KAMI-51 WP1/WP2-A/WP3 and quarantined WP2-B typed authenticated boundary; no runtime/network activation. |
| Encrypted persistence | `kami-storage`, KAMI-22/WP3 | SQLCipher, lease, worker, migrations, outbox/inbox/history, receipts/order facts, CAS, quotas, atomic transactions. |
| Sender lifecycle | `kami-sender`, KAMI-24/KAMI-25 | Durable enqueue, retry/recovery, cancellation, queue state, exact logical-message retention, authenticated receipt consumption and Delivered projection. Not runtime or transport. |
| Receiver lifecycle | `kami-receiver`, KAMI-25 | Implemented bounded durable inbox/dedup, ordering/gaps, exact replay and same-session authenticated delivery receipt persistence. No live receive loop. |
| Supervised runtime | `kami-runtime`, KAMI-26 | Verified bounded Tokio owner, blocking storage/key seams, profile ownership, cancellation, shutdown, durable restart reconciliation, and relay-renewal scheduling. No live transport or production authenticated ingress. |
| Direct transport | KAMI-27 | Owner-accepted bounded authenticated rust-libp2p direct path with Peer-ID validation; Linux localhost qualification only. |
| Relay service/session | KAMI-29/KAMI-30 | Default-deny live relay, reservations, inner authenticated endpoint session. |
| Path fallback | KAMI-31 | Direct-first selection, relay failover, same logical message across paths. |
| CLI/local IPC | KAMI-32 | Unix sockets/named pipes, service control, authorization, diagnostics, rendering. |
| Harness/evidence | KAMI-33 | Disposable processes, fault schedules, topology/path evidence, redaction. |
| Local matrix | KAMI-34 | Same-host correctness, resilience, privacy, and failure matrix. |
| Network qualification | KAMI-35 | Real separate-network direct and genuine automatic relay fallback. |
| Regression matrix | KAMI-36 | Retry, ACK, identity, relay, restart, malformed input, and truthful failure outcomes. |
| Privacy/diagnostics | KAMI-37 | Redacted storage/log/relay/CLI output and ciphertext-forwarding boundaries. |
| Independent milestone review | KAMI-39 | Evidence review and bounded M1 claim; missing evidence blocks the claim. |

## 4. Completed foundations and evidence

### KAMI-19 — workspace skeleton

Jira status: Done, last read 2026-11-03.

The Rust workspace and package boundaries exist. The scaffold intentionally did not claim network, identity, or plaintext-key behavior.

Evidence: workspace manifests, lockfile, package boundaries, and canonical repository documentation.

### KAMI-20 — core IDs, delivery facts, and reducer

Jira status: Done, last read 2026-11-03.

Functionally implemented and tested. `kami-core` provides opaque IDs, immutable `MessageFacts`, duplicate/conflict classification, deterministic delivery projections, retry attempt handling, identity/authorization holds, cancellation, queue accounting, property tests, and golden vectors. It does not authenticate, persist, encrypt, schedule, or send.

### KAMI-21 — bounded protocol codec and validators

Jira status: Done, last read 2026-11-03.

Functionally implemented and tested on Linux. The codec enforces canonical structural Protobuf rules and a 65,536-byte encoded-frame cap with vectors, negative corpus, properties, and allocation checks. It does not prove identity, E2EE authenticity, durable receipts, or delivery.

### KAMI-22 — profile lease, SQLite worker, and migrations

Jira status: In Progress, last read 2026-11-03. This reflects pending native Windows/macOS and broader qualification, not unfinished core Rust behavior.

Functionally implemented. The crate contains the exclusive profile lease, SQLCipher-only storage, injected key provider, migrations, single worker, bounded commands, encrypted outbox/inbox/history, receipts, ordering, quotas, cancellation, crash/restart seams, fault rollback, and storage artifact checks.

Tested: focused and workspace Rust evidence passed on the tested Linux checkout. Independently verified: implementation/review evidence exists for the relevant storage work, but this roadmap does not convert Jira status into a separate KAMI-22 acceptance claim. Owner accepted: not inferred here. Qualification pending: native Windows/macOS secure-store/filesystem behavior, broader OS qualification, power-loss/filesystem certification, and production integration.

### KAMI-23 — protected identity initialization and peer trust

Jira status: In Progress, last read 2026-11-03. This reflects pending native/platform qualification, not unfinished core Rust behavior.

Functionally implemented. `kami-identity` supplies protected identity/key custody seams, enrollment contracts, native adapters, fingerprints, explicit trust transitions, device-to-Peer-ID proof, and WP2-A1 identity-to-Olm binding. It fails closed on locked, missing, unavailable, corrupt, or changed custody and never exposes private identity keys.

Tested: focused identity, trust, binding, custody, and storage adapter tests passed on the tested Linux checkout. Independently verified: existing implementation evidence exists; this documentation task did not perform a new independent security review. Qualification pending: native Windows/macOS behavior, Linux logged-in secure-store coverage, cross-process enrollment/lease integration, live transport provenance, and full M1 qualification.

### KAMI-50 — contact fingerprint verification and identity trust binding

Jira status: Done, last read 2026-11-03.

Functionally implemented and tested. It imports invitations as untrusted data, derives canonical fingerprints, requires explicit bilateral verification, persists trust transitions, holds queued sends on changed identity, and verifies device-to-Peer-ID binding. The commit is present on the pushed `identity-security` branch but is not merged to main.

### KAMI-51 — authenticated session and crash-safe ratchet persistence

Jira status: last manager evidence read 2026-11-08; Jira was not silently changed to match implementation evidence. The full issue remains incomplete.

Recorded work-package status and evidence:

The actual implementation order was **WP1 -> WP2-A1 -> WP2-A2 -> WP3 -> WP2-B -> WP-A**.
WP4 and WP5 are downstream planning groups, not prerequisites for accepting these
foundations.

- WP1: implemented and independently reviewed within the bounded provider/profile scope. Implementation `t_9da66f08`; QA `t_36a44ee9`; Evaluator `t_aab9a8e3`.
- WP2-A1: implemented and independently reviewed within the identity/Olm-binding scope. Implementation `t_f348cf2d`; QA `t_f5da1af2`; Evaluator `t_cf4c39d6`.
- WP2-A2: implemented and independently reviewed within the authenticated-envelope scope. Implementation `t_501a91b6`; QA `t_707f16fb`; Evaluator `t_099f6098`.
- WP3: implemented, independently QA-verified, independently security-evaluated, and owner accepted as a bounded private storage foundation. Implementation `t_fd283614`; QA `t_2d834cf1`; Evaluator `t_82d8776a`. Acceptance covers the quarantined storage scope, not production rollback protection or runtime activation.
- WP2-B: implemented and independently reviewed within the conditional quarantined scope. Implementation run 126 on task `t_b8cb3f89`; QA task `t_48119cc5`, run 127; Evaluator task `t_21f65076`, run 128. The owner-facing recommendation was **ACCEPT WITH CONDITIONS** for WP2-B only.
- WP-A: implemented and accepted with limitations as the bounded authenticated-context/provider-contract package. Implementation `t_b295f09f`, run 187; independent QA `t_d92abbc2`, run 188; final security Evaluator `t_e03aec4f`, run 189; acceptance checkpoint Kanban comment 176. It retains vodozemac 0.11.1, makes `NativeAadProvider` explicit while preserving `E2eeProvider` compatibility, retains `supports_aad=false`/`AadUnsupported`, preserves canonical Olm-authenticated `InnerEnvelopeV1` context checks, and keeps public production encryption fail-closed.

WP2-B consumes the accepted WP3 storage foundation and the preceding WP1/WP2-A
contracts. WP-A corrects and qualifies the provider/context boundary; it does not
connect E2EE to the producer-owned KAMI-27 authenticated transport capability.
KAMI-24 may consume this accepted KAMI-51 foundation without waiting for the full
KAMI-51 parent issue, WP4, or WP5 to be complete. None of these bounded acceptances
marks the full KAMI-51 Jira issue complete.

Pending: live runtime end-peer provenance, WP-B production prekey lifecycle, WP-C durable session recovery and rollback policy, production orchestration of the bounded KAMI-25 receipt/Delivered seam, external rollback authority, service race guards, cross-platform/interoperability qualification, WP-D integrated qualification, and production security review.

### KAMI-51 WP-A — accepted with limitations

WP-A is the current owner-accepted bounded milestone for the authenticated-context and provider-contract correction. It builds on the earlier WP1/WP2-A1/WP2-A2/WP3 and quarantined WP2-B evidence but does not re-accept or replace those work packages. The retained `vodozemac = 0.11.1` adapter implements the explicit
`NativeAadProvider` contract while remaining compatible with `E2eeProvider`; its
`supports_aad()` is false and public calls remain fail-closed with `AadUnsupported`.
The bounded path instead authenticates a canonical `InnerEnvelopeV1` inside Olm,
performs exact post-decryption context and correspondence checks, and uses the outer
SHA-256 only as a correspondence commitment. Staged values and exact-frame retry are
not durable commit or delivery authority; test-only unbound helpers and negative-
security fixtures do not create a production bypass.

The supplied trust/end-peer/clock/context values are checked structurally, but pure
E2EE code cannot establish live runtime provenance or snapshot freshness. The
in-process snapshot custodian can consume full provider pickles, including private
prekeys, so custody is trusted and not a malicious-custodian or endpoint-compromise
defense. `ProfileRestore` fails closed without an external rollback anchor. WP-A does
not enable public/production E2EE, application delivery, ACK/Delivered authority, or
M1 completion.

Evidence: implementation `t_b295f09f`, run 187; independent QA `t_d92abbc2`, run
188; final security Evaluator `t_e03aec4f`, run 189; acceptance checkpoint Kanban
comment 176. The required formatting, workspace check, Clippy, focused E2EE tests,
workspace tests, diff check, and focused KAMI-27 direct-transport regression passed;
three existing native/owner-operated tests remained ignored by design.

Remaining KAMI-51 work is explicitly outside this acceptance: WP-B owns production
prekey publication/rotation/freshness/replenishment, WP-C owns durable recovery and
rollback authority, and WP-D owns integrated qualification and production security
evidence. KAMI-28 first-message delivery remains downstream of these gates. The
original WP4 runtime/delivery and WP5 qualification/evidence groupings below remain
planning groups and are not replaced by WP-A or converted into Jira issues.

### WP-B — production prekey lifecycle

Internal planning group, not a Jira issue. Scope is identity-owned prekey publication,
rotation, freshness, replenishment, consumption, fallback, restart behavior, and
custody. It depends on the accepted WP-A context/provider boundary, existing identity
and storage contracts, and a runtime-owned provenance seam. It must not enable public
E2EE until trust, freshness, and failure behavior are independently qualified.

### WP-C — durable session recovery and rollback policy

Internal planning group, not a Jira issue. Scope is durable mutable session restoration,
crash-safe CAS, restart reconciliation, deduplication/conflict policy, and an
independent rollback anchor. It depends on WP-B lifecycle semantics and must preserve
fail-closed restore behavior when rollback authority is unavailable.

### WP-D — integrated qualification

Internal planning group, not a Jira issue. Scope is runtime/transport/E2EE integration,
production-boundary qualification, cross-platform and interoperability evidence,
failure/retry/restart testing, and final security review. It depends on WP-B, WP-C,
actual runtime-owned end-peer provenance, and the relevant original WP4/WP5 evidence.

## 5. KAMI-51 full work-package sequence

The work-package names are internal planning groups. They must not be confused with Jira dependency links or treated as a replacement for Jira issue records.

### WP1 — provider/profile foundation

Scope: maintained provider pin, profile/version handling, bounded provider state, fail-closed public API behavior, redacted errors, and deterministic fixtures.

Deliverables: provider adapter, profile validation, state envelope, bounded parsing, public `AadUnsupported` behavior where the provider cannot bind required context, and foundation tests.

Dependencies: KAMI-19, KAMI-20, KAMI-21, selected maintained-provider feasibility evidence.

Gate: no production activation unless the selected provider offers a defensible authenticated profile and platform/interoperability review.

### WP2-A1 — signed identity and Olm binding

Scope: identity-owned signatures binding the selected Olm account/prekey data to the verified device and Peer-ID context.

Deliverables: bounded binding types, strict parsing, custody-owned signing, verification, freshness fields, trust/Peer-ID checks, and negative tests.

Dependencies: KAMI-23 and KAMI-50 contracts.

Gate: detached signatures are not live possession, freshness, revocation, or durable prekey lifecycle proof.

### WP2-A2 — authenticated inner envelope

Scope: canonical private inner envelope, exact context commitment, end-peer/device/proof checks, staged send/receive/replay behavior, and bounded frame accounting.

Deliverables: private envelope and authentication stages, exact bytes, staged provider/account/session effects, prekey-kind checks, redacted failures, and qualification fixtures.

Dependencies: WP1, WP2-A1, KAMI-20, KAMI-21.

Gate: no public production encryption API while authenticated AAD/session integration remains quarantined.

### WP3 — private persistence foundation

Scope: SQLCipher account/session/frame persistence and worker transaction seams beneath WP2-B.

Deliverables: migration 003, private state rows, bounded snapshots, exact CAS/digest checks, transaction atomicity, replay/receipt facts, fault rollback, and restart evidence.

Dependencies: KAMI-22, WP1/WP2-A state contracts.

Status: implemented and owner accepted as a bounded private storage foundation; independently QA-verified by `t_2d834cf1` and independently security-evaluated by `t_82d8776a`. This acceptance does not provide an external rollback anchor, production restore, or runtime activation.

### WP2-B — typed E2EE-to-storage composition

Scope: E2EE-owned opaque authenticated commit consumed by storage through the reverse dependency.

Deliverables: `AuthenticatedCommit`, no-bypass privacy boundary, exact-frame retry, fresh trust/end-peer checks, atomic account/session/prekey/message/frame commit, CAS and fault behavior.

Dependencies: WP2-A2 and the accepted WP3 foundation, plus KAMI-22, KAMI-23, and KAMI-50.

Evidence: implementation run 126 on `t_b8cb3f89`; QA `t_48119cc5`, run 127; Evaluator `t_21f65076`, run 128; 93 package tests/doc-tests; 187 workspace tests; locked all-target check; formatting; strict Clippy; diff check; boundary fixtures.

Status: owner accepted **with conditions** as a quarantined internal foundation only. No network send, ACK authority, runtime activation, public encryption, production rollback recovery, or M1 completion claim.

### WP4 — runtime and delivery integration group

Planning grouping only; not an explicit Jira workflow object.

Expected contents: KAMI-24, KAMI-25, KAMI-26, KAMI-27, KAMI-28, KAMI-29, KAMI-30, KAMI-31, and KAMI-32. This grouping adds sender/receiver lifecycle, supervised runtime, direct transport, relay service/session, path fallback, first direct message, and CLI/IPC.

Gate: all service and transport paths must preserve the KAMI-51 typed boundary, exact logical-message identity, trust/Peer-ID checks, and truthful Delivered semantics.

### WP5 — qualification and evidence group

Planning grouping only; not an explicit Jira workflow object.

Expected contents: KAMI-33, KAMI-34, KAMI-35, KAMI-36, KAMI-37, and KAMI-39. This grouping supplies harnesses, local matrices, real-network/cross-platform evidence, regression, privacy/diagnostic inspection, and independent M1 review.

Gate: passing unit tests or local fixtures alone cannot satisfy M1. Missing OS, network, relay, interoperability, rollback, or evidence coverage blocks the M1 claim.

## 6. Remaining development phases

### Phase A — bounded sender and receiver durable lifecycle implemented

KAMI-24: sender durable enqueue and attempt recovery. It consumes the accepted
KAMI-51 WP1/WP2-A/WP3/WP2-B encryption/storage foundation; it does not require the
full KAMI-51 parent issue, WP4, or WP5 to be complete.

- Commit encrypted outbox before network I/O.
- Preserve one logical ID, sequence, recipient, and exact content binding.
- Recover ambiguous commits and restart state.
- Apply bounded retry, cancellation, queue pressure, and identity-change holds.
- Do not implement Tokio ownership or transport.

KAMI-25: receiver inbox, deduplication, receipts, and replay are implemented as a
bounded runtime-neutral path against shared core, protocol, identity, KAMI-51 and
storage contracts. Sender receipt consumption composes this with KAMI-24. Full
end-to-end runtime/network message-flow integration remains downstream.

- Validate authenticated sender/device, message ID, content binding, sequence, and context.
- Atomically persist inbox/dedup/history/receipt facts before ACK eligibility.
- Re-ACK after a crash between persistence and send.
- Reject forged, stale, mismatched, and conflicting messages/receipts.
- Handle bounded ordering gaps and one durable presentation identity per logical
  message, not exactly-once terminal rendering.

These are source-grounded implemented behaviors, not new test execution or an
owner-acceptance/Jira-closure claim from this documentation task. See section 8
and the [receiver README](../crates/kami-receiver/README.md) for limits and source
coverage. Runtime, transport, relays, CLI/IPC and network/OS qualification remain
Phases B–E; a durable offline mailbox remains outside M1.

### Phase B — supervised runtime and local CLI

KAMI-26 is functionally implemented and independently verified within its bounded
runtime-library/private-mock scope. It supplies one supervised Tokio engine-state
owner, bounded command/event/blocking queues, isolated SQLCipher and key-provider
work, authoritative profile ownership, cancellation, bounded shutdown reporting,
durable restart reconciliation, and a transport-neutral relay reservation-renewal
scheduler. The runtime composes KAMI-24/KAMI-25 only through a private test seam;
its public enqueue path fails closed until a real authenticated end-peer producer
exists.

Evidence: implementation `t_277bb59e`, independent QA `t_e9419e05`, and final
security Evaluator `t_fda1ffe3`, all on `protocol-storage` at HEAD `0de16c9`.
Recorded verification passed formatting, locked workspace checking, warnings-denied
Clippy, 21 focused runtime tests, and 236 workspace tests with two pre-existing
native secure-store tests ignored for unavailable D-Bus/keyring prerequisites.

KAMI-26 is a reusable runtime library, not a complete messaging service. It does
not implement KAMI-27 direct transport, a functioning relay network, KAMI-32
IPC/CLI, production authenticated end-peer evidence, fresh mutable session
restoration, cross-platform qualification, or real-network qualification.

KAMI-32: authenticated Unix-domain-socket/named-pipe IPC, service control, bounded
commands, state rendering, redacted diagnostics, and cross-platform CLI behavior.
Its minimal IPC/service-control seam can be developed after KAMI-26 so KAMI-28 does
not wait for final CLI qualification; its final fallback-integrated completion and
qualification consume KAMI-31.

KAMI-26 must not bypass storage transactions or treat a socket/network write as delivery.

### Phase C — direct transport and live relay

#### KAMI-27 — accepted bounded direct transport

KAMI-27 is owner-accepted within the bounded direct-transport scope. The implementation uses the existing identity custody/enrollment and ProfileLease authority, runtime-owned transport lifecycle, authenticated Peer-ID/device binding, and direct rust-libp2p QUIC-preferred/TCP/Noise/Yamux paths. Independent implementation QA and final security evaluation passed. The owner-operated two-process harness then confirmed manual fingerprint comparison, explicit verification, QUIC and TCP authentication, disconnect/reconnect, and clean shutdown on Linux localhost.

Accepted evidence includes the repository-local `scripts/kami27-manual/` suite and the ignored `transport_tests::native::manual_endpoint` harness. This acceptance does not enable application payloads, E2EE, ACKs, receipts, Delivered, CLI/IPC messaging, relay fallback, or M1 delivery.

Deferred qualification and follow-up scope are explicit: no NAT traversal, DCUtR, hole punching, relay service/reservations, automatic direct-to-relay fallback, cross-network testing, Windows/macOS testing, or production E2EE. Comprehensive rollback resistance and global private-key non-exportability are not claimed. KAMI-29/KAMI-30 own relay work, KAMI-31 owns path fallback, KAMI-35 owns real separate-network qualification, and KAMI-51/KAMI-28 own production E2EE and first-message integration respectively.

### KAMI-29 — separate default-deny live relay service

KAMI-30: client relay reservation/renewal, candidate selection, and authenticated endpoint-to-endpoint inner sessions through a relay.

KAMI-31: bounded direct-first automatic fallback, relay priority/failover, same logical message across paths, duplicate suppression, and no false Delivered state.

### Phase D — first direct encrypted message

KAMI-28 integrates CLI submission, service orchestration, KAMI-51 session composition, KAMI-24 outbox, KAMI-25 inbox/receipt, KAMI-26 runtime, and KAMI-27 direct transport into one two-process direct-message path.

Acceptance requires recipient persistence before ACK, sender receipt validation, stable message identity, restart/lost-ACK safety, duplicate/conflict safety, and changed-identity blocking.

### Phase E — evidence and M1 qualification

KAMI-33 builds the reproducible Rust harness and redacted evidence recorder.

KAMI-34 executes local correctness/resilience/privacy cases.

KAMI-35 qualifies real separate-network direct delivery and genuine automatic relay fallback.

KAMI-36 runs the regression/retry/truthful-failure matrix.

KAMI-37 verifies diagnostics, storage/WAL/temp/crash artifacts, relay forwarding, and ciphertext boundaries.

KAMI-39 independently reviews all evidence and either supports or rejects the bounded M1 claim.

## 7. Dependency DAG

Bounded implementation prerequisites:

```text
KAMI-19 -> KAMI-20 -> KAMI-21
KAMI-19 -> KAMI-22
KAMI-19 + approved identity contract -> KAMI-23
KAMI-23 -> KAMI-50
KAMI-21 + KAMI-22 + KAMI-23 + KAMI-50 -> KAMI-51 foundation
WP1 -> WP2-A1 -> WP2-A2 -> WP3 -> WP2-B -> WP-A
accepted KAMI-51 foundation + KAMI-22 + KAMI-23 + KAMI-50 -> KAMI-24 || KAMI-25
KAMI-24 + KAMI-25 + KAMI-22 + KAMI-23 -> KAMI-26
KAMI-26 + KAMI-23 + KAMI-50 -> KAMI-27
KAMI-27 -> KAMI-29
KAMI-27 + KAMI-29 + KAMI-51 -> KAMI-30
KAMI-21..KAMI-27 + WP-A + WP-B + WP-C + WP-D readiness -> KAMI-28
KAMI-24..KAMI-30 -> KAMI-31
KAMI-26 -> KAMI-32 early IPC/service seam
KAMI-31 -> KAMI-32 final fallback-integrated qualification
KAMI-19 -> KAMI-33
```

Final qualification prerequisites:

```text
KAMI-24..KAMI-27 + KAMI-29..KAMI-32 + KAMI-33 -> KAMI-34
KAMI-27..KAMI-31 + KAMI-33 -> KAMI-35
KAMI-24..KAMI-35 -> KAMI-36
KAMI-22 + KAMI-29 + KAMI-32 + KAMI-34 -> KAMI-37
KAMI-34 + KAMI-35 + KAMI-36 + KAMI-37 -> KAMI-39
```

The live Jira graph contains contradictory links, including reciprocal KAMI-22/KAMI-23 links, reciprocal KAMI-51/KAMI-24 and KAMI-51/KAMI-25 links, and a KAMI-31 -> KAMI-28 link that conflicts with KAMI-28's stated dependencies. This roadmap uses the technically coherent DAG above and does not silently alter Jira. The `||` notation means independent bounded planning/implementation, not that later integration can omit either result.

## 8. Bounded KAMI-25 implementation and downstream integration

KAMI-24 is owner-accepted as a bounded, runtime-neutral sender lifecycle implementation. The accepted scope covers durable SQLCipher sender persistence, authenticated-commit integration, stable request identity and sequencing, exact-frame recovery, leases, bounded retry state, cancellation, and identity holds. Evidence is preserved in implementation task `t_3caec9e9`, pre-implementation review `t_903b8e19`, independent QA `t_d1b015bb`, independent Evaluator `t_4c8b8ad4`, and the synchronized root/storage/sender READMEs.

Accepted bounded scope:

- sender-side durable enqueue and recovery;
- existing `kami-core` facts/reducer and `kami-storage` outbox/worker seams;
- existing KAMI-51 session/commit contracts;
- stable logical ID/sequence/content binding;
- outbox-before-network ordering seam;
- exact-frame retry/recovery, cancellation, queue pressure, ambiguous-commit handling, and identity-change holds.

Explicit exclusions and remaining qualification:

- no new cryptographic protocol;
- no public production encryption activation;
- no Tokio service ownership, timers, or automatic retry scheduler;
- no IPC, direct P2P, relay, or network implementation;
- no live end-peer provenance or production receipt/Delivered orchestration;
  authenticated receipt consumption and durable Delivered projection were added
  separately by bounded KAMI-25;
- no KAMI-26 responsibilities;
- no cross-platform, real-network, interoperability, external rollback-anchor, or production-security qualification;
- finite eight-attempt retry state and retained preparation-tombstone quota/compaction policy remain bounded follow-up work;
- no replacement for WP2-B authenticated storage composition.

The KAMI-24 implementation and documentation were independently verified; that
bounded acceptance does not close the full end-to-end delivery path or imply Jira
workflow closure.

KAMI-25 is now functionally implemented in the current shared `protocol-storage`
worktree. Implementation task `t_52c7a993` records the bounded receiver-to-E2EE-to-
SQLCipher-to-sender path. This synchronization task `t_f5b34761` inspects actual
public APIs and test sources, not a new implementation/QA/Evaluator run. It does
not independently assert KAMI-25 owner acceptance or change Jira status.

Bounded implemented behavior:

- `kami-receiver::ReceiveContext` and `Receiver::{receive,receive_first}` enforce
  expected text/device/conversation admission and current durable Verified trust.
- E2EE's dedicated `AuthenticatedDelivery` stages text reception and
  DeviceDelivered generation in the same bidirectional session. One SQLCipher
  transaction commits account/bootstrap/OTK effects, both session transitions,
  inbox/dedup/history/receipt facts, exact frames and gap observations.
- Migration v5 adds only canonical `delivery_artifact` and `receiver_gap`
  observations referencing existing frame/message authorities; no duplicate inbox
  or mutable session authority. Migration failure preserves old schema/version.
- `ReplayPermit` plus storage recovery returns only exact previously authenticated
  durable ACK/Delivered evidence, including after reopen or history deletion, with
  current trust/original pin and coherent facts/account/session checks.
- `Coordinator::receive_receipt` and `ReceiptOutcome` validate same-session
  receipts against original immutable text facts/frame digest and durably project
  Delivered. Raw socket/relay success cannot substitute for receipt validation.
- Per-sender ordering supports bounded presentation wait, Unresolved/Permanent
  gap observations and late-predecessor reconciliation. Quotas fail closed without
  partially accepting messages or granting ACK eligibility.

Source coverage is in `crates/kami-receiver/tests/receive.rs` (including
`failures.rs`), `crates/kami-e2ee/tests/qualification/receipts.rs`, and
`crates/kami-storage/tests/receiver_migration.rs`: local receipt flow, forged ACKs,
duplicate/conflict and alternate-ciphertext rejection, concurrent trust/commit
behavior, crash/reopen and fault rollback, ordering, corruption, legacy isolation,
and capacity. The crate READMEs give source-checked Cargo target commands; this
documentation-only run does not rerun Rust checks or establish new test counts.

Mandatory remaining boundaries:

- Live end-peer provenance is caller/future-runtime supplied. KAMI-26 must own
  supervised scheduling and serialize current trust/provenance with actual I/O.
- Fresh mutable cryptographic session restoration remains restricted without a
  qualified external rollback anchor; exact persisted-frame recovery is not
  permission to restore a ratchet from DB-only counters/digests.
- Exact-frame retry rejects alternate ciphertext even for identical logical
  content. Bounded receipt/dedup evidence has no active cleanup/compaction and
  exhausts capacity fail-closed; finite sender retry/tombstone policy remains.
- No exactly-once terminal rendering, runtime, direct/relay transport, CLI/IPC,
  offline mailbox, production activation, or production/real-network/cross-platform
  qualification is supplied by this scope. Olm-v1, upstream skipped-key windows,
  prekey lifecycle and interoperability remain separately qualified concerns.

KAMI-26 is now functionally implemented and independently verified as the
bounded supervised-service integration milestone under the coherent DAG. It
consumes the bounded KAMI-24 and KAMI-25 lifecycle seams without activating live
transport. Its private mock integration verifies sender/receiver/E2EE/SQLCipher
composition, while its public API remains fail-closed for live authenticated
end-peer evidence. The roadmap does not authorize KAMI-27 activation or claim
M1 completion.

## 9. Security limitations and non-bypassable gates

The following remain open and must not be bypassed:

1. Public/production encryption remains disabled until the maintained provider/profile and runtime integration are independently accepted.
2. A valid local or detached signature is not live end-peer possession, freshness, revocation, or durable prekey lifecycle proof.
3. KAMI-26 must provide actual authenticated end-peer provenance and serialize trust checks with enqueue, retry, receive, and ACK operations.
4. No production prekey publication, rotation, freshness, replenishment, or service lifecycle is implemented.
5. `ProfileRestore` fails closed because no external non-rollbackable anchor exists. Database-only generations/digests cannot detect coherent old DB/WAL copies.
6. The approved snapshot custodian can technically consume full provider pickles, including private prekeys. Zeroizing containers do not prevent malicious-custodian, native, allocator, dump, or endpoint-compromise exposure.
7. Comprehensive zeroization is not claimed.
8. Storage and E2EE tests are not live network tests, not interoperability vectors, and not cross-platform qualification.
9. Two ignored native secure-store probes require external DBus/keyring prerequisites; Windows/macOS native runtime qualification remains pending.
10. Relay forwarding, socket/network success, reservation success, and ACK transmission never independently establish Delivered.
11. M1 requires real separate-network direct and authorized-live-relay evidence, restart/ACK-loss/duplicate/conflict/order evidence, supported OS evidence, redacted diagnostics, and independent review.
12. No offline server mailbox, silent identity replacement, plaintext fallback, automatic startup, universal NAT guarantee, exactly-once network guarantee, anonymity guarantee, or production SLA is promised.
13. KAMI-25 accepts exact retained frames only, not alternate ciphertext retries.
    Receipt/dedup evidence is bounded and fails closed without active cleanup or
    compaction. Durable presentation eligibility is not exactly-once terminal
    rendering. Bounded ACK/Delivered recovery does not unlock fresh mutable
    cryptographic restoration or production activation.

## 10. Branch and Git integration strategy

Current repository observation: branch `protocol-storage` is at `ebcfc5907a5de17dad2c6643f86730bd52643daf`, matching both `origin/main` and `origin/protocol-storage`. The working tree remains dirty with the uncommitted/untracked KAMI-27 implementation, documentation, and manual-suite files preserved. No Git or GitHub action is performed by this documentation synchronization.

The historical integration plan below remains guidance for owner-managed synchronization, not an instruction to reset or merge automatically:

- Classify KAMI-27 implementation, manual-suite, KAMI-51, governance, and editor-workspace changes before staging.
- Keep KAMI-27 direct-transport closure separate from KAMI-51 production E2EE work.
- Stage only reviewed paths and inspect `git diff --cached` before committing.
- Push `protocol-storage` only after owner review; synchronize `main` through a reviewed fast-forward or merge based on the actual remote relationship.
- Do not bundle unrelated governance or editor-workspace files.

No Git or GitHub action is authorized by this roadmap.

## 11. Status table

Jira statuses below were last read on 2026-11-08. `Functionally implemented` and `tested` refer to repository evidence; `independent` and `owner accepted` are not inferred from Jira workflow status.

| Issue | Implementation state | Verification state | Remaining qualification | Dependencies | Evidence/source |
|---|---|---|---|---|---|
| KAMI-19 | Functionally implemented | Workspace/build evidence; Jira Done | Supported-target reproducibility | Canonical requirements | Jira issue; workspace manifests |
| KAMI-20 | Functionally implemented | Unit/property/golden vectors | Integration consumers | KAMI-19 | `kami-core/README.md`; Jira issue |
| KAMI-21 | Functionally implemented | Vectors, negative corpus, properties, bounds/allocation | Authenticated integration and cross-platform | KAMI-19/20 | `kami-protocol/README.md`; Jira issue |
| KAMI-22 | Functionally implemented | Linux focused/workspace tests passed | Windows/macOS native, broader platform, production qualification | KAMI-19/20; key-provider interface | `kami-storage/README.md`; Jira issue, In Progress |
| KAMI-23 | Functionally implemented | Linux/local identity, custody, trust, binding tests | Native OS, cross-process, live transport, M1 qualification | KAMI-19; identity contract | `kami-identity/README.md`; Jira issue, In Progress |
| KAMI-24 | Owner-accepted bounded implementation | Implementation `t_3caec9e9`; pre-review `t_903b8e19` PASS; QA `t_d1b015bb` PASS; Evaluator `t_4c8b8ad4` PASS; README synchronization `t_0f4d4791` done | Runtime/transport, live end-peer provenance, production receipt/Delivered orchestration, cross-platform, network, rollback-anchor, and production qualification remain deferred; finite retry/tombstone quota policy remains bounded follow-up; bounded receipt consumption added by KAMI-25 | Accepted KAMI-51 foundation; KAMI-20/21/22/23/50 | Implementation/review Kanban evidence; root, storage, and sender READMEs; Jira issue remains unchanged |
| KAMI-25 | Functionally implemented, bounded/quarantined | Public API and local test sources inspected; no new Rust/QA/Evaluator execution in this documentation task | Runtime/live provenance, transport/relay/CLI, external rollback anchor, cleanup/compaction, production, network and cross-platform qualification; no exactly-once terminal rendering | Accepted KAMI-51 foundation; KAMI-20/21/22/23/50; composes KAMI-24 receipt consumption | Implementation `t_52c7a993`; documentation `t_f5b34761`; receiver/E2EE/storage/sender READMEs; Jira unchanged |
| KAMI-26 | Implemented; independently verified bounded runtime/private-mock scope | Implementation `t_277bb59e`; QA `t_e9419e05` PASS; Evaluator `t_fda1ffe3` PASS; fmt/check/Clippy, 21 focused runtime tests, and 236 workspace tests passed (2 native secure-store tests ignored) | Live authenticated producer, direct/relay transport, functioning relay, IPC/CLI, fresh session restoration, external rollback anchor, cross-platform, real-network, interoperability, and M1 qualification remain deferred | KAMI-20..25 | `crates/kami-runtime/README.md`; implementation/QA/Evaluator Kanban evidence; Jira issue unchanged |
| KAMI-27 | Accepted bounded direct transport | Implementation `t_b5624c04`; QA `t_ce4a480c` PASS; Evaluator `t_148a9df5` PASS; owner-operated harness `t_a908edd8` and repository script suite `t_9212dbd6` complete; owner manual QUIC/TCP/reconnect accepted | Linux localhost qualification only; no NAT/DCUtR/hole punching, relay/fallback, cross-network, Windows/macOS, production E2EE, application delivery, CLI/IPC, rollback-resistance, or global private-key non-exportability claim | KAMI-23/26/50; accepted bounded KAMI-51 foundations | `crates/kami-transport/README.md`, `crates/kami-runtime/README.md`, `scripts/kami27-manual/README.md`; Jira KAMI-27 remains authoritative for broader deferred criteria |
| KAMI-28 | Not implemented | Not started | Two-process first direct encrypted message after production E2EE readiness | KAMI-21..27 plus WP-A/WP-B/WP-C/WP-D readiness | Jira issue |
| KAMI-29 | Not implemented | Not started | Default-deny live relay service qualification | KAMI-2/4/19/27 | Jira issue |
| KAMI-30 | Not implemented | Not started | Reservations, inner sessions, renewal, failover | KAMI-23/27/29/51 | Jira issue |
| KAMI-31 | Not implemented | Not started | Automatic direct-first/live-relay fallback | KAMI-24..30 | Jira issue |
| KAMI-32 | Not implemented | Not started | IPC, CLI, diagnostics, shutdown UX | KAMI-2/3/26; final KAMI-31 | Jira issue |
| KAMI-33 | Not implemented | Not started | Reproducible harness and evidence records | KAMI-19; evolves with features | Jira issue |
| KAMI-34 | Not implemented | Not started | Local correctness/resilience/privacy matrix | KAMI-20..26/32/33 | Jira issue |
| KAMI-35 | Not implemented | Not started | Real separate-network direct/fallback qualification | KAMI-27..31/33 | Jira issue |
| KAMI-36 | Not implemented | Not started | Regression/retry/truthful-failure matrix | KAMI-24..35 | Jira issue |
| KAMI-37 | Not implemented | Not started | Diagnostic/privacy/ciphertext-forwarding inspection | KAMI-2/22/29/32/34 | Jira issue |
| KAMI-39 | Not implemented | Not started | Independent final M1 evidence review | KAMI-34..37 | Jira issue |
| KAMI-50 | Functionally implemented | Focused trust/binding tests; Jira Done | Cross-platform/network qualification downstream | KAMI-23 | KAMI-50 commit; `kami-identity`/storage READMEs; Jira issue |
| KAMI-51 WP1 | Implemented/quarantined bounded foundation | Implementation `t_9da66f08`; QA `t_36a44ee9`; Evaluator `t_aab9a8e3` | Production provider/profile qualification | KAMI-21/22/23/50 | `kami-e2ee/README.md`; original WP1 task/reviews |
| KAMI-51 WP2-A1 | Implemented/quarantined bounded foundation | Implementation `t_f348cf2d`; QA `t_f5da1af2`; Evaluator `t_cf4c39d6` | Live possession/freshness/prekey lifecycle | KAMI-23/50 | `kami-identity/README.md`; original WP2-A1 task/reviews |
| KAMI-51 WP2-A2 | Implemented/quarantined bounded foundation | Implementation `t_501a91b6`; QA `t_707f16fb`; Evaluator `t_099f6098` | Runtime provenance/interoperability | WP1/WP2-A1/KAMI-21 | `kami-e2ee/README.md`; original WP2-A2 task/reviews |
| KAMI-51 WP3 | Implemented; owner accepted as bounded private foundation | Implementation `t_fd283614`; QA `t_2d834cf1`; Evaluator `t_82d8776a` | Runtime restore, external rollback, cross-platform | KAMI-22/WP1/WP2-A | `kami-storage/README.md`; original WP3 task/reviews |
| KAMI-51 WP2-B | Implemented; owner accepted with conditions for quarantined scope only | Implementation task `t_b8cb3f89`, run 126; QA `t_48119cc5`, run 127; Evaluator `t_21f65076`, run 128 | No public activation, ACK authority, rollback anchor, or production qualification | WP2-A2 + accepted WP3/KAMI-22/23/50 | Acceptance report; WP2-B task/reviews |
| KAMI-51 WP-A | Implemented; owner accepted with limitations for bounded authenticated-context/provider-contract scope | Implementation `t_b295f09f`, run 187; QA `t_d92abbc2`, run 188 PASS; Evaluator `t_e03aec4f`, run 189 PASS; Kanban comment 176 | Runtime-owned live provenance, prekey lifecycle, durable recovery/rollback, integrated qualification, production E2EE, application delivery, and KAMI-28 remain deferred | WP2-B and prior KAMI-51 foundations; KAMI-27 capability exists but is not consumed by E2EE | `crates/kami-e2ee/README.md`; WP-A task/reviews |
| KAMI-51 overall | Incomplete | Bounded WP1/WP2-A1/WP2-A2/WP3/WP2-B evidence plus WP-A acceptance; full issue not accepted | WP-B/WP-C/WP-D, runtime provenance, production security, interoperability and M1 gates | KAMI-22/23/27/50 and downstream KAMI-24..39 | This roadmap and KAMI-51 Jira record |

## 12. Maintenance procedure

Update this roadmap only when one of these events occurs:

1. A bounded implementation or qualification scope is owner-approved.
2. A source/test/API boundary changes and the relevant README becomes inaccurate.
3. A Jira description, dependency, or workflow status materially changes.
4. A verified test, independent review, owner acceptance, or qualification result changes.
5. A Git integration event changes branch/PR/merge state.

Procedure:

1. Read the canonical requirements and the affected crate README/source/tests.
2. Read Jira only for the affected issue when requirements, links, or status may have changed; record the last verification date.
3. Preserve the distinction between local implementation evidence, Jira evidence, independent review, owner acceptance, and qualification.
4. Update only the root README, affected crate README, and this roadmap as needed. Do not create additional planning Markdown files.
5. Verify every path/link and run `git diff --check`; for documentation-only changes, do not rerun unrelated Cargo tests unless source state changed or the documented evidence is in doubt.
6. Report changed paths, unresolved inconsistencies, and any required owner decision.
7. Never silently edit Jira, Git, GitHub, Obsidian, Kanban, source, manifests, migrations, or Hermes configuration from a roadmap maintenance pass.

## 13. Primary evidence references

Repository evidence:

- `AGENTS.md`
- `docs/Kami_Project_Requirements.md`
- `docs/Kami_Communication_Design.md`
- `docs/architecture/Identity_And_Encryption.md`
- `docs/architecture/Messaging_Protocol.md`
- `README.md`
- `crates/kami-core/README.md`
- `crates/kami-protocol/README.md`
- `crates/kami-key-provider/README.md`
- `crates/kami-identity/README.md`
- `crates/kami-e2ee/README.md`
- `crates/kami-storage/README.md`
- `crates/kami-sender/README.md`
- `crates/kami-receiver/README.md`

Recorded implementation/review evidence:

- KAMI-51 implementation task `t_b8cb3f89`, run 126.
- Independent QA task `t_48119cc5`, PASS.
- Independent Evaluator task `t_21f65076`, PASS.
- Revised architecture review `t_c0dec1dd`.
- Independent security review `t_38ff1f2f`.
- KAMI-24 readiness/Git integration audit, 2026-11-03.

Jira evidence:

- KAMI-19 through KAMI-39, KAMI-50, and KAMI-51 were read through the authenticated manager Jira path on 2026-11-03. Issue records remain authoritative for current status and links; this roadmap records the last verification date and does not mutate them.
