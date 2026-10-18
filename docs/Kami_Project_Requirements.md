# Kami — Product Requirements

**Status:** Owner-approved product direction and Milestone 1 requirements  
**Implementation language:** Rust  
**Scope:** Milestone 1 (M1) and explicitly separated future capabilities

## 1. Purpose

Kami is a self-hostable, communication-first network for **human ↔ human**, **human ↔ AI agent**, and **AI agent ↔ AI agent** communication across computers and networks. It supplies authenticated identities, private message delivery, and interoperable communication contracts; existing AI harnesses join through adapters rather than being replaced. The name evokes *ishin-denshin* (heart-to-heart transmission), without implying that Kami is an AI runtime, model router, memory system, shell execution service, or task planner.

Kami must preserve the meaning, identity, authorization, and security of a message regardless of whether its transport path is direct P2P or an authorized live relay. A live relay is a forwarding mechanism; it is **not** a durable offline mailbox.

## 2. Product boundaries and users

- **M1 users:** Two human-operated, explicitly verified endpoints exchanging one-to-one private text through the Kami CLI across Linux, Windows, and macOS.
- **Deployment operators:** May host independent relay servers; operating a relay does not confer access to E2EE plaintext.
- **Future users:** Human teams, organization/project administrators, and identifiable AI agents integrated through adapters with least-privilege permissions.
- **Participant and device:** Distinct concepts. M1 supports **one active device per participant identity**. Device/transport identities remain explicit to permit later multi-device designs.

## 3. M1 functional requirements

### Identity, invitations, and trust

- **ID-01** Each M1 installation creates a cryptographic device identity with keys protected by a supported operating-system secure credential facility.
- **ID-02** Users exchange contact invitations containing authenticated public identity and usable connectivity/relay hints. The invitation alone does not prove that a recipient is genuine.
- **ID-03** Both parties explicitly verify each other's identity fingerprints through a trusted independent channel before private messaging. No silent trust on first use.
- **ID-04** The verified device identity must be cryptographically bound to its authenticated rust-libp2p Peer ID. Direct and relayed connections must authenticate the expected identity.
- **ID-05** An unexpected identity-key replacement blocks sending, including queued messages, until the new identity is independently verified. If keys are irrecoverably lost, the installation generates a **new identity**, requiring contact reverification. Automated backup/recovery is not in M1.

### Private messaging and local data

- **MSG-01** Two verified online endpoints can exchange UTF-8 text. The maximum **encoded network message** is **65,536 bytes** including envelope overhead; larger artifacts are not silently embedded.
- **MSG-02** Every logical message has a stable unique ID across retries and changes of transport path. Use a versioned Protocol Buffers wire contract.
- **MSG-03** End-to-end message confidentiality and authenticity hold over both direct P2P and live relay paths. Relay operators must not be able to decrypt message contents merely by forwarding them.
- **MSG-04** Use an established, authenticated Signal-style session-establishment design and Double Ratchet message-key progression, with a reviewed, viable Rust implementation; no custom cryptographic primitives.
- **MSG-05** Sender durably queues messages in a local encrypted SQLite outbox before transmission; recipient stores accepted messages in a durable encrypted SQLite inbox/history.
- **MSG-06** Recipient sends a **device-delivered** acknowledgment only **after** durable local persistence and validation. Network send success or relay acceptance never counts as recipient delivery.
- **MSG-07** Retry delivery automatically with bounded backoff following connectivity loss, service restart, or missing receipts. Retain undelivered messages until delivered or explicitly canceled; enforce safe resource limits and provide visible queue-pressure/failure state.
- **MSG-08** Suppress duplicate receipt/display using authenticated sender identity plus message ID and content binding. Reject reuse of the same ID with conflicting content; do not promise exactly-once network transmission.
- **MSG-09** Preserve per-sender order in each conversation. Briefly hold later-arriving messages for a missing earlier message; after a bounded wait, show later messages with a visible gap, without losing or re-identifying messages.
- **MSG-10** Preserve encrypted local conversation history across process and machine restarts. Support local deletion of messages/history, separately from canceling outstanding queued messages; never claim to delete recipients' copies or guarantee forensic erasure.

### Direct connectivity and independently operated relays

- **NET-01** Use rust-libp2p with QUIC preferred and TCP alternative; attempt authenticated **direct P2P first**, within a bounded interval.
- **NET-02** Attempt supported NAT traversal and hole punching. If a direct path is unavailable, automatically use an authorized **live** relay. Do not promise direct connectivity through every firewall/NAT.
- **NET-03** Support **multiple independently operated relays** without requiring inter-relay federation. Configure relay candidates locally or obtain them through verified invitations; no global relay directory in M1.
- **NET-04** A relay operator controls use through an authenticated identity allowlist and source/destination admission policy. Relay access is not proof of recipient identity, and knowing an address is not authorization.
- **NET-05** Try authorized, mutually usable relay candidates in configured priority order with automatic failover. While the Kami service is running, maintain and renew permitted relay availability/reservations and recover after relay interruption.
- **NET-06** When no direct or authorized relay path works, keep messages in the **sender's local outbox** until retry succeeds or the sender cancels. Successful delivery requires sender and recipient to be online concurrently; M1 has no server-side offline mailbox.

### Application execution and integration boundary

- **APP-01** Provide a **manually started** background Kami communication service. It manages connections, retries, crypto sessions, local message state, and receiving; do not silently install automatic startup.
- **APP-02** Provide a Rust CLI that communicates with the background service through a local authenticated/authorized IPC API: **Unix domain sockets on Linux/macOS**, **named pipes on Windows**. No public local-API TCP listener by default.
- **APP-03** The IPC API must be designed so future external harness adapters can use it without direct database/key access. Future agents receive per-agent scoped permissions; no adapter or agent execution is required in M1.
- **APP-04** Where OS secure key storage is unavailable or locked, fail safely and report a clear unsupported/unavailable state. Do not silently downgrade to plaintext secrets.

## 4. Delivery state and user-visible behavior

- **Queued:** The sender has durably retained a message locally.
- **In flight:** An active transmission attempt is underway; this is not a delivery guarantee.
- **Delivered:** The expected recipient device durably stored the authenticated message and the sender validated its delivery acknowledgment.
- **Blocked / failed attempt:** Identity mismatch, denied relay access, unsupported key storage, or network failure is surfaced clearly; retryable failures need not terminate the queued message.
- **Canceled:** The sender explicitly stopped attempting delivery of a queued message.
- **Read** is distinct from **Delivered** and is not an M1 receipt requirement. Likewise, an agent's future **task acceptance/completion** is not a device-delivery receipt.

## 5. Security and operational constraints

- Treat invitations, network input, messages, files, and future task payloads as untrusted data; never infer remote execution permission from message delivery.
- Keep message contents E2EE and locally encrypted at rest. Protect key material separately using OS-backed credential storage when supported; document metadata still visible to relays (such as IP addresses, timing, sizes, and routing identities).
- Apply bounded frames/messages, admission controls, storage quotas, retry backoff, and resource caps. Deny or safely reject malformed, oversized, unauthorized, or cryptographically invalid input.
- No external paid API or proprietary service is required for core transport. Deployment and resource costs for independent relays remain the operator's responsibility.
- Cross-platform support must be demonstrated, not inferred from Rust portability. Unsupported OS credential-store or crypto-library combinations are genuine blockers to resolve before M1 release.

## 6. M1 acceptance criteria

M1 is complete only after reproducible evidence demonstrates:

1. Two separately enrolled, independently verified endpoints exchange private text on **Linux, Windows, and macOS** using supported OS key protection.
2. The same authenticated logical message can reach its recipient via direct P2P or a deliberately forced, authorized live-relay fallback.
3. Tests exercise real, distinct networks, attempted NAT traversal/hole punching, and independently operated configured relays with permission/priority/failover behavior.
4. Encrypted message bodies remain unavailable to relay operators; unexpected identity/Peer-ID binding changes fail closed.
5. After sender/recipient process restart, queued/inbox messages, encrypted history, receipts, and deduplication state remain consistent.
6. Lost ACKs, duplicate sends, conflicting message IDs, out-of-order arrival, interrupted links, missing recipients, and relay restarts behave as specified.
7. No message is labeled Delivered until the intended recipient durably stores it and the sender validates the receipt.
8. Oversized/untrusted input, locked/unavailable key storage, unauthorized relay admission, and other relevant error cases produce safe diagnostics without leaking plaintext/secrets.
9. Automated unit/integration tests and documented cross-network/cross-OS qualification both pass for the supported operating environments.

M1 does **not** promise universal hole punching, offline-server delivery, uninterrupted availability, anonymous routing, exactly-once side effects, or a production SLA.

## 7. Subsequent capabilities (outside M1)

Kami's approved product direction includes future support for: organizations and project-scoped membership; group conversations and group encryption/fanout; encrypted attachments and resumable artifacts; a durable encrypted offline mailbox service; multiple devices under one participant identity with secure backup/recovery; external agent adapters and structured task/result/approval events; scoped agent capabilities and recipient-side execution authorization; and eventually optional cross-operator federation. These are **future implementation scopes**, not M1 acceptance requirements or commitments to a specific sequence/date.

## 8. Engineering ownership and change control

The Kami Architect and development agents select ordinary protocol field details, dependency versions, retry intervals, storage schemas, IPC methods, diagnostics, and other engineering specifics **within these approved constraints**. They must verify feasibility and interoperability before committing to a design; escalate material changes to security guarantees, M1 behavior/scope, OS support, or infrastructure costs. Delegation of engineering choices does not waive the repository-change and Jira approval rules in `AGENTS.md`.
