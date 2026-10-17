# Kami Rust Implementation Kickoff

Status: Draft — Pending Approval

Owner decision: Rust is Kami's sole implementation language. This document prepares a controlled implementation phase; it does not authorize application implementation, dependency installation, deployment, release, or a security claim.

## 1. Purpose and relationship to existing planning

This kickoff uses the approved Rust direction and reconciled First-Message Milestone (FM) in `docs/planning/Master_Execution_Roadmap.md`. It does not restart architecture planning, revise requirements, or replace the architecture, protocol, CLI, or QA documents.

The current technical direction is:

- Rust and Tokio.
- rust-libp2p with TCP, Noise, and Yamux for the initial live peer-session candidate.
- Circuit Relay v2 in a separate relay process for live fallback.
- SQLite for durable endpoint-local state.
- clap for the command-mode CLI.
- QUIC or DCUtR only after an approved network scenario justifies a bounded investigation.
- No custom cryptographic algorithms or improvised application encryption.

All crate versions, feature combinations, relay authorization hooks, circuit budgets, key-storage backends, NAT behavior, cross-network performance, and packaging targets remain unverified until the feasibility gates in this document and the existing QA plan produce evidence.

## 2. First-Message Milestone definition

### 2.1 Precise success definition

FM is complete only when two independently initialized, human-operated CLI endpoints, each with one active device profile, exchange one bounded private UTF-8 text message while both endpoints are online.

The required sequence is:

1. Each endpoint has a distinct immutable participant/device identity and protected local key reference.
2. The sender imports the recipient's bounded public descriptor and explicitly verifies the recipient fingerprint through an independently trusted channel.
3. The sender durably commits the outbox record, immutable logical `message_id`, recipient binding, content binding and delivery deadline before network I/O.
4. The client attempts an authenticated direct peer path under the approved bounded path policy.
5. If direct delivery is unavailable or fails before a valid device receipt, the client automatically uses an authenticated live Circuit Relay v2 path through a separately running relay process.
6. The relayed path establishes a fresh endpoint-to-endpoint authenticated session through the circuit; authenticating only to the relay is insufficient.
7. The recipient validates the peer/device binding, protocol version, bounds, recipient binding and content identity; it atomically commits inbox, deduplication, receipt and display-event facts to SQLite.
8. Only after that commit does the recipient emit an authenticated `device_delivered` receipt.
9. The sender validates the receipt against the pinned recipient device and reports `device_delivered`.
10. Direct-to-relay retries retain the same logical message ID and content binding and produce at most one visible recipient message.

A socket write, relay reservation, circuit establishment, relay forwarding event or process exit is not device delivery.

### 2.2 FM scope

In scope, pending approval:

- Two CLI endpoint processes and one independent relay process.
- Explicit two-participant/device enrollment and fingerprint verification.
- One-to-one bounded text messaging.
- Live endpoint-to-endpoint authenticated encryption using the approved reviewed configuration.
- TCP + Noise + Yamux candidate transport.
- Direct attempt plus automatic live relay fallback.
- SQLite outbox, inbox, deduplication and receipt facts.
- Bounded retries, expiry, path evidence and honest non-success states.
- Safe human and JSON diagnostics.
- Reproducible direct, relay-fallback, duplicate, identity-change, restart, ACK-loss and malformed-input evidence.

### 2.3 Explicitly deferred

The following are not FM deliverables and must not be implied by a passing FM result:

- Durable offline mailbox, remote queue acceptance, mailbox TTL or mailbox restart recovery.
- Portable asynchronous application ciphertext or service receipts.
- Groups, group membership epochs, fanout, ordering, group cryptography and multi-device synchronization.
- Artifacts, object storage, chunking, resume and artifact authorization.
- Agent adapters, structured tasks, task execution, cancellation, cross-harness interoperability and A2A conformance.
- Full organization/project administration, public discovery, federation and broad revocation/recovery.
- GUI/mobile clients, anonymity, traffic-analysis resistance and universal NAT traversal.
- Uptime, latency, scale, capacity or cross-network SLAs.
- Production incident response, all-devices-lost recovery and historical key restoration.
- QUIC, DCUtR, AutoNAT and unsupported network profiles unless separately approved as bounded experiments.

A live relay is forwarding infrastructure, not an offline mailbox. FM Noise/session ciphertext must not be reused as a future mailbox envelope.

## 3. Proposed secure defaults

These are conservative proposed defaults for owner review. They are not normative until G0–G3 approval, exact implementation configuration and tests. Where a default cannot be safely selected from current evidence, the default is fail closed.

### 3.1 Identity enrollment and verification

| Area | Proposed default | Evidence or consequence |
|---|---|---|
| Enrollment | Manual import of a bounded public descriptor followed by explicit fingerprint comparison over a separately trusted channel | No public directory or automatic enrollment claim. Import remains `pending` until verification. |
| Trust-on-first-use | Disabled for private FM sends | A convenient first contact cannot silently become authorization. A missing verification ceremony blocks sending. |
| Identity binding | Bind participant/device record to the authenticated libp2p Peer ID on every direct and inner-relayed session | A relay identity, alias, IP or display name cannot substitute for endpoint verification. |
| Unknown key | Reject inbound private text and outbound send | The user must explicitly verify the intended device. |
| Changed key | Preserve the prior pin, mark the peer `changed`, close affected sessions and block new sends until re-verification | Prevents silent key replacement; recovery remains a later contract. |
| Local block | Persist a local `blocked` state and reject new sessions/messages | This is not production-wide revocation and cannot erase content already received. |
| Descriptor | Versioned, size-bounded, signed/provenance-checked public data with bounded addresses and relay identity hints | A self-signature proves key possession only; it does not prove human identity or organization authority. |

### 3.2 Key storage and local history

- Generate transport identity keys with the selected reviewed library and secure RNG.
- Store private keys behind an approved OS-backed or reviewed protected-key provider.
- Do not export private keys through descriptors, CLI arguments, environment variables, ordinary configuration, logs or diagnostics.
- If the provider is locked or unavailable, fail closed. Never regenerate an identity silently and never use a plaintext fallback.
- Treat SQLite, WAL, temporary pages, backups and crash artifacts as plaintext unless an approved at-rest mechanism protects them.
- The initial implementation must support an explicitly named protected lab profile or stop before private real-user data is used. File permissions alone are not encryption.
- Record key-loss and backup behavior as an owner decision; do not imply recovery merely because a public descriptor exists.

### 3.3 Protocol, limits and receipts

Candidate baseline values below are recommendations for a small controlled FM test, not approved SLAs:

| Limit or behavior | Proposed baseline | Required validation |
|---|---:|---|
| Application frame | 64 KiB encoded frame cap | Test prefix validation before allocation and account for Noise/Yamux/ACK overhead. |
| Text body | 16 KiB UTF-8 cap | Test below, equal and above limits; reject malformed UTF-8 safely. |
| Address hints | At most 8 candidates, each length-bounded | Reject unsupported components and never dial arbitrary message content. |
| Direct attempt | 3 seconds per bounded candidate group | Measure only on named network profiles; do not claim universal reachability. |
| Overall foreground delivery | 15 seconds candidate baseline | Must preserve time for relay fallback and remain configurable only within approved bounds. |
| Retries | At most 2 additional same-ID attempts | Count attempts across paths; ACK loss remains uncertain, not proof of non-delivery. |
| Deduplication | Retain through the full approved retry, expiry and clock-skew horizon; candidate lab horizon 24 hours | Cleanup must not permit replay as a new visible message. |
| Message expiry | Sender-owned absolute deadline from local enqueue; candidate lab value 60 seconds | Must not extend silently on path change or relay use. |
| Receipt | `device_delivered` only after recipient inbox/dedup/receipt/display facts commit | Socket, relay and stream events cannot produce this state. |
| Duplicate receipt | Idempotently re-emit the persisted receipt without a second display event | Test restart after recipient commit and before ACK. |
| Late ACK | Preserve `delivery_unconfirmed` or expiry history and expose explicit late confirmation | Do not erase uncertainty or create a second message. |
| Content conflict | Same sender/device/message ID with different content is `id_conflict` and is rejected | Test across direct and relay sessions. |

The 64 KiB/16 KiB/3-second/15-second/2-attempt/24-hour/60-second values remain proposed lab defaults. Owner approval may narrow or change them before coding; no document may present them as production commitments.

### 3.4 SQLite durability

Proposed endpoint-local defaults:

- One SQLite worker/connection per profile with a bounded command queue.
- Exclusive profile ownership while the live engine runs.
- WAL mode, foreign keys enabled, `synchronous=FULL`, parameterized SQL and a finite busy timeout, subject to target-filesystem testing.
- No transaction spans a network await.
- Sender outbox transaction commits before the first network write.
- Recipient transaction atomically commits inbox, dedup key, receipt fact and replayable display event before ACK.
- Ambiguous commit outcomes are reconciled by durable operation/message key before retry.
- Storage-full, corruption, lock, sync and unavailable-key errors produce explicit non-success states.
- WAL, temporary files, backups and cleanup obey the approved local confidentiality policy.

These settings do not prove power-loss durability or at-rest confidentiality. Process-kill, filesystem and target-device evidence are separate gates.

### 3.5 Relay admission and limits

Proposed secure default is deny by default:

- Separate relay identity and operator profile.
- No open public relay and no useful forwarding before admission succeeds.
- Explicit FM allowlist containing the two approved endpoint identities and the approved source-to-destination relationship.
- Reject unauthorized reservations and unauthorized source-to-destination circuits before useful forwarding.
- Pin the relay identity at clients.
- Bound reservations, concurrent circuits, connection count, bytes, rates, duration and renewal.
- Do not treat a broad peer allowlist or post-accept event as proof of per-circuit authorization.
- If the pinned rust-libp2p configuration cannot enforce the required pre-forward policy, stop and return for an owner decision; do not fork or weaken the claim silently.
- Relay stores only required routing metadata and bounded live buffers; it does not store client inboxes or offline messages.

The exact mechanism and limits are unverified until the relay feasibility trial exercises the pinned build.

## 4. Rust-only feasibility plan

The feasibility work is a bounded experiment, not production implementation. It must use disposable profiles, synthetic text, a third relay process and retained redacted evidence.

### F1 — Pinned composition and identity binding

Build the smallest Rust endpoint and relay processes using the selected toolchain and dependency lock. Exercise TCP, Noise, Yamux, Circuit Relay v2 reservation and a bounded application frame.

Required evidence:

- Exact toolchain, Cargo lock, crate features and target recorded.
- Direct and inner-relayed sessions authenticate the intended endpoint Peer ID.
- Unknown, changed, mismatched and relay-impersonation identities fail closed.
- A direct and a relayed session carry the same logical message/content binding while transport ciphertext may differ.
- The relay cannot be mistaken for the recipient.

Stop conditions:

- The pinned APIs cannot compose without an unreviewed workaround.
- Inner endpoint identity cannot be proven on the relayed stream.
- The selected key provider cannot operate without plaintext fallback.

### F2 — Relay admission and circuit budgets

Use the separate relay process with explicit allowed and denied reservations and source-to-destination pairs. Test exact and over-limit frame/circuit cases.

Required evidence:

- Unauthorized reservation denied before useful forwarding.
- Unauthorized source-to-destination circuit denied before useful forwarding.
- Allowed reservation renews and survives the approved operating interval.
- Byte, duration, rate, connection and reservation limits include secure-channel, mux, framing and ACK overhead.
- Quota refusal, relay restart, renewal failure and mid-stream exhaustion produce bounded safe outcomes.
- Relay logs/metrics contain no test plaintext, key material or unredacted sensitive data within the declared inspection boundary.

Stop conditions:

- Only post-accept events or a generic peer allowlist are available where pre-forward authorization is required.
- Circuit limits cannot carry the approved frame and receipt within overhead.
- Authorization behavior requires an unapproved custom fork or open relay.

### F3 — Durable local state and cancellation

Add only disposable SQLite and key-provider seams to the feasibility artifact. Exercise outbox/inbox/receipt ordering, crash boundaries and Tokio cancellation.

Required evidence:

- No sender network write precedes committed outbox state.
- No recipient ACK precedes committed inbox/dedup/receipt state.
- Restart after recipient commit before ACK re-ACKs without a second display.
- Same-ID content conflict, lost ACK, late ACK, storage-full and unavailable-key cases are explicit.
- Delayed store/key work does not starve the continuously polled Swarm or reservation renewal.

### F4 — Genuine separate-network path

Use two independently controlled endpoint networks and a distinct third relay process.

Required evidence:

- A named direct-eligible profile carries both frame and ACK over the actual direct connection.
- A separate controlled fault genuinely prevents direct connectivity while relay reachability remains.
- `auto`, not forced relay mode, performs authenticated live fallback.
- The same logical message ID reaches one recipient inbox/display and produces one valid receipt.
- Topology, NAT/egress condition, direct failure, selected path, timings and redacted state transitions are retained.

Loopback, ping-only, an endpoint-hosted relay, a forced relay-only run or a path label without actual connection evidence is invalid for this gate.

A passing F1–F4 result proves only the tested composition and profiles. It does not approve production deployment, universal NAT traversal, a security audit, a performance SLA or future mailbox/group behavior.

## 5. Pinned-dependency selection procedure

No dependency version is verified by the planning documents. The following is the selection procedure, not a version claim.

1. Define the initial supported target set and minimum Rust edition/MSRV as an owner-approved input.
2. Select a dated stable Rust toolchain compatible with the required Tokio, rust-libp2p, clap and SQLite integration APIs. Record the exact toolchain identifier in `rust-toolchain.toml` only after authorization.
3. Create a temporary compatibility matrix for Rust, Tokio, rust-libp2p, clap, SQLite binding and any approved key provider. Record candidate versions as `PROPOSED`, never `VERIFIED`.
4. Prefer released crates from the approved registry. Avoid unpinned Git branches, wildcard versions and unnecessary default features.
5. Pin workspace dependencies and features centrally. Generate and retain `Cargo.lock` for binaries. Record source provenance, checksums, licenses, advisories, transitive dependencies, native linkage and effective features using `cargo tree -e features`.
6. Verify the exact rust-libp2p feature composition for TCP, Noise, Yamux, relay client/server and only the behaviors required by FM. Do not enable QUIC, DCUtR, DHT, pubsub or other transports by assumption.
7. Verify Tokio runtime, time, sync, signal and I/O features against the selected rust-libp2p APIs. Confirm one runtime and Swarm polling ownership.
8. Verify clap command/help/JSON integration without letting CLI parsing own semantic authorization.
9. Verify the SQLite binding and native build path, journal/synchronous settings, locking behavior, target support and key/history protection. Stock SQLite is not treated as encryption.
10. Run a disposable build and test matrix for each claimed target. Record command, exit code, warnings, binary identity, feature set and limitations.
11. Re-run F1–F4 against the pinned lockfile. Only capabilities demonstrated in those runs may be labeled `verified for the tested profile`.
12. Freeze the accepted manifest and evidence references at the dependency/transport approval gate. Any update requires compatibility, security, relay, storage and regression review.

Status vocabulary:

- `PROPOSED`: planning candidate; no build or experiment evidence.
- `BUILD-VERIFIED`: exact dependency graph builds on a named target; no capability claim follows automatically.
- `TEST-VERIFIED`: named behavior passes a reproducible test on the pinned graph.
- `UNVERIFIED`: no adequate evidence, including undocumented defaults or library examples.

Current status for Rust, Tokio, rust-libp2p, clap and SQLite versions: `PROPOSED/UNVERIFIED`; no dependency installation or feasibility run has occurred for this kickoff.

## 6. Minimal implementation work breakdown

Implementation begins only after owner gates G0–G3 and the feasibility evidence permit the corresponding work.

### W0 — Authorization and contract freeze

Inputs: approved FM scope, trust ceremony, key/history policy, protocol/receipt/state contract, limits, relay admission policy and supported test profiles.

Acceptance evidence:

- Signed or otherwise recorded approval references for G0–G3.
- Versioned protocol vectors and state-transition table.
- No unresolved blocker is silently converted into a code default.

### W1 — Cargo workspace skeleton

Create the authorized Rust workspace with `kami-cli`, `kami-core`, `kami-protocol`, `kami-identity`, `kami-store`, `kami-transport`, `kami-engine` and `kami-relay` boundaries from the CLI plan. Add the approved toolchain, lockfile policy, minimal features and dependency provenance.

Acceptance evidence:

- Workspace metadata and lockfile are reproducible.
- `cargo check`, formatting and lint policy run on the named target.
- No network behavior, private-key handling or production claim is hidden in the skeleton.

### W2 — Profiles and identity

Implement profile initialization, immutable device identity, descriptor export/import, pending/verified/changed/blocked trust states and the approved protected key provider.

Acceptance evidence:

- Re-initialization cannot silently replace identity.
- Explicit verification unlocks only the intended peer.
- Unknown, changed, wrong and relay-impersonated identities fail closed.
- No secret material appears in descriptors, logs, CLI output or diagnostics.

### W3 — Core protocol and state reducer

Implement bounded opaque IDs, message/receipt/error types, version validation, content binding and pure delivery-state transitions.

Acceptance evidence:

- Golden vectors and parser negative tests pass.
- Same-ID/different-content conflicts are rejected.
- Duplicate, lost-ACK, late-ACK and terminal-state behavior is deterministic.
- Limits are enforced before unbounded allocation or network transmission.

### W4 — SQLite durable facts

Implement migrations, profile locking, outbox/inbox/dedup/receipt facts and the transaction boundaries approved at G2.

Acceptance evidence:

- Outbox-before-send and inbox-before-ACK are proven.
- Crash/restart and ambiguous commit recovery preserve facts.
- Storage-full, lock, sync, corruption and unavailable-key states are explicit.
- SQLite work is isolated from Swarm polling and bounded Tokio queues.

### W5 — Secure single-path transport

Integrate the pinned rust-libp2p TCP + Noise + Yamux configuration and endpoint identity binding. Establish one authenticated two-CLI session before adding fallback.

Acceptance evidence:

- Both endpoints verify the intended Peer ID/device binding.
- A single message produces one durable recipient record and one validated receipt.
- Direct transport and relay identity are distinguishable in diagnostics.
- A socket write or relay event cannot mark delivery.

### W6 — Separate relay and automatic fallback

Implement the separately operated relay process, reservation/admission policy, circuit limits and `auto` direct-to-live-relay behavior.

Acceptance evidence:

- Admission is denied before useful forwarding for unauthorized cases.
- Direct delivery works on a named eligible network profile.
- Genuine direct failure triggers live relay fallback without a new logical message ID.
- Cross-path duplicates, ACK loss, relay restart and limit exhaustion remain safe and observable.

### W7 — CLI qualification

Expose the approved clap commands, JSON/human output, status vocabulary, bounded stdin handling, safe diagnostics and shutdown behavior.

Acceptance evidence:

- CLI output distinguishes queued, in-flight, unconfirmed, failed and device-delivered states.
- Exit codes and JSON schema match approved vectors.
- Control characters and instruction-like message text are rendered as untrusted data.
- Profile locking and interruption behavior are reproducible.

### W8 — FM qualification

Run the QA plan's FM matrix and assemble a redacted evidence package for independent review and owner approval.

Acceptance evidence:

- Every FM criterion is pass, fail, excluded, invalid or not testable with evidence.
- Direct and automatic fallback evidence uses two endpoint networks and an independent relay.
- No result claims mailbox, groups, artifacts, tasks, universal reachability or full v0.1.

## 7. Experimental feasibility code versus production implementation

### Feasibility artifact

The feasibility artifact may contain disposable Rust code, test-only configuration and synthetic profiles solely to answer F1–F4. It must:

- Live in an explicitly separate experimental workspace or branch after implementation authorization.
- Use synthetic identities and non-sensitive text only.
- Have finite scope, fixed fixtures, bounded runtime and retained redacted evidence.
- Avoid production schemas, migration promises, public APIs, release packaging and real-user data.
- Never be promoted by copying it into the maintained workspace without review.
- Be deleted or archived with its configuration and failure evidence after the gate decision.

### Production implementation

The maintained implementation begins at W1 only after the relevant approval gate. It requires:

- Reviewed module ownership and public/internal interface boundaries.
- Pinned dependencies and reproducible builds.
- Protected key and local-history policy.
- Explicit protocol and SQLite migration compatibility.
- Error, cancellation, shutdown and recovery semantics.
- QA evidence on named environments.
- Code review, Jira tracking, a reviewed branch and pull request under the repository instructions.

A feasibility pass is evidence for a bounded claim, not production readiness, security certification or authorization to add deferred capabilities.

## 8. Owner decisions that genuinely block implementation

The following decisions should be resolved before the corresponding code is authorized. Recommended defaults are deliberately narrow.

| Decision | Recommended default | Consequence if unresolved or changed |
|---|---|---|
| FM scope | Online two-CLI, one-device-per-participant, one bounded private text message; live relay only | Expanding to mailbox, groups, tasks or artifacts requires a new contract and schedule. |
| Enrollment/trust | Manual descriptor exchange and explicit fingerprint verification; no TOFU | No private send is permitted until verification exists. |
| Changed keys | Preserve old pin, block new sends/sessions and require explicit re-verification | Automatic replacement would weaken identity guarantees and invalidate acceptance tests. |
| Key/history protection | Approved protected key provider; fail closed if unavailable; no plaintext fallback | Without a viable provider, implementation stops before real private data. |
| Live E2EE boundary | Reviewed live endpoint session; no custom cryptography and no mailbox claim | A future mailbox needs a separate asynchronous E2EE contract. |
| Protocol bounds | Approve the candidate small-FM limits or provide replacements before W3/W5 | Without finite bounds, parser, circuit and retry acceptance cannot be measured. |
| Path policy | Direct-first with finite budget and automatic relay fallback | Parallel/racing policy changes duplicate, cost and timing tests. |
| Receipt semantics | ACK only after durable recipient commit; socket/relay success is never delivery | Any weaker rule permits false-positive delivery. |
| SQLite durability | Single worker, WAL, FULL synchronous, explicit crash/recovery tests | Unsupported filesystem or at-rest policy blocks W4. |
| Relay admission | Default deny; explicit FM identity-pair allowlist with pre-forward denial | If pre-forward authorization is unavailable, stop F2/W6 rather than weaken the claim. |
| Supported test profile | Name the OS/toolchain and two endpoint networks plus third relay | Without named environments, cross-network claims are not testable. |
| Dependency pins | Approve exact tested Rust/Tokio/rust-libp2p/clap/SQLite graph after F1 | Versions remain proposed and implementation cannot claim compatibility. |

Rust as the implementation language is already owner-approved. The decisions above are technical and scope approvals; this document does not assume they are approved merely because defaults are recommended.

## 9. Exact first coding task after authorization

After W0 and the dependency/toolchain decision are approved, the first coding task should be:

**Create the minimal Cargo workspace skeleton for Kami's controlled FM, with no network behavior.**

Scope:

- Add the authorized workspace manifest and toolchain declaration.
- Add the crate and binary boundaries defined in W1.
- Add only the approved minimal dependency declarations and lockfile policy.
- Add compile-only placeholder modules, ownership comments and a workspace smoke test.
- Verify metadata, formatting, lint configuration and a clean build on the one approved development target.
- Do not generate identities, open sockets, initialize SQLite, implement protocol frames, start a relay, or claim any network/security capability.

Acceptance result: a reproducible, reviewable empty Rust workspace that establishes module ownership and dependency control without making a runtime or cryptographic claim. The next coding task remains gated on the workspace review and the F1 feasibility decision.

## 10. Approval boundary and source basis

This kickoff remains `Draft — Pending Approval`. It authorizes no code, dependency installation, experiment, deployment, Jira/GitHub mutation, commit or push.

Before coding, the project should approve the FM scope and G0–G3 gates in the existing roadmap, then separately authorize the exact first coding task in Section 9. The existing `First_Communication_MVP.md`, `CLI_Implementation_Plan.md`, `Master_Execution_Roadmap.md`, `Test_And_Validation_Plan.md`, architecture documents and `Rust_Architecture_Decisions.md` remain the controlling planning sources.

The original requirements and communication design remain preserved. Deferred v0.1 capabilities remain deferred and require their own contracts and evidence.
