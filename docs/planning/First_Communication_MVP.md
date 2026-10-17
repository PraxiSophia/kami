# Kami First Communication MVP

Status: Draft — Pending Approval

## 1. Purpose and requirement status

This document proposes a deliberately narrow first working communication milestone: two authenticated Kami CLI instances exchange one private text message across separate network conditions. It is a planning artifact, not an approved requirements amendment, protocol specification, technology choice, or release commitment.

The owner has decided that Rust is Kami's sole implementation language. For this proposed milestone, the technical direction is a Rust CLI/client using Tokio and rust-libp2p, with TCP plus libp2p Noise and Yamux for the candidate live peer-session path, Circuit Relay v2 for live fallback, SQLite for local durable state, and a separately operated relay process. This direction does not approve a crate version, feature set, protocol configuration, or production architecture.

The source requirements classify CLI messaging, authenticated identity, E2EE, direct P2P with relay fallback, acknowledgments, retries, and deduplication as proposed MUST items. They still leave the cryptographic identity and 1:1 construction, enrollment/verification, retention, and quantitative targets unresolved. The communication design proposes a transport-independent message identity and distinguishes direct P2P, live relay, and durable offline mailbox, but is explicitly for design review.

Accordingly, all MVP contracts, limits, and sequencing below are recommendations pending product and architecture approval. Where a decision is still open, an implementation must not substitute a convenient default and represent it as approved.

## 2. Proposed first-success definition

A first successful communication occurs when both CLI instances produce verifiable, correlated evidence for this sequence:

1. Alice's CLI has a locally durable outbound record for logical message `M` with a stable message ID and protocol version.
2. Alice's CLI obtains a currently authorized, authenticated identity/device record for Bob's CLI under the approved enrollment and verification policy.
3. Alice encrypts the private message for Bob using an approved reviewed E2EE construction. No relay, discovery component, or diagnostic output contains the plaintext or decryption key.
4. The clients attempt an authenticated direct peer path according to the approved path-selection policy. If that path is not established or fails before delivery, the same logical message ID uses an authenticated live relay path.
5. Bob verifies origin, authorization, replay/deduplication information, and cryptographic validity; durably persists `M` locally; displays it once; and returns a device-delivery receipt.
6. Alice presents `device delivered` only after receiving and validating Bob's receipt. If the receipt is absent, expired, or rejected, Alice shows the applicable non-delivered state rather than success.

“Successful” in this milestone means device delivery of one text message, not task execution, read acknowledgment, group completion, durable offline acceptance, artifact transfer, internet-wide reachability, or a production availability/security certification.

## 3. Intended user journeys

### J1 — Direct path succeeds

- Alice opens a CLI session, identifies Bob through an authorized direct-contact or invitation flow, and sees a verification/trust state defined by the approved identity policy.
- Alice sends private text to Bob.
- Both endpoints are online and a direct authenticated P2P session is established.
- Bob sees exactly one message. Alice sees a receipt whose meaning is explicitly device delivery, not read.
- A privacy-aware diagnostic record identifies the selected path as `direct_p2p`, message ID, state transitions, and non-secret timing/error class.

### J2 — Direct path is unavailable; live relay succeeds

- The same identity, logical message ID, content binding, and delivery semantics are used. Direct and relayed live sessions may produce different transport ciphertext.
- Direct connection establishment is unavailable or fails within the approved bounded policy.
- The client switches to, or races, an authenticated live relay path without issuing a new logical message.
- Bob sees exactly one message and acknowledges durable receipt. Alice sees `device delivered` with path information that does not expose private content.

### J3 — Delivery cannot complete

- Bob is unauthorized, a key/identity check fails, neither approved path is usable, the relay rejects the attempt, or a receipt cannot be obtained.
- Alice retains the local outbox item until the approved terminal policy applies and sees an explicit, actionable status such as authorization rejection, identity verification required, relay unavailable, delivery timeout, or delivery failed.
- The CLI does not report delivered merely because a socket write, connection attempt, or relay forwarding attempt occurred.

## 4. Proposed MVP scope

### In scope, pending approval

- Two CLI instances operated by two explicitly enrolled and authorized human participants.
- One private one-to-one UTF-8 text message per send operation.
- A versioned logical envelope with a stable `message_id`, sender participant/device identity, recipient device identity, protocol version, and payload carried only inside the approved authenticated endpoint session for this online milestone. Routing fields must contain no secret, private prompt, decryption key, or unrestricted local path.
- A narrowly specified authenticated identity/device enrollment and counterpart-verification flow sufficient for the two test participants.
- One approved, maintained E2EE library/protocol configuration for this exact one-to-one use case; application code must not invent cryptography.
- A proposed Rust runtime boundary in which Tokio owns asynchronous client work and rust-libp2p owns the configured direct or relayed peer sessions. Candidate live sessions use TCP, libp2p Noise, and Yamux; exact crate versions, enabled features, interoperability, and security configuration remain unverified.
- Local durable outbox before transmit and local durable inbox before emitting a device-delivery receipt.
- Direct P2P attempt when network conditions allow and an authenticated live relay fallback when it does not, with path-independent message identity and deduplication.
- A separately operated Circuit Relay v2 process for the live fallback path. It forwards live encrypted traffic only and is not a mailbox.
- SQLite-backed local state for the sender outbox, recipient inbox, deduplication facts, and receipt facts, subject to an approved local key/history-at-rest policy. SQLite alone does not encrypt local history, WAL files, backups, or private keys.
- Per-recipient-device delivery state for this single-device-per-participant scenario: at minimum `local_outbox`, `in_flight`, `device_delivered`, and explicitly named terminal failure states.
- Bounded retries and receipt validation only after the responsible owner approves retry, TTL, and failure policy.
- Privacy-aware CLI diagnostics that expose selected transport path, message ID, state, and non-secret failure category without message content, keys, or sensitive endpoint data by default.
- Reproducible test evidence for direct success, relay fallback, duplicate suppression, invalid/changed identity behavior, and failure reporting.

### Explicitly out of scope for this milestone

- Offline delivery, durable remote mailbox, queue acceptance, mailbox restart recovery, and mailbox retention/TTL claims.
- Group conversations, group fanout, group ordering, group encryption, membership epochs, and multi-device synchronization.
- Files, artifacts, chunking/resume, artifacts keys, or artifact access rules.
- Agent adapters, structured tasks, task acceptance/execution/cancellation, cross-harness interoperability, and A2A conformance claims.
- Organization/project administration beyond the minimum authorization data necessary for the two approved participants.
- Federation, public directory/discovery, mobile/graphical clients, voice/video, global history/search, anonymity, and traffic-analysis resistance.
- Exactly-once delivery or execution; universal NAT traversal; a guarantee that a direct path exists; uptime, latency, or scalability SLAs.
- Device recovery, historical key restoration, broad key rotation, and production incident response beyond the approval gate needed to avoid unsafe identity claims.
- Relay authorization policy, reservation/circuit/byte/duration limits, and any claim that a Rust relay integration enforces source-and-destination authorization before useful forwarding.
- NAT traversal mechanisms or success claims beyond the observed direct path and controlled direct-blocked fallback cases. DCUtR, AutoNAT, QUIC, and unsupported network profiles are not implied by the TCP/Noise/Yamux direction.

A future full initial release may include these excluded capabilities only under separately approved requirements. This MVP must not be labeled as satisfying the entire proposed initial scope in the canonical requirements.

## 5. Required behavior and measurable acceptance criteria

The following are proposed pass/fail criteria. Exact durations, retry ceilings, and test-environment capacity are deliberately not set here because no approved deployment baseline exists.

| ID | Proposed criterion | Required evidence |
|---|---|---|
| AC-01 | Each CLI uses a distinct, immutable participant/device identity accepted by the approved enrollment policy; unverified or changed counterpart credentials trigger the approved warning/block behavior before private send. | Redacted identity records and test output for enrollment plus changed/unknown credential behavior. |
| AC-02 | Before its first network attempt, the sender durably records a versioned envelope with stable `message_id`; all retries and transport-path changes retain that ID. | Sender outbox record and direct/relay trace correlated by the same ID. |
| AC-03 | A successful direct test results in one Bob-side durable inbox record and one visible message, followed by a validated device-delivery receipt. | Direct-path test trace, recipient storage assertion, display assertion, and receipt correlation. |
| AC-04 | Under a controlled condition that prevents direct connectivity while both endpoints remain online, the same send is delivered through an authenticated live relay with the original `message_id`. | Test configuration, path diagnostic, recipient receipt, and direct-attempt/fallback evidence. |
| AC-05 | Relay fallback preserves E2EE: inspection of relay storage/logging and wire-facing diagnostic data for a unique test sentinel finds no plaintext payload or decryption material. | Redacted inspection result, configuration/version record, and retained sentinel-test assertion. |
| AC-06 | A duplicate submission of the same authenticated message ID, including delivery by more than one path, produces at most one visible message and one logical inbox record. | Controlled duplicate/path-race test and recipient state assertion. |
| AC-07 | Sender status distinguishes local outbox persistence, in-flight attempt, and device delivery. A connection, relay acceptance, or socket write alone cannot produce a delivered display. | CLI-state test including a withheld/invalid receipt. |
| AC-08 | Invalid authorization, invalid/changed identity, decryption/authentication failure, relay unavailability, direct-and-relay exhaustion, and receipt timeout each produce a defined non-success status without leaking plaintext or secret key material. | Negative-path matrix with status/error class and redacted logs. |
| AC-09 | Logs/diagnostics are sufficient to correlate sender and recipient outcomes using safe identifiers, protocol version, selected path, and state transitions, while default output excludes message bodies, private keys, shared secrets, and raw endpoint addresses unless an approved secure diagnostic mode authorizes them. | Diagnostic-output review and automated/redacted assertions. |
| AC-10 | The test record is reproducible: it identifies Rust toolchain, client and separate-relay build versions, exact rust-libp2p/SQLite dependency versions and enabled features, approved protocol/library configuration, environment/network setup, test inputs or safe equivalents, observed state transitions, and pass/fail outcome. | Versioned test matrix retained with the milestone evidence. |

Acceptance of AC-05 demonstrates only that the inspected relay configuration did not expose the test sentinel. It is not a substitute for a cryptographic review or a complete security audit.

## 6. Delivery, identity, and E2EE boundary

The milestone needs a small normative contract before implementation begins:

- A direct P2P path and a live relay path are transport alternatives, not distinct user messages. The candidate Rust transport uses rust-libp2p TCP/Noise/Yamux sessions for both paths; path selection or failure must not alter message identity, sender/recipient authorization, content confidentiality, or receipt meaning.
- The separately operated Circuit Relay v2 process may forward encrypted live traffic and observe the minimum routing metadata needed to operate. It is not an offline mailbox and must not be described as one.
- `device delivered` is emitted only after Bob has authenticated, decrypted/validated, deduplicated, and durably persisted the message. It is not an assertion that Bob read it.
- A direct connection attempt must have an approved finite stop/fallback condition. Waiting indefinitely for P2P is not an acceptable response to a send.
- The sender needs a durable record sufficient to retry safely. The recipient needs a durable deduplication record scoped to the authenticated sender/device and `message_id` under the approved protocol.
- Endpoint cryptography, identity binding, trust-state transitions, nonce/replay protections, and key handling must come from selected reviewed libraries and documented integrations. Libp2p Noise provides a candidate authenticated live-session boundary; it does not settle Kami identity enrollment, key storage, application-envelope encryption, or future mailbox encryption. The milestone cannot proceed on a custom or unspecified encryption scheme.

## 7. Assumptions to validate, not hidden requirements

- A single self-hosted logical deployment can provide the minimum discovery/signaling and live-relay service for the test; its exact hosting model remains unapproved.
- Both participants are online during sending and each initially has one active device.
- The test environment can deliberately permit and deny direct connectivity without relying on uncontrolled public-NAT conditions.
- The selected Rust/Tokio/rust-libp2p ecosystem has a maintained, compatible, pinned configuration that supports mutually authenticated TCP/Noise/Yamux endpoint sessions and Circuit Relay v2 integration. This remains unverified until a bounded feasibility and build review names exact versions and enabled features.
- The separate relay process has an approved authorization/admission mechanism and compatible reservation, circuit, byte, duration, rate, and connection limits. This remains unverified; library defaults and connection allowlists must not be assumed to implement Kami project authorization.
- Relay availability during a test does not establish production availability, persistence, backup, capacity, NAT traversal, global network reachability, or cross-network performance. Cross-network performance remains unverified until measured against an approved topology and bounds.
- Minimal participant authorization can be represented without implementing all organization/project management functions.
- Test diagnostics can be collected under a protected local test environment and redacted before retention or sharing.

If an assumption fails, the team should revise scope or return to an approved design decision rather than silently expanding the MVP.

## 8. Open decisions requiring owner approval

| Decision | Why it blocks or changes the MVP |
|---|---|
| Identity authority and enrollment ceremony | Determines who can enroll the two endpoints, how proof of possession is verified, and how changed keys are handled. |
| Trust policy | Must decide whether trust-on-first-use is allowed and which operations are blocked until explicit counterpart verification. |
| 1:1 E2EE protocol/library and key lifecycle | Required to substantiate confidentiality, authenticated sender claims, replay defense, rotation/revocation behavior, and supported platforms. |
| Rust dependency and transport profile | Rust is the owner-selected implementation language, but exact Tokio/rust-libp2p/SQLite crate versions, features, TCP/Noise/Yamux configuration, platform support, and inner endpoint-authentication behavior must be pinned and validated before implementation. |
| Direct-path policy | Must define direct-first or bounded parallel/racing behavior, fallback trigger, timeout ownership, and what happens to an in-flight message. |
| Minimal authorization model | Must identify how a sender proves it may address Bob and how Bob's client validates that authorization without creating a public directory. |
| Receipt schema and persistence semantics | Must define fields, signature/authentication, ordering/idempotency, and exactly what state Bob has durably committed before acknowledging. |
| Retry, expiration, and failure policy | Needed to prevent unbounded retry, false success, and unclear terminal state; offline mailbox semantics are explicitly excluded here. |
| Diagnostic data policy | Determines safe default logging, access control for sensitive troubleshooting, and retention/redaction obligations. |
| Relay admission and resource policy | Must define how the separate Circuit Relay v2 process authorizes reservations and source-to-destination circuits before useful forwarding, plus finite reservation, circuit, byte, duration, rate, and connection limits. |
| Supported environments and test topology | Bounds the initial claim; actual cross-NAT/network behavior, NAT traversal, and performance cannot be inferred from a localhost-only test. |

## 9. Rust technical direction and remaining tradeoffs

### Proposed runtime, transport, relay, and state boundaries

- The candidate client is a Rust CLI with a Tokio-owned asynchronous engine. A transport owner continuously services rust-libp2p events and must not be blocked by SQLite, key-provider prompts, terminal input, or long-running work.
- The candidate peer-session composition is rust-libp2p TCP with libp2p Noise and Yamux. Direct and Circuit Relay v2 paths must authenticate the intended endpoint under the approved device-to-transport-key binding; transport authentication to the relay alone is insufficient.
- The relay is a separate Rust process with its own infrastructure identity, admission configuration, reservations, quotas, and health/operational responsibilities. It has no access to client outboxes, inboxes, plaintext, or durable mailbox data.
- SQLite is the proposed local durable-fact store for the client outbox, inbox, deduplication, and receipt state. Synchronous storage and protected-key operations need bounded worker/task ownership so Tokio transport progress remains live. Local key storage, local plaintext history, WAL, backups, and crash artifacts require a separate approved protection policy.
- Direct P2P with live relay fallback remains the milestone path. A relay-only slice can be an internal precursor but cannot satisfy this MVP; direct-only communication cannot satisfy its fallback acceptance criterion.

### Connection/path policy

- Direct-first with a bounded fallback is conceptually simple but may delay delivery on networks where direct connectivity will not work.
- A bounded parallel or happy-eyeballs-style direct/relay race can reduce user-visible delay but requires careful duplicate suppression and cost control.
- The selected policy must be measured under a named test topology before any performance claim.

### Identity enrollment

- Existing-device approval reduces centralized impersonation risk but leaves all-devices-lost recovery unresolved.
- Administrator-assisted recovery can make managed operation practical but creates a high-value recovery authority and must not imply operator plaintext access.
- A narrowly controlled two-participant setup may be acceptable for an experiment, but its trust ceremony must still be explicit and reproducible.

### E2EE integration

- Libp2p Noise is a candidate reviewed live-session mechanism for this online milestone, but a pinned configuration, endpoint/device binding, replay behavior, and target-platform support require integration review.
- Separate session ciphertext may differ between direct and relayed connections. The invariant is the same logical message ID and content binding, not identical transport ciphertext.
- A reviewed, maintained 1:1 protocol/library reduces cryptographic-design risk but requires an integration review.
- A custom construction may appear to accelerate a demonstration but is incompatible with the source requirement to use reviewed cryptographic protocols/libraries and is rejected for this milestone.

## 10. Approval checkpoints and stop conditions

### Checkpoint A — Scope and claims

Product owner approves the bounded first-message claim, confirms excluded capabilities, and confirms that it is not a full v0.1 release claim.

Stop if: stakeholders require offline delivery, groups, artifacts, tasks, or broad organization administration in this same milestone. Those needs require additional approved contracts and separate acceptance evidence.

### Checkpoint B — Security and identity contract

Product and architecture owners approve the identity/enrollment, counterpart-verification, key-change, credential-revocation, protected key-storage, and reviewed Rust live-session-library decisions for this limited path.

Stop if: no reviewed Rust library configuration and viable identity binding support the selected clients, if the selected configuration requires an unspecified custom cryptographic scheme, or if protected key storage cannot be provided without plaintext fallback.

### Checkpoint C — Transport and operations contract

Architecture and operations owners approve the pinned Tokio/rust-libp2p/SQLite dependency profile, TCP/Noise/Yamux session configuration, separate Circuit Relay v2 process, relay admission and finite resource limits, path-selection policy, minimal metadata/diagnostic handling, and explicit distinction between relay and mailbox.

Stop if: relay fallback cannot remain endpoint-to-endpoint authenticated/encrypted, the selected Rust stack cannot demonstrate the controlled direct-unavailable case, required relay authorization cannot be enforced before useful forwarding, or the configured circuit limits cannot carry approved frames and receipts.

### Checkpoint D — Test and evidence plan

QA/product approve the AC-01 through AC-10 test matrix, safe diagnostic capture, and success/failure reporting vocabulary before implementation is considered complete.

Stop if: evidence cannot distinguish device delivery from attempt/connection success, cannot test duplicate suppression, or exposes plaintext/secrets in routine artifacts.

### Checkpoint E — Milestone review

After implementation, reviewers verify the approved contract and reproducible evidence. Passing the tests authorizes only the approved milestone claim; it does not approve next-phase mailboxes, groups, artifacts, or tasks.

## 11. Recommended minimal path, pending approval

1. Approve the narrow scope and mandatory decision gates above.
2. Specify a single versioned two-party envelope and local state/receipt reducer before implementing transport.
3. Pin and review the Rust/Tokio/rust-libp2p/SQLite dependency profile, Rust toolchain, enabled features, target environments, and TCP/Noise/Yamux session configuration; reject unreviewed defaults and unsupported required targets.
4. Implement controlled identity enrollment and counterpart verification for two test participants using the approved trust model and protected key-storage policy.
5. Integrate the selected reviewed live-session configuration, run the relay as a separate process, and prove ciphertext-only relay handling with a sentinel test.
6. Implement SQLite-backed local outbox/inbox persistence and authenticated per-device receipt/deduplication without blocking transport progress before exposing send success in the CLI.
7. Demonstrate one direct delivery, then deliberately prevent direct connectivity and demonstrate the same-message-ID Circuit Relay v2 fallback.
8. Verify relay authorization and configured circuit/resource-limit behavior, then execute and retain the approved negative-path and diagnostic tests.

This sequence minimizes the risk of mistaking a relay socket demonstration for secure, acknowledged communication. It intentionally defers durable offline queues, group state, artifacts, and task semantics until their unresolved contracts are approved.

## 12. Source basis

- `docs/Kami_Project_Requirements.md`: §§3, 7 (UC-01 and UC-06), 8.1, 8.3, 9, 10–12.
- `docs/Kami_Communication_Design.md`: §§2–5, 7–9, 11, 15, 17–19.
- `docs/planning/Language_Implementation_Assessment.md`, `docs/architecture/Language_And_Network_Stack_Decision.md`, and `docs/architecture/Language_Stack_Independent_Review.md`: Rust/Tokio/rust-libp2p candidate evidence and remaining implementation, admission, storage, and network-validation gaps.
- Prior analysis: `requirements-gap-report.md`, `mvp-acceptance-criteria.md`, and `architecture-options-tradeoffs.md` in the requirements-gap-report planning output. Their recommendations remain proposals pending the approval checkpoints in this document.
