# Kami Master Execution Roadmap — First-Message Milestone

Status: Draft — Pending Approval

## 1. Purpose and authority

This roadmap reconciles the proposed requirements, communication design, prior gap analyses, and the Product, Architecture, Senior Developer, QA, and Evaluator planning drafts. It is a planning deliverable only. It authorizes no implementation, dependency installation, deployment, release claim, or security certification.

Source labels used below:

- R — `docs/Kami_Project_Requirements.md`, a proposed requirements draft.
- D — `docs/Kami_Communication_Design.md`, a design proposal for review.
- G — `requirements-gap-report.md`, the prior requirements-gap analysis.
- A — `mvp-acceptance-criteria.md`, proposed product acceptance criteria and numeric candidates.
- T — `architecture-options-tradeoffs.md`, conditional architecture analysis.
- P — `docs/planning/First_Communication_MVP.md`, the Product proposal.
- AR — `docs/architecture/*.md`, the five Architecture proposals.
- I — `docs/planning/CLI_Implementation_Plan.md`, the Senior Developer proposal.
- V — `docs/planning/Test_And_Validation_Plan.md`, the QA proposal.
- REV — `docs/architecture/Architecture_Review.md`, the independent Evaluator review.
- N — a new recommendation made by this roadmap for sequencing or reconciliation; it is not an approved requirement.

The roadmap preserves the distinction between requirements, proposals, recommendations, and approvals. A draft appearing in this roadmap does not become normative by repetition.

## 2. Recommended milestone boundary

The recommended first externally reviewable milestone is the First-Message Milestone (FM): two independently initialized, explicitly authorized human-operated CLI endpoints exchange one private UTF-8 text message while online; the client attempts authenticated direct P2P when possible and uses an authenticated live relay when direct delivery is unavailable; the recipient durably records the message before emitting a device-delivery receipt; the sender reports delivery only after validating that receipt; retries and path changes retain one logical message ID and produce at most one visible recipient message.

FM is a deliberately narrow engineering milestone, not the full proposed v0.1 release. It provides an honest foundation for later mailbox, group, artifact, and adapter work without pretending to implement those contracts.

### In scope for FM, pending approval

- Two CLI processes, one active device per participant, with distinct local profiles and durable local outbox/inbox state.
- Explicit enrollment/import and counterpart verification sufficient for the controlled two-participant demonstration.
- One reviewed and maintained live authenticated E2EE/session construction; no custom cryptography.
- Versioned, bounded message framing and stable logical message identifiers.
- Direct P2P attempt plus authenticated live relay fallback, with a finite fallback policy and no security downgrade.
- Durable recipient inbox-before-ACK, sender delivery state, retry, deduplication, and restart behavior.
- Safe, correlated diagnostics that exclude message bodies, keys, and secrets by default.
- Reproducible direct-success, controlled-direct-failure/relay-success, duplicate, identity-change, malformed-input, receipt-loss, and terminal-failure evidence.

### Explicit FM non-goals

Offline mailboxes, durable remote queue acceptance, mailbox retention/TTL, groups and group crypto, artifacts and chunking, organizations/projects beyond minimum authorization data, public discovery, federation, multi-device synchronization, agent adapters, tasks, A2A conformance, GUI/mobile clients, anonymity, universal NAT traversal, exactly-once execution, uptime/latency SLAs, and full v0.1 release certification are excluded.

R classifies many excluded capabilities as proposed initial-scope MUSTs; this roadmap does not remove or amend them. It sequences them after FM behind separate approval packages.

## 3. Reconciled decisions and unresolved conflicts

### Decisions recommended for approval before implementation

1. Treat FM as online-only and mailbox-free. The architect's mailbox/directory/group trace is retained as a later conditional design, not silently combined with FM.
2. Use manually exchanged, explicitly verified peer descriptors for the controlled FM setup. This does not satisfy the full permission-aware discovery or production revocation claims in R; those belong to later work.
3. Use one Rust transport ecosystem for the first implementation: validate Rust/Tokio with rust-libp2p TCP, Noise, Yamux, and Circuit Relay v2. QUIC or DCUtR may be investigated only if an approved network profile demonstrates that the baseline path requires them; neither is an FM default. Do not combine stacks or add a broker “just in case.”
4. Treat live-channel confidentiality and future asynchronous application-envelope encryption as separate contracts. A Noise-protected live stream is not a mailbox ciphertext format. If Product or Architecture requires portable application ciphertext in FM, stop and re-baseline the scope and protocol before coding.
5. Preserve one logical message ID and content binding across paths, but do not claim identical transport ciphertext across separate secure sessions. A same-ID duplicate is suppressed at the recipient.
6. Model delivery as durable facts, not one mutable success flag: local outbox, in-flight observation, device delivered, and named failure/unconfirmed outcomes. `mailbox_accepted` is not an FM state.
7. Use an internal relay-only/single-path slice only as a precursor. It cannot satisfy FM completion until direct success and genuine direct-failure fallback are demonstrated.

### Unresolved decisions requiring explicit approval

- Enrollment authority, trust-on-first-use policy, changed-key behavior, revocation semantics, recovery, and protected key/local-history storage.
- Exact reviewed live E2EE library/configuration and binding between participant/device identity and transport identity.
- Exact wire serialization, canonicalization, frame/text limits, ID encoding, receipt fields/authentication, and protocol version policy.
- Direct-first versus bounded parallel path selection; direct budget, overall deadline, retry count, absolute expiry, clock skew, and dedup retention.
- Relay admission, credentials, quotas, supported OSes/network profiles, and operator ownership.
- Durable store semantics, filesystem/SQLite sync settings, crash boundaries, profile locking, and storage-full behavior.
- Exact Rust toolchain, crate versions/features, API compatibility, target builds, packaging/native linkage, and supported platform claims.
- Relay pre-forward authorization for reservations and source-to-destination circuits; circuit byte/duration/rate/connection limits including protocol overhead.
- Protected key storage and local-history/WAL/backup policy, actual NAT traversal behavior, and cross-network performance measurements.
- FM measurement sample sizes and thresholds. A's 95/100, p95, 24-hour, 10-attempt, and size/quota values remain proposals for later or separately approved use and must not be inherited.

### Rust direction and evidence boundary

Rust is the owner's approved implementation language. The remaining Rust/Tokio, rust-libp2p, TCP + Noise + Yamux, SQLite, clap, and separate-relay choices are proposed technical direction pending approval and validation; this roadmap authorizes no implementation. Exact crate versions and feature/API compatibility, relay authorization before useful forwarding, circuit/resource limits, NAT traversal, protected key storage, cross-network performance, and packaging/native linkage remain unverified until the named tests produce evidence. QUIC and DCUtR are conditional investigations, not implied capabilities. A live Circuit Relay v2 process forwards online traffic only and is not an offline mailbox.

## 4. Milestones, dependencies, and gates

Milestones are sequencing gates, not calendar commitments.

### M0 — Scope and claim freeze

Owner: Product, with Architecture and QA. Inputs: R, D, P, REV. Freeze FM wording, actors, online-only topology, exclusions, supported deployment claim, and the distinction from full v0.1.

Exit gate: written approval of FM scope and non-goals. Stop if stakeholders require mailbox, groups, artifacts, tasks, or broad administration in this milestone without a new contract and acceptance plan.

### M1 — Security, identity, and storage contract

Owner: Architecture/security, with Senior Developer. Dependencies: M0. Define the two-participant enrollment ceremony, fingerprint verification, participant/device/transport binding, key-change response, local key provider, local plaintext/at-rest policy, and threat boundary. Select the reviewed live E2EE/session library and supported platform combination.

Exit gate: feasibility evidence for the selected library and key/storage approach; approved trust-state table; no plaintext-key fallback. Stop if the selected construction is custom/unreviewed, cannot authenticate the intended endpoint through relay, or cannot be operated safely on the supported platform.

### M2 — Protocol and persistence contract

Owner: Architect and Senior Developer, QA review. Dependencies: M0 and M1. Freeze a milestone-specific envelope/frame, IDs, version negotiation, recipient binding, size bounds, receipt schema, delivery state reducer, error vocabulary, expiry/retry ownership, dedup retention, and atomic inbox-before-ACK/outbox-before-send rules.

Exit gate: test vectors and state-transition table accepted by Product, Architecture, and QA. Stop on contradictory envelope/receipt semantics, unspecified late-ACK handling, or limits that cannot be measured and enforced.

### M3 — Controlled single-path secure slice

Owner: Senior Developer, with QA. Dependencies: M1 and M2. Implement isolated profiles, identity import/verification, local store, bounded protocol parser/validator, secure live session, receiver persistence, receipt, CLI status/output, and deterministic fault seams. A simple relay or local path may be used as an internal precursor.

Exit gate: unit, golden, fuzz, reducer, store-crash, and two-process tests demonstrate durable state and one-message deduplication. This is not FM acceptance.

### M4 — Direct P2P and real live-relay fallback

Owner: Senior networking/developer, with Architecture and QA. Dependencies: M3 and transport feasibility. Add direct dialing, relay reservation/admission, authenticated path negotiation, finite fallback, path evidence, and same-ID retry. Use a separate relay process; do not treat forced relay mode, localhost, or an endpoint-hosted fake as fallback proof.

Exit gate: direct-eligible cross-network evidence plus a controlled condition that genuinely blocks direct connectivity while preserving relay reachability; recipient authenticates the endpoint, delivers once, and sender validates the receipt. Stop if required supported network profiles cannot meet the bounded claim.

### M5 — FM qualification and approval

Owner: QA, with Evaluator and Product/Architecture sign-off. Dependencies: M4. Execute the FM-only matrix, including negative paths, ACK loss/restart, duplicate direct/relay arrivals, changed identity, malformed/oversize input, no-route, relay outage, store fault, safe diagnostics, and deployment/network manifest.

Exit gate: every FM criterion is pass, fail, invalid, excluded, or not testable; no proposal is reported as a requirement or SLA. Evaluator confirms the approved claim and evidence. FM completion does not release mailbox, group, artifact, adapter, or full v0.1 work.

### M6+ — separately approved follow-on packages

After FM approval, create separate gates for: asynchronous E2EE and durable mailbox (D C3), groups and artifacts (D C4), and two named adapters/tasks (D C5). Close G's five release-blocking decision packages before a full v0.1 claim: identity lifecycle; delivery/task lifecycle; groups; adapters/A2A posture; operational bounds. Do not scope-creep these packages into M0–M5.

## 5. Work packages and module ownership

These are proposed boundaries from I, refined by N. Paths are new logical surfaces; the repository currently has no implementation manifest. Concrete owners are assigned only after approval.

| Package | Proposed responsibility | Dependencies | Handoff/owner |
|---|---|---|---|
| WP-1 profile and identity | profile isolation, device identity, descriptor import, fingerprint verification, key-provider abstraction | M1 | Identity/security → CLI/engine |
| WP-2 protocol | versioned IDs, bounded frames, validation, content binding, receipt/error types | M2 | Architecture → engine/store/QA |
| WP-3 local persistence | migrations, outbox/inbox/dedup/receipt facts, crash and lock semantics | M1, M2 | Senior Developer → delivery/engine |
| WP-4 delivery reducer | attempt budgets, path-independent IDs, retry, terminal/unconfirmed states, late receipts | M2, WP-3 | Senior Developer → transport/CLI |
| WP-5 transport adapter | direct session, relay session, authenticated peer binding, path metadata | M1, M2 | Networking → engine |
| WP-6 relay service | reservation/admission, bounded forwarding, quotas, no mailbox promise | M1, WP-5 | Operations/networking → QA |
| WP-7 engine | orchestration, authorization checks, send/receive lifecycle, cancellation and restart boundaries | WP-1–WP-5 | Senior Developer → CLI |
| WP-8 CLI and diagnostics | init/import/verify/listen/send/receive/status, JSON/exit codes, safe rendering/logging | WP-1–WP-4, WP-7 | CLI → QA |
| WP-9 test harness | fake clock/network, fault injection, subprocess fixtures, trace/redaction, cross-network manifests | M2, observability contract | QA → Evaluator |
| WP-10 qualification | FM crosswalk, evidence package, independent review, claim recommendation | M4, WP-9 | QA/Evaluator → Product/Architecture |

Dependency direction should remain CLI → engine → narrow identity/protocol/delivery/store/transport interfaces; concrete SQLite/libp2p providers are injected at composition. Do not let CLI strings, transport types, or future task execution semantics leak into the core contracts. Protocol, engine, and store are integration hotspots and require one designated owner per approved change set.

## 6. Normative protocol and test gates

Before M2 approval, the team must publish one FM contract covering:

- immutable logical `message_id`, sender/recipient participant and device IDs, protocol version, creation/expiry facts, and content binding;
- authenticated transport identity bound to the verified device record;
- bounded serialization/framing and rejection of unknown required versions/features;
- sender local durable enqueue before network write;
- recipient validation, deduplication, and atomic inbox/event commit before `device_delivered`;
- same-ID retries and duplicate receipt behavior, including ACK loss after receiver commit;
- no false delivery from socket write, relay forwarding, process exit, or a mailbox-like state;
- typed terminal/unconfirmed statuses, safe diagnostics, and no plaintext/secret logging.

The FM test gate must crosswalk Product AC-01–AC-10 to executable or reproducible evidence. Minimum gates:

1. Component: ID immutability, frame/version/size validation, reducer monotonicity, dedup, fake-clock expiry, error mapping, and malformed/untrusted input.
2. Local integration: two independent profiles, enrollment/verification, outbox-before-send, inbox-before-ACK, restart after commit, profile lock, store failure, and safe CLI output.
3. Direct network: at least one separately recorded cross-network profile where direct authentication succeeds.
4. Fallback network: direct path is actually blocked or made unavailable, both endpoints retain relay reachability, relay is separate, and automatic fallback delivers the same logical ID through endpoint-authenticated E2EE.
5. Race/retry/security: duplicate arrival by direct/relay, lost ACK, wrong peer/key, changed key, invalid receipt, tampered/oversize/version-mismatch frame, no route, relay outage, and disk/store failure.
6. Evidence: build/dependency/config versions, topology and fault proof, safe IDs, selected path, event sequence, state, timings, and redacted inspection. No FM result generalizes to all NATs, platforms, or production security.

## 7. Measurable FM acceptance criteria

The following are N's reconciled FM criteria, pending approval. They intentionally avoid adopting A's full-v0.1 thresholds without a deployment baseline.

| ID | Pass condition | Evidence |
|---|---|---|
| FM-01 | Two distinct local device identities are enrolled/imported and explicitly verified; unknown or changed counterpart credentials follow the approved block/warning rule. | Redacted identity records, fingerprints, and negative test. |
| FM-02 | Sender persists a versioned message record before first network I/O; every retry/path switch retains the same message ID and content binding. | Outbox/store trace and path-correlated events. |
| FM-03 | On a direct-eligible network, Bob durably stores and displays exactly one message, then emits an authenticated device-delivery receipt; Alice reports delivered only after validation. | Cross-network direct trace, inbox/display count, receipt. |
| FM-04 | When direct connectivity is genuinely prevented but relay access remains, `auto` uses the authenticated live relay and delivers the same logical message once. | Fault proof, direct attempt, relay path, endpoint-authentication and receipt trace. |
| FM-05 | Relay and diagnostics expose no tested plaintext, private key, or decryption material within the declared inspection boundary. | Sentinel/storage/log/trace inspection with limits stated; not a security audit. |
| FM-06 | Duplicate/path-race delivery and lost-ACK retry yield one logical inbox/display record and an idempotent receipt. | Injected duplicate and restart evidence. |
| FM-07 | Status distinguishes local outbox, in-flight, unconfirmed/failed, and device delivered; socket or relay acceptance alone never reports delivered. | CLI/JSON state tests. |
| FM-08 | Invalid authorization/key, malformed/oversize/version-mismatch input, relay/no-route, store failure, and receipt timeout produce defined safe non-success states. | Negative matrix and redacted errors. |
| FM-09 | The implementation enforces approved finite frame/text, attempt, deadline, storage, and dedup bounds and fails explicitly at boundaries. | Boundary tests and approved configuration. |
| FM-10 | The complete result is reproducible from a versioned manifest naming builds, libraries, topology, fixtures, faults, observed state transitions, and pass/fail disposition. | QA evidence package and evaluator review. |

No numeric threshold is normative until Product, Architecture, and QA approve it for named environments. FM must report raw samples and conditions rather than inventing SLA language.

## 8. Risks, rollback, and stop conditions

- Scope conflation: if FM is described as v0.1 or includes mailbox/groups/tasks by implication, stop and correct claims before implementation or demo.
- Cryptographic ambiguity: if live Noise/session security and future asynchronous mailbox encryption are conflated, stop; reopen M1/M2.
- Trust gap: if a display name, relay allowlist, or self-signature is treated as human authorization, stop and require explicit verification and device binding.
- Transport infeasibility: if the selected Rust/libp2p combination cannot meet the approved direct or relay test profile, stop M4; investigate QUIC or DCUtR only for a named approved network need, or reduce the claim with approval. Do not force a passing result.
- False durability: if database commit/sync, crash recovery, inbox-before-ACK, or dedup retention is unproven, stop M3/M5 and do not report delivery.
- Resource leakage: if bounds, queue behavior, logs, WAL/backups, or diagnostic retention are unspecified, stop M2/M5 until the boundary is approved.
- Conflicting states: if late receipts, expiration, or path races can regress state or create a second display, stop M2/M4.
- Shared-file hotspot: protocol, engine, and store schemas must have a designated owner; parallel edits require reconciliation before tests are accepted.

Rollback/stop policy: preserve local outbox and evidence, stop new sends, close affected sessions, and mark pending delivery unconfirmed when a contract, key, dependency, relay, or storage invariant fails. Never downgrade authentication, silently regenerate identity, delete unacknowledged content to satisfy quotas, or retry a potentially executed external action. A failed experiment may be discarded only with its configuration and failure evidence retained; no release or deployment rollback is authorized by this roadmap.

## 9. Ownership handoffs and approval checkpoints

- Product owns FM label, user-visible claims, exclusions, and acceptance meaning.
- Architecture/security owns trust, cryptographic/session choice, key storage, identity binding, and protocol contracts.
- Senior Developer owns module boundaries, implementation sequencing, dependency feasibility, and migration-safe local persistence.
- Networking/operations owns relay admission, supported topology, capacity limits, and operator responsibilities.
- QA owns fixture/fault design, evidence integrity, FM crosswalk, and qualification disposition.
- Evaluator independently checks that implementation evidence matches the approved scope and does not certify beyond it.

Approval checkpoints:

- G0 Scope: Product signs FM boundary and non-goals.
- G1 Security/identity: Architecture/security signs trust ceremony, live E2EE, key/storage policy, and endpoint binding.
- G2 Contract: Product/Architecture/QA freeze wire, receipt, state, errors, bounds, retries, and diagnostics.
- G3 Network readiness: Networking/Architecture/QA approve pinned dependencies, relay admission, supported network matrix, fault controls, and measurable FM thresholds.
- G4 FM acceptance: QA produces evidence; Evaluator verifies it; Product and Architecture approve only the bounded FM claim.
- G5 Full v0.1 progression: owners separately close G's identity, lifecycle, group, adapter, and operational packages before claiming R §10 scope.

Each handoff must include the approved contract version, changed module/package scope, evidence, known limitations, and explicit next gate. Implementation work after approval follows AGENTS.md Jira/GitHub branch and review requirements; this roadmap creates no Jira/GitHub objects.

## 10. Recommended minimal path

1. Approve G0 and publish the FM/non-FM boundary.
2. Run the smallest M1 feasibility spike: two profiles, selected live E2EE/session library, identity-to-transport binding, and protected local key/storage behavior.
3. Freeze one FM message/receipt/state contract and test vectors at G2; reject the incompatible mailbox/application-envelope design from FM unless the scope is deliberately expanded.
4. Build the local persistence, verification, secure single-path, and deterministic fault slice.
5. Prove one honest direct delivery on a named cross-network profile.
6. Add a separately running authenticated live relay and prove automatic fallback after a real direct-path failure, retaining the same message ID and one recipient display.
7. Run the FM-only QA matrix and independent evaluation, including security/privacy negative evidence and explicit unsupported claims.
8. Stop at FM approval. Treat mailbox, asynchronous E2EE, groups, artifacts, adapters, and full operational bounds as new, separately approved packages.

This path is the smallest sequence that demonstrates the requested first working two-CLI encrypted message without conflating a relay demo with P2P fallback, a live session with offline E2EE, or transport delivery with task execution.

## 11. Source and proposal reconciliation record

The roadmap accepts from R/D: CLI-first communication, authenticated identities, E2EE, direct P2P with relay fallback, durable local state, honest acknowledgments, and explicit failure semantics. It defers the broader initial scope rather than deleting it.

It accepts from P/I: online two-CLI FM definition, manual verified descriptors for the controlled slice, one transport stack, endpoint-authenticated live session, outbox/inbox ordering, stable IDs, bounded retry, and FM AC-01–AC-10 shape.

It accepts from AR/V: separation of direct/live relay/mailbox roles, per-device delivery facts, security and identity gates, real network evidence, controlled fault injection, safe diagnostics, `not testable` dispositions, and the distinction between FM and full v0.1.

It carries REV findings R1–R6 as resolved-by-sequencing constraints: incompatible mailbox and online-only designs are not merged; trust and revocation claims are narrowed for FM; one milestone contract is required; network feasibility must be demonstrated; durability/resource/security evidence must be bounded; and QA is split into an FM gate and later full-release gates.

It retains G/A/T as prior proposals. Their five gap packages and suggested values (including group, adapter, mailbox, 60-second revocation, 95/100, p95, 24-hour TTL, and size/quota candidates) require owner approval and measured validation; none becomes a commitment through this roadmap.
