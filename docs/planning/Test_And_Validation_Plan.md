# Kami Test and Validation Plan

**Status:** Draft — Pending Approval

## 1. Purpose and approval boundary

This plan defines reproducible functional, security, reliability, cross-network, and interoperability evidence for the proposed Kami v0.1 scope. Rust is the owner-selected implementation language for the current planning direction: the proposed first-message implementation uses Cargo, Tokio, rust-libp2p TCP/Noise/Yamux, a separate Circuit Relay v2 process, SQLite local state, and clap CLI commands. This does not approve a crate version, feature graph, protocol configuration, cryptographic suite, release scope, numeric service level, or untested capability.

The plan is intentionally usable for phased delivery. A capability is not release-ready merely because a related test exists: its normative behavior, selected implementation, supported environment, limits, and acceptance threshold must first be approved in a versioned source. Proposed values in the prior acceptance-criteria analysis are useful candidate inputs, not approved Kami commitments.

Evidence must identify the tested build/configuration and must never include private keys, credentials, plaintext conversation/task data, unrestricted local paths, or sensitive artifacts.

## 2. Test objectives

1. Show that message semantics and stable logical identity remain correct across direct, live-relay, and durable-mailbox paths.
2. Verify E2EE and authorization boundaries without treating transport acceptance as delivery or task execution.
3. Demonstrate recoverable behavior for realistic connectivity failure, restart, retry, duplicate, timeout, revocation, and malformed-input cases.
4. Exercise the selected two-harness support matrix bidirectionally, including safe negative cases.
5. Produce evidence that an intermediary stores/forwards ciphertext only for the inspected configuration.
6. State clearly what cannot be proven by automated or deployment tests.

## 3. Preconditions and test governance

### 3.1 Required approved inputs before release qualification

A release qualification run must be blocked or marked `not testable` until the following are approved and versioned:

- identity/device enrollment, verification, changed-key, recovery, rotation, and revocation contract;
- selected reviewed 1:1 E2EE construction and library configuration, plus selected group encryption/fanout design if groups are in release scope;
- message/receipt/task schemas, valid transitions, terminal states, TTL/deadline ownership, error taxonomy, retry/backoff policy, and duplicate/idempotency retention;
- authoritative membership snapshot, group join/leave/removal epoch behavior, display ordering, recipient-completion rule, and supported group/device limits;
- named adapters, harness versions/platforms, capability schema, common task contract, approval policy, and A2A claim posture;
- supported deployment profile, service topology, quotas, retention, finite capacities, measurement thresholds, and recovery/backup policy.

Until an input is approved, tests may be run as exploratory design evidence only. Their outcome must not be reported as conformance or release acceptance.

### 3.2 Test environments

Use isolated, disposable environments with an explicit configuration manifest.

| Environment | Topology and purpose | Required controls |
|---|---|---|
| E0 deterministic component/simulator | In-memory or local deterministic clock/network simulator for state reducers, retries, duplicate/reordered receipts, TTL, and error paths | Seed recorded; fault schedule versioned; no real secrets |
| E1 single-host integration | CLI/engine, discovery, relay, mailbox, authorization/membership service, and two endpoints on isolated local network | Service logs/metrics enabled; storage/log inspection access; clock synchronization |
| E2 same-LAN multi-host | At least two physical or isolated virtual hosts in one LAN; optional third recipient | Packet capture at test boundary; direct path eligibility confirmed |
| E3 cross-network reachable | Two endpoint networks with separately recorded egress/NAT profiles where direct authenticated P2P is expected to work | Record public-candidate discovery and selected path without retaining unnecessary IP data |
| E4 cross-network restricted | Two independently controlled networks where direct path is blocked or made unavailable but both reach the relay | Firewall/NAT rule capture; relay health confirmed; no direct bypass |
| E5 offline/recovery | Recipient endpoint offline; durable mailbox available; includes service crash/restart and reconnect on changed address | Transaction/durable-ack evidence; fault timing recorded |
| E6 adversarial input/security | Isolated services and adapters receiving generated malformed, unauthorized, replayed, oversized, corrupted, and hostile-content fixtures | No production identities, networks, or harness permissions; safe test harness task only |
| E7 interoperability | Named approved harness A and B with their selected adapters, versions, and operating systems | Capability captures, adapter logs, non-destructive task fixture, authorization audit trail |

A report must identify which environment was actually used. E1 results must not be generalized as E3/E4 network evidence.

### 3.3 Fixtures and controlled faults

The test harness should provide versioned fixtures with non-sensitive synthetic identities:

- participants `human-a`, `human-b`, `agent-a`, `agent-b`; each has distinct immutable test device IDs and public test credentials;
- direct-eligible, relay-only, offline, revoked, unverified/changed-key, and unauthorized devices;
- a one-to-one conversation and a mixed group with an explicit membership version/recipient-device snapshot;
- unique benign plaintext sentinels and binary markers for leak detection, plus known ciphertext/envelope fixtures;
- valid message, receipt, task, capability, artifact-reference, key-update, revocation, and membership-change fixtures;
- malformed JSON/CBOR or selected wire encoding, unknown required version/field, invalid signature/MAC, replay, duplicate ID, mismatched sender/device identity, stale membership/key epoch, corrupt/truncated envelope, oversize payload, quota-exhaustion, expired-TTL, and poisoned artifact fixtures;
- a non-destructive idempotency task whose single side effect is an append-only test record keyed by `task_id`; and
- deterministic faults: packet loss, delay, partition, direct-path loss, relay disconnect, mailbox/database restart, disk/queue-full simulation, duplicate receipt injection, clock advance, and adapter crash after durable task claim.

Each fixture has an ID, generator version or content hash, expected outcome, and sensitivity label. Do not use a real user message, credential, or artifact as a fixture.

## 4. Evidence, observability, and result rules

### 4.1 Required run record

Every matrix execution produces a machine-readable record and a human-readable summary containing:

- test-case ID, purpose, source requirement/design references, and approval status of its contract;
- release/build revision, client/service/adapter/library versions and configuration hashes with secrets redacted;
- environment ID, topology, operating system/runtime, NAT/firewall profile, seed, fixture IDs, start/end timestamps, and clock source;
- safe participant/device pseudonyms; logical `message_id`, `task_id`, `attempt_id`, receipt/event IDs, membership/key version, and artifact ID where applicable;
- selected transport path(s), path-change reason, ordered observed lifecycle events, terminal state, duration measurements, duplicate count, and resource-limit result;
- log/trace/metric/capture references, storage inspection method, assertions, expected versus actual result, pass/fail/not-testable decision, and tester/reviewer identity.

Plaintext sentinels may be retained only in an access-controlled test fixture manifest. Evidence attached to a release report must use hashes or redacted tokens instead.

### 4.2 Minimum observability contract

Before integration testing, components must emit privacy-aware structured events/metrics sufficient to correlate a run without recording private content:

- event/receipt issuer, protocol version, logical IDs, participant/device pseudonyms, event type, lifecycle state, reason class, and timestamp;
- transport selection and transitions (`direct`, `live_relay`, `mailbox`) with attempt count and safe failure class;
- local-outbox durable write, mailbox durable acceptance, recipient local-inbox durable write, and device ACK as separate events;
- authorization/membership/revocation decision version and a redacted allow/deny reason class;
- cryptographic validation outcome and key/membership epoch identifier, never key material or plaintext;
- retry/backoff, expiry, quota, duplicate suppression, dead-letter/poison handling, restart recovery, and per-recipient group state; and
- adapter capability selection, policy/approval decision, task lifecycle, idempotency decision, and artifact authorization outcome.

Metrics and traces must support aggregate delivery rate, delivery latency from local-outbox persistence to device ACK, mailbox backlog/age, retry count, duplicate-suppression count, relay fallback count, failure reason distribution, and test-only ciphertext sentinel scan result. They must not expose message/task bodies or artifact content.

### 4.3 General pass/fail rules

- A test passes only when every listed assertion and required evidence item succeeds.
- Any unexpected plaintext exposure, unauthorized authentication/fetch/decrypt, unauthorized harness invocation, missing durable precondition for an ACK, duplicate visible processing, unreported data loss, or state regression is a failure.
- A test is `not testable`, not passed, when a selected protocol/threshold/implementation behavior is unspecified.
- A test whose fault injector did not establish the declared condition is invalid and rerun; it is not a pass.
- Numeric pass/fail thresholds are taken only from the approved release profile. Before that, report raw samples, percentiles, limits, and conditions without implying an SLA.

## 5. Rust-specific first-message validation strategy

### 5.1 Scope split and executable entry points

The first qualification target is the online First-Message Milestone (FM), not the entire proposed v0.1 release. FM covers two independently initialized, explicitly verified CLI profiles, one bounded private UTF-8 text message, durable sender outbox and recipient inbox, a validated device ACK, direct P2P where eligible, and automatic live Circuit Relay v2 fallback when direct connectivity is genuinely blocked. The relay is a third, separately started process; it is not a mailbox.

Mailbox acceptance/restart/TTL, groups, artifacts, multi-device synchronization, adapters/tasks, and cross-harness tests remain later v0.1 packages. Their existing matrix rows are retained as deferred test designs, not FM pass criteria. No FM report may describe a live relay circuit as durable offline delivery, a socket write as device delivery, or a direct-only/forced-relay test as automatic fallback.

Before a run, freeze and record the exact Rust toolchain, Cargo.lock, crate versions/features, enabled target-specific dependencies, selected feature flags, profile, operating-system target, relay configuration, SQLite journal/synchronous configuration, and CLI configuration hash. If any is absent, the affected case is `not testable` for qualification.

### 5.2 Required test layers and Rust boundaries

The eventual approved workspace must expose stable test seams; this plan does not prescribe unreviewed APIs. The test manifest must map each case to its actual Cargo package, binary, test target, command, seed, timeout, and evidence location.

| Layer | Candidate validation method | Pass evidence / failure boundary |
|---|---|---|
| Rust domain, protocol, and reducer | `cargo test` unit and integration targets; golden frame/receipt vectors; property tests with recorded seeds and shrinking output | Stable logical ID/content binding, state monotonicity, same-ID/different-content conflict, bounds and error classifications. A panic, non-deterministic reducer result, or terminal-state regression fails. |
| Parser and frame boundary | Negative corpus plus coverage-guided fuzz target (for example `cargo fuzz`, if the approved toolchain/target supports it); run malformed, truncated, oversize, unknown-required-field, invalid UTF-8 and hostile diagnostic sequences | No crash, uncontrolled allocation, unsafe terminal output, unbounded work, or accepted malformed frame. Fuzzing is evidence only for the corpus/runtime executed; it does not prove absence of defects. |
| Tokio ownership, cancellation, and liveness | Deterministic-time/concurrency tests where the selected runtime permits them; subprocess watchdog tests under delayed SQLite/key-provider stubs and bounded channels | A cancelled caller reconciles the committed result rather than assuming rollback; the continuously polled Swarm remains able to process connection, receipt, timeout, and shutdown events. Starvation, leaked task/worker, lost committed fact, or shutdown hang fails. |
| SQLite durability and crash recovery | Isolated temporary profile/database fixtures; transaction-fault seams; kill/restart subprocess at named commit boundaries | No network send before outbox commit; no ACK before atomic inbox/dedup/receipt commit; restart re-ACKs matching duplicate without second display. Store-full, lock, journal/recovery and unavailable-key paths return safe explicit failures. SQLite at-rest confidentiality, WAL and backups are separately gated. |
| rust-libp2p transport and relay | Two client processes plus an independent `kami-relay` process; actual connection/path event capture, not a CLI path override | TCP/Noise/Yamux compatibility, endpoint identity-to-Peer-ID binding on direct and inner relayed sessions, Circuit Relay v2 reservation/circuit behavior, and configured limits work on the pinned build. A relay-facing connection alone does not authenticate the recipient. |
| clap/CLI behavior | Black-box process tests of approved commands, stdout/stderr, JSON/text mode, exit status, profile isolation, signal/cancellation, and safe rendering | Observable state distinguishes locally queued, in-flight, unconfirmed/failed, and device delivered. Defaults must not disclose body, key, or raw private endpoint data. A CLI success exit/status on relay forwarding or socket write alone fails. |

`cargo clippy --all-targets --all-features -- -D warnings` is a proposed static gate once the Cargo workspace and feature policy are approved. It complements, but cannot replace, behavioral tests. `cargo test --workspace --all-targets` and each pinned fuzz/property/concurrency command must be recorded exactly rather than inferred from a generic passing command. Feature combinations outside the approved profile are `not testable`, not covered by a default-feature run.

### 5.3 FM fixtures and controlled failures

In addition to Section 3.3, the FM fixture manifest must define:

- two distinct client profile directories (`alice`, `bob`) with synthetic, non-secret device identities and deliberately changed/unknown-key variants; a separately pinned relay identity; and a record of the expected device-to-Peer-ID binding;
- one bounded UTF-8 message with a stable logical ID/content binding, matching duplicate, same-ID/different-content conflict, valid/forged/late ACKs, and a recorded dedup retention horizon;
- a direct-eligible separate-network topology and a restricted topology in which direct traffic is blocked by a recorded rule while both endpoint processes can still reach the third relay process outbound;
- allowed and denied relay reservation fixtures and allowed and denied source-to-destination circuit fixtures. Rejection must occur before useful forwarding; a broad connection allowlist or post-accept event is insufficient evidence;
- exact/over configured frame, encoded-frame, circuit byte, duration, rate, reservation, connection, and queue/channel boundaries. Tests include Noise, Yamux, framing, and ACK overhead rather than treating application payload size as the circuit budget;
- delayed or failing SQLite/key-provider work, sender cancellation after durable outbox commit, recipient crash after inbox commit before ACK, relay drop, ACK loss, duplicate direct/relay arrival, no-route, and relay-unavailable schedules; and
- non-sensitive CLI input containing control characters and instruction-like text, with assertions that it is rendered safely and never treated as command authority.

### 5.4 FM reproducible matrix

| ID | Procedure and required assertions | Minimum evidence |
|---|---|---|
| RFM-01 build and static profile | Build and run the approved Cargo packages/targets; execute the pinned unit/integration suite and approved clippy gate. Deliberately select a missing/incompatible feature or target when feasible and require an explicit configuration failure. | Toolchain, Cargo.lock, `cargo metadata`/feature manifest, exact commands, exit codes, warnings disposition, binary and relay build identifiers. |
| RFM-02 protocol/property/fuzz boundary | Exercise valid vectors and generated bounded IDs, receipts, retries, state transitions, and limits. Run the approved malformed corpus/fuzz duration with seed/corpus recorded. | Vector/corpus revisions, property seed and shrink result, fuzz command/duration, coverage/crash disposition, no-secret logs. |
| RFM-03 Tokio liveness and cancellation | Delay blocking storage/key operations while creating connection/receipt/timeout work; cancel a send after its durable commit and cancel/shut down the CLI under load. | Task/liveness watchdog trace, bounded-channel metrics, commit/result reconciliation, shutdown result, no orphaned task/worker assertion. |
| RFM-04 SQLite ordering and restart | Inject failure before/after sender outbox transaction, recipient inbox/dedup/receipt transaction, and ACK emission; kill and restart client profiles. | Transaction boundary trace, database state inspection under test access, one display/event count, idempotent re-ACK, store/key failure status. |
| RFM-05 identity and secure session | Attempt direct and relayed sessions with correct, unknown, changed, mismatched, and relay-impersonation identities. | Pinned descriptor/Peer-ID assertions, Noise endpoint-authentication outcome on each path, fail-closed status and redacted diagnostics. |
| RFM-06 relay admission and limits | Exercise allowed and denied reservation and source-to-destination circuit attempts; test renewal, restart, exact/over frame/circuit limits, quota/rate/duration refusal. | Relay configuration/build, pre-forward denial proof, reservation/circuit event timeline, byte/overhead accounting, explicit reason classes. |
| RFM-07 direct separate-network delivery | With two endpoint hosts on a recorded direct-eligible profile, send a Kami frame and receive its ACK over the actual direct connection. | Topology/NAT-egress manifest, per-frame/ACK selected connection, durable inbox-before-ACK trace, one display and validated sender receipt. |
| RFM-08 automatic live-relay fallback | Use `auto`; prove a genuine direct-route failure while retaining both clients' outbound reachability to the independent third relay. Do not force relay mode. Assert same ID/content, inner endpoint authentication, one durable display, and receipt. | Firewall/fault record, direct-attempt failure, separate process identities, relay circuit/path trace, duplicate count, final state. |
| RFM-09 retry and failure truthfulness | Cover duplicate/path race, ACK loss, late ACK, no-route, relay outage, malformed/oversize input, changed key, disk/full or locked store, and unavailable key provider. | Input/fault schedule, state timeline, no terminal regression, no duplicate display, typed non-success/late-confirmation outcomes, safe diagnostics scan. |
| RFM-10 CLI observability and privacy | Drive approved clap commands and signals as a user would. Verify exit status and stdout/stderr/JSON for each FM state and error, including control/instruction-like message text. | Captured redacted output, exit codes, safe-rendering assertions, profile-isolation check, plaintext/key/endpoint sentinel scan. |

### 5.5 FM pass/fail/not-testable disposition

- `pass` requires every assertion and evidence item for the relevant RFM row on the approved pinned profile. A same-host simulation can pass component coverage only; it cannot pass RFM-07 or RFM-08.
- `fail` includes a test panic/crash/hang, unbounded task or channel, blocked Swarm liveness, false delivery status, duplicate display, identity-binding failure, forwarding before authorization denial, lost durable fact, limit bypass, unsafe diagnostic disclosure, or a fallback run without demonstrated direct failure.
- `invalid` applies when a fixture/fault did not create its stated condition, the selected actual frame/ACK path is not recorded, or the topology lacks two endpoint networks and a distinct relay process.
- `not testable` applies when contract values, crate/features, authorization policy, key/storage policy, supported target, finite limits, or the required network profile are not approved/available. It must name the missing prerequisite and cannot be counted as a pass.

## 6. Reproducible validation matrix

The following cases are minimum release-gate coverage when their corresponding capability is in scope. `AC` means an approved acceptance contract is required before the result can be a conformance pass.

| ID | Environment | Procedure and assertions | Required evidence |
|---|---|---|---|
| TV-01 initialization and local persistence | E0, E1 | Initialize a fresh client/device. Confirm immutable device ID, public credential record, local encrypted-at-rest configuration where selected, durable outbox/inbox creation, safe default permissions, and idempotent re-initialization behavior. Simulate unavailable local storage. | Initialization transcript; redacted config; file permission/integrity checks; local durable-write and error events; no secret material in logs. |
| TV-02 identity enrollment and counterpart verification | E1, E6 | Enroll a second test device through the approved procedure. Verify state/privilege restrictions, proof-of-possession, audit record, and explicit fingerprint/key-verification UI/API. Substitute a changed credential and verify the approved warning/block behavior before a protected task or artifact-key action. | Credential-state transitions, approver/audit record, test key fingerprints, authorization decisions, blocked/allowed action record. |
| TV-03 one-to-one E2EE messaging | E1, E2, E3 | Send unique test content after authenticated session establishment. Assert sender authentication, ciphertext on every intermediary boundary, successful recipient decryption only on authorized endpoint, durable inbox before device ACK, and stable `message_id`. Repeat with a new session. | Endpoint decrypt assertion; encrypted transport/storage captures; message and ACK correlation; sentinel scan report. |
| TV-04 same-LAN direct delivery | E2 | Confirm both peers are online and direct-eligible. Send text and approved small artifact reference. Assert direct authenticated path selection, no relay/mailbox delivery unless policy allows a documented parallel attempt, one recipient display/process, correct ACK ordering, and local persistence. | Network/path trace, direct session authentication evidence, event timeline, recipient display/processing count. |
| TV-05 different-network direct delivery | E3 | Use separately recorded networks/NAT profiles. Attempt authenticated direct delivery under the selected connection policy. Assert the same logical ID and semantics as TV-04 and record direct establishment outcome/latency. | NAT profile, candidate/path trace, logical-ID correlation, timing samples, result classification. |
| TV-06 direct-path failure and live-relay fallback | E4 | Establish or attempt direct path, then block it before/during send according to a scripted fault. Assert bounded behavior per AC, authenticated E2EE relay session, stable `message_id`, delivery or a defined terminal result, and one recipient display/process despite retransmission. | Firewall/fault schedule, path transition trace, relay authentication event, duplicate count, final lifecycle state. |
| TV-07 offline mailbox acceptance and reconnect | E5 | Take recipient offline, submit ciphertext, then reconnect under a new address. Assert mailbox receipt is reported only as server acceptance, message survives until approved TTL, recipient authenticates/fetches, durable inbox precedes device ACK, and sender state transitions correctly. | Mailbox transaction/receipt, queue record with ciphertext-only inspection, reconnect authentication, ordered state timeline. |
| TV-08 mailbox restart, timeout, expiry, quota, retry | E0, E5 | Acknowledge queued envelopes, restart mailbox/service ungracefully, and fetch after recovery. Independently advance TTL, exhaust quota, inject repeated transport failures, and verify approved retry/backoff/attempt limits. Assert no silent loss and named terminal state for each case. | Durable-write/recovery evidence, backup/recovery logs where relevant, TTL/clock record, quota metrics, retry schedule, terminal reason classes. |
| TV-09 duplicate, replay, reorder, and path-switch suppression | E0, E1, E4, E5 | Deliver identical logical envelopes via duplicate direct/relay/mailbox paths; replay an accepted envelope; reorder receipts including one delayed nonterminal receipt after terminal state. Assert one visible message/application processing, authorized deduplication, monotonic durable state reduction, and receipt correlation without terminal regression. | Injection manifest, message/receipt IDs, inbox/dedup records, UI/API count, before/after state snapshots. |
| TV-10 disconnection and reconnect resilience | E2, E4, E5 | Disconnect sender or recipient during encryption, in-flight transport, relay forwarding, mailbox fetch, and ACK return. Reconnect with changed network address. Assert recovery uses stable identifiers; path selection/retry follows AC; ambiguous delivery is visible rather than guessed; no duplicate display. | Fault timestamps, connection/path logs, retry records, lifecycle timeline, final status. |
| TV-11 lifecycle and task idempotency | E0, E7 | Validate every approved state transition and reject every invalid transition. Submit task to offline recipient; prove mailbox acceptance is not task acceptance. Crash adapter after durable task claim and retry same `task_id`; execute the controlled side-effect fixture. Test cancellation before acceptance, after acceptance, and during run. | Transition-table coverage report, task/attempt IDs, policy/approval audit, side-effect ledger showing at most one execution, cancellation/race outcome. |
| TV-12 named cross-harness interoperability | E7 | Validate both approved capability advertisements. In both directions, submit one advertised non-destructive task and retrieve a permitted artifact reference. Then send absent capability, unknown required version/field, denied policy, and unavailable approval cases. Assert explicit pre-invocation rejection and no local harness work for negatives. | Advertisement schema validation, selected version/capability, correlated request/status/result events, adapter/harness invocation logs, artifact authorization result, support-matrix versions. |
| TV-13 group membership, fanout, ordering, and key epochs | E1, E4, E5, E7 | Create an approved-size mixed group with direct, relay-only, and offline devices. Send concurrent messages and assert recipient snapshot/membership version, stable per-message ID, exactly one processing per authorized device, approved deterministic display order, and recipient-specific state. Remove an offline member; send post-removal content; test missed epoch catch-up or explicit rejoin behavior. | Membership/key-epoch events, recipient ledger, fanout paths, display-order records, removal timing, denied fetch/decrypt proof, partial/full-delivery status. |
| TV-14 device key rotation, revocation, and recovery | E1, E5, E6 | Rotate or replace a test credential using AC. Revoke an offline device after queueing a message but before fetch, then reconnect it. Assert approved cutoff/propagation status, denied authentication/mailbox access, no post-revocation direct/group delivery/decrypt, defined pre-revocation queue disposition, and explicit prior-copy limitation. Test approved recovery path without silent identity reuse. | Authorization/mailbox/membership confirmations, timing evidence, denied access/decrypt results, credential audit history, recovery record. |
| TV-15 malformed and untrusted input handling | E0, E1, E6, E7 | Submit all hostile fixtures: malformed/oversize/corrupt envelope, invalid crypto, unknown required protocol field, spoofed sender/device, replay/stale epoch, unauthorized membership, poisoned artifact metadata, and prompt-injection-style content. Assert reject/quarantine/drop behavior is explicit and safe, parser/service remains available, no content becomes trusted instructions, and no privileged adapter invocation occurs. | Fuzz/generator seed and corpus version, response/error classes, crash/resource record, authorization/adapter audit, redacted logs. |
| TV-16 artifact integrity and access control | E1, E4, E5, E7 | Transfer authorized artifact/reference over selected path. Corrupt/truncate an encrypted chunk or reference and verify integrity failure. Attempt retrieval as removed/unauthorized recipient. If resume/chunking is approved for the milestone, interrupt/resume and test size/chunk boundary behavior; otherwise record it out of scope. | Artifact ID, integrity verification, authorization records, chunk/resume trace, ciphertext-only intermediary scan, boundary result. |
| TV-17 ciphertext-only intermediary storage and diagnostics | E1, E4, E5 | Send uniquely identifiable test payloads/artifact markers through relay and mailbox, including after restart. Inspect persistent blobs, database rows, packet/log/trace/metric output, crash dumps, dead-letter records, and diagnostics under authorized test access. Assert no plaintext marker, private key, or unredacted sensitive payload appears; verify diagnostics still identify safe IDs/path/reason. | Repeatable scan commands/hashes, inspection scope and component versions, redacted samples, no-match results, permitted metadata inventory. |
| TV-18 limits, load, and measurement | E1, E3, E4, E5 | At each approved boundary, run below/equal/above for group/device count, payload, artifact, mailbox, local outbox, TTL, retry, and backlog. Run approved sample count/load on each supported path. Assert explicit pre-send/backpressure/terminal behavior, no unintended partial delivery, durable recovery, delivery success/latency and backlog measurements against approved threshold. | Boundary table, resource metrics, sample data and percentile calculation method, failure/reason events, configuration manifest. |

## 7. Test sequencing and validation gates

1. **G0 FM contract gate:** Product, Architecture, and QA approve the online-only FM scope, identity/Peer-ID binding contract, frame/ACK/state rules, finite retry/dedup/limit policies, safe diagnostic vocabulary, and the exact deferred-capability list. Freeze vectors, fixture format, simulator/fault seams, observability, and the Rust test profile. Until then RFM cases can yield exploratory evidence only.
2. **G1 Rust build and local-correctness gate:** Pin Rust toolchain, Cargo.lock, crate features, target OS, Tokio runtime profile, SQLite configuration, and clap behavior. Pass RFM-01 through RFM-04, including property/fuzz corpus disposition, cancellation/liveness, outbox-before-send, inbox/dedup-before-ACK, crash restart, and safe observable CLI states. A passing build/static check is not network or E2EE evidence.
3. **G2 transport, identity, relay-admission, and limit gate:** Pass RFM-05 and RFM-06 on the pinned rust-libp2p configuration. Verify direct and inner-relayed endpoint identity-to-Peer-ID binding, wrong/changed key fail-closed behavior, pre-forward reservation and source-to-destination denial, and configured frame/circuit resource boundaries including overhead. Stop if supported Rust behavior cannot enforce the approved admission policy before useful forwarding.
4. **G3 separate-network path gate:** Pass RFM-07 and RFM-08 with two recorded endpoint networks and a distinct third relay process. The automatic case requires `auto`, a recorded genuine direct-route failure, preserved relay reachability, actual relay-path frame and ACK evidence, same logical ID/content, and one recipient display. Loopback, an endpoint-hosted relay, ping-only evidence, or forced relay mode is invalid.
5. **G4 FM resilience/privacy gate:** Pass RFM-09 and RFM-10, plus the applicable FM portions of TV-15 and TV-17. Exercise duplicate/path races, ACK loss and late ACK, restart, no-route, relay outage, malformed input, changed key, store/key failure, cancellation, and safe diagnostics. Sentinel scans and tests of reviewed library integration are bounded evidence, not a cryptographic audit.
6. **G5 FM qualification gate:** Execute all RFM cases and the FM-relevant existing rows on the approved deployment profile. The evidence package lists each case as pass, fail, excluded by approved scope, invalid, or not testable. A G5 pass authorizes only the approved FM claim; it does not enable mailbox, group, artifact, multi-device, adapter/task, cross-harness, or full-v0.1 claims.
7. **Later-capability gates:** After separately approved contracts, run TV-07/TV-08 for durable mailbox/recovery, TV-13/TV-14/TV-16 for groups/artifacts/identity lifecycle, and TV-11/TV-12 for adapters/tasks. These cases require additional asynchronous E2EE, authorization, and receipt contracts; FM Noise sessions and Circuit Relay v2 results cannot satisfy them.

## 8. Security assumptions and negative-test focus

These tests assume selected cryptographic libraries/protocols are reviewed and correctly integrated; they do not substitute for a cryptographic design review, implementation audit, penetration test, or formal verification.

Threats that must be represented in negative testing include external observers, curious/compromised relay or mailbox operators, unauthorized/malicious participants, compromised adapters, replay/injection attempts, malicious artifact content, and prompt-injection content. Tests must verify content confidentiality and authorization boundaries, not claim anonymity or traffic-analysis resistance. Services may still observe IP/address candidates, timing, payload sizes, pseudonymous device/queue identifiers, and delivery patterns as documented metadata.

Test fixtures and adapters must never translate received message/artifact text into shell execution or privileged control merely because it resembles an instruction. Execution assertions use explicit recipient-side policy, capability, and approval decisions.

## 9. Explicit limits of this plan

- It cannot prove that E2EE is cryptographically sound, that endpoints are uncompromised, or that metadata analysis is impossible.
- It cannot prove exactly-once execution for arbitrary external side effects; it verifies declared idempotency/ambiguity handling with controlled fixtures.
- It cannot prove reachability across all NATs, CGNATs, enterprise firewalls, ISPs, or future network conditions. Results apply only to recorded profiles.
- It cannot establish an uptime, latency, reliability, capacity, revocation-propagation, or recovery SLA until values and deployment assumptions are approved and measured.
- Inspecting selected relay/mailbox storage, logs, and captures for sentinels demonstrates ciphertext-only handling only for those inspected components, builds, paths, and configuration. It is not a universal no-leakage guarantee.
- It does not validate federation, global public discovery, mobile/GUI clients, voice/video, unlimited groups/files/retention, arbitrary harnesses, or A2A conformance unless those are separately approved and added to the matrix.
- It does not claim that revocation erases previously downloaded plaintext, ciphertext, keys, or independently copied artifacts.

## 10. Completion evidence package

For each completed milestone, publish a redacted evidence package containing the approved contract references, exact scope, matrix results, environment/configuration manifests, fixture and fault-injection versions, trace/log/metric references, ciphertext-only inspection report, known failures, excluded/not-testable cases, and reviewer sign-off. The release decision must distinguish verified behavior from proposals, experimental results, and open risks.
