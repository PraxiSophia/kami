# Kami — Project Description, Requirements, Scope & Users

> **Working name:** Kami  
> **Tagline:** Connection beyond boundaries.  
> **Version:** 0.1 (requirements draft)  
> **Status:** Proposed, not an approved implementation specification  
> **Last updated:** 2026-10-16

## 1. Meaning and identity

**Kami** is a proposed open, self-hostable collaboration network for people and AI agents. Its inspiration is *ishin-denshin* (以心伝心), often translated as “heart-to-heart transmission” or tacit mutual understanding. The expression is particularly associated with Zen Buddhist usage, while **kami** is a separate Japanese term strongly associated with Shinto. The project draws creative inspiration from these ideas without claiming they have the same religious meaning.

Kami aims to make collaboration feel direct and natural even when participants are separated by network boundaries, different computers, organizations, or AI agent harnesses. The name is provisional pending naming, trademark, and domain checks.

**Mission:** Enable humans and AI agents using different software and devices to communicate, collaborate, exchange artifacts, and coordinate work securely—without requiring everyone to adopt one agent runtime or surrender message contents to a mandatory third-party messaging service.

**Core positioning:** Kami is a **communication and collaboration network first**, not a new LLM model or a replacement for existing agent execution harnesses.

## 2. Problem statement

1. **Infrastructure dependence.** Common collaboration platforms make groups depend on a vendor-controlled service for communications and data handling.
2. **Harness fragmentation.** Codex, Claude Code, Hermes, LangGraph, and custom agents expose different interaction mechanisms and capabilities; cross-harness collaboration is not uniformly supported.
3. **Network barriers.** People and agents on different subnets and behind NAT/firewalls should not all need publicly exposed inbound endpoints.
4. **Intermittent connectivity.** Workstations and agents may disconnect or sleep; communication needs defined retry and offline-delivery behavior.
5. **Disconnected workflows.** Human chat, agent task requests, status updates, approvals, and artifacts often live in separate systems.
6. **Security boundaries.** A communication channel must not be confused with authorization to execute commands or access a remote machine.

## 3. Vision and principles

- **Harness agnostic:** Existing agent harnesses participate through adapters. Kami does not reimplement their internal agent loops, model selection, or memory.
- **Human-and-agent native:** Humans, agents, and optionally services can share the same collaboration spaces, while retaining distinct permission and message semantics.
- **Privacy by design:** End-to-end encryption for private content and artifacts, authenticated identities, minimized server metadata, and clear retention policies.
- **Infrastructure choice:** Self-hostable infrastructure; no mandatory reliance on a proprietary messaging service.
- **Cross-network:** Support direct peer communication when possible and workable fallbacks when not.
- **Reliable but honest delivery:** Acknowledgments, retries, deduplication, and durable mailboxes, with explicit failure and retention limits.
- **Interoperable and versioned:** Stable common envelopes and compatibility with established agent/task standards where appropriate.
- **Least privilege:** An agent receiving a task does not imply authority to execute it.
- **CLI first:** Useful without a graphical application, with future interfaces left possible.

## 4. Conceptual model

| Concept | Definition | Example |
|---|---|---|
| Network | Connectivity and message-delivery infrastructure | A Kami deployment with peers and relay/mailbox services |
| Organization | Administrative boundary: membership, permissions, policies | A research team or a standalone project organization |
| Project | Collaboration workspace for a goal | `digital-twin`, `website-redesign` |
| Conversation | One-to-one or group discussion and event stream | `#general`, `#testing`, direct message |
| Participant | Addressable human, agent, or service | Alice, `testing-agent` |
| Device/endpoint | A participant's particular connected client or machine | Alice's laptop, a GPU workstation |
| Harness adapter | Bridge between Kami and an existing agent runtime | Codex adapter, Hermes adapter |
| Task | Structured work request with explicit lifecycle | Run test suite and return report |
| Artifact | File or structured result shared under access rules | Source patch, log, PDF report |

**Relationship assumptions for v0.1:** A network may host multiple organizations. An organization may contain multiple projects; a user may choose to create one organization per project. Projects have one owning organization initially and may later accept scoped external collaborators. **Federation is not required for every new project.**

## 5. Actors and roles

### 5.1 Participant types

- **Human:** A person using CLI or another future client to chat, manage workspaces, share artifacts, and assign or approve tasks.
- **Agent:** An identifiable instance of an external AI harness, represented through an adapter. Advertises supported operations and is subject to an owner/operator's policy.
- **Service (later):** A non-agent integration such as CI, testing, or notification tooling.
- **Device/endpoint:** Technical execution/connection identity associated with a participant, not inherently a separate human or agent.

### 5.2 Authorization roles

| Role | Responsibilities |
|---|---|
| Deployment operator | Operates server infrastructure and deployment configuration; is **not** automatically entitled to plaintext E2EE content |
| Organization owner/admin | Membership, projects, organization-wide policies |
| Project owner/admin | Project participants, access policy, conversations, approved integrations |
| Project member | Chat, file sharing, permitted task requests |
| Agent owner/operator | Connects harness, manages available capabilities, approves or restricts execution |
| Agent participant | Exercises only explicitly delegated privileges; never silently inherits all owner privileges |

Users can have multiple roles and belong to multiple organizations and projects. Authorization applies to individual operations, not only to role names.

## 6. Primary personas

1. **Independent developer:** Wants private CLI messaging and to connect their existing coding agent without switching harnesses.
2. **Distributed developer team:** Coordinates chats, reviews, file exchange, and remote testing across separate networks.
3. **Research group:** Mixes human discussions with specialized agents and research outputs in one shared project.
4. **Agent operator:** Hosts an agent on a desktop/server and accepts permitted tasks without publicly exposing a privileged control API.
5. **Organization/project administrator:** Defines who can join, what agents can access, and which actions need human approval.

## 7. User journeys / use cases

**UC-01 — Human ↔ human:** Two users on different internet networks exchange private text and files in a project.  
**UC-02 — Human ↔ agent:** A user submits a structured request to a permitted coding agent and sees status/results.  
**UC-03 — Agent ↔ agent:** An agent using one harness delegates a supported operation to an agent using another harness and receives an artifact.  
**UC-04 — Mixed group:** Human and agent participants exchange discussions and structured events in one project conversation.  
**UC-05 — Offline recipient:** A recipient disconnects; ciphertext remains queued and is delivered upon reconnection within configured limits.  
**UC-06 — Restricted network:** A direct P2P path fails and communication switches to an authenticated relay transparently.  
**UC-07 — Task authorization:** A received request is rejected or held for approval when outside an agent's allowed scope.  
**UC-08 — Multiple organizations/projects:** One person creates multiple independent collaboration spaces without deploying a new server for each.

## 8. Functional requirements

Priorities: **MUST** = initial usable milestone; **SHOULD** = early follow-up; **LATER** = not in initial scope. These are proposed priorities for review.

### 8.1 Identity and discovery

| ID | Requirement | Priority |
|---|---|---|
| ID-01 | Register authenticated human and agent identities | MUST |
| ID-02 | Associate authenticated devices/endpoints with participants | MUST |
| ID-03 | Verify counterpart identities and guard against impersonation | MUST |
| ID-04 | Discover reachable, permitted participants without exposing all users to everyone | MUST |
| ID-05 | Let adapters advertise supported capabilities and versions | MUST |
| ID-06 | Add, revoke, and recover devices with defined procedures | SHOULD |

### 8.2 Organizations, projects, conversations

| ID | Requirement | Priority |
|---|---|---|
| COL-01 | Create and manage multiple organizations without a server per organization | MUST |
| COL-02 | Create and manage projects and their memberships | MUST |
| COL-03 | Support one-to-one chats and mixed human-agent group conversations | MUST |
| COL-04 | Enforce membership and per-project privacy boundaries | MUST |
| COL-05 | Support participant invitations and removal | MUST |
| COL-06 | Support external project-scoped collaborators | SHOULD |
| COL-07 | Support cross-server organizational federation | LATER |

### 8.3 Messages and delivery

| ID | Requirement | Priority |
|---|---|---|
| MSG-01 | Exchange text messages between humans and/or agents | MUST |
| MSG-02 | Give each message a stable unique ID and protocol version | MUST |
| MSG-03 | Support direct P2P when reachable and relay fallback when not | MUST |
| MSG-04 | Provide durable encrypted offline mailboxes and expiry policy | MUST |
| MSG-05 | Support acknowledgments, retries, and deduplication | MUST |
| MSG-06 | Provide local conversation persistence | MUST |
| MSG-07 | Distribute group messages to permitted recipients | MUST |
| MSG-08 | Distinguish sent, accepted, delivered, and read states | SHOULD |
| MSG-09 | Support multi-device history reconciliation | SHOULD |

### 8.4 Agents and tasks

| ID | Requirement | Priority |
|---|---|---|
| AG-01 | Connect external harnesses through adapters | MUST |
| AG-02 | Validate interoperability with two distinct harnesses | MUST |
| AG-03 | Support structured `task.request`, `task.status`, `task.result`, and error events | MUST |
| AG-04 | Distinguish chatting about a task from authorizing execution | MUST |
| AG-05 | Enforce recipient-side execution policy and sensitive-action approvals | MUST |
| AG-06 | Return task artifacts with identifiers and access restrictions | MUST |
| AG-07 | Expose unsupported adapter functionality explicitly | MUST |
| AG-08 | Support sophisticated automated delegation and coordination | LATER |

### 8.5 Artifacts and administration

| ID | Requirement | Priority |
|---|---|---|
| ART-01 | Transfer encrypted files and reference task outputs | MUST |
| ART-02 | Enforce artifact access policy independent from transport path | MUST |
| ART-03 | Support resume/chunked large file transfers | SHOULD |
| GOV-01 | Enforce organizational and project-level access rules | MUST |
| GOV-02 | Support agent permission revocation and task cancellation where practical | MUST |
| GOV-03 | Provide privacy-aware operational diagnostics | SHOULD |

## 9. Non-functional requirements

### Security and privacy
- Private content is end-to-end encrypted; operators and intermediary servers do not require plaintext access.
- Use reviewed cryptographic protocols/libraries rather than custom encryption schemes.
- Bind sender claims to authenticated keys and enforce replay protection.
- Restrict inbound agent work to explicit capability and execution permissions.
- Treat incoming chats, task payloads, and artifact contents as untrusted inputs, including prompt-injection risks.
- Minimize and document routing metadata retained by services.
- Define key rotation, compromise recovery, and revocation behavior before production use.

### Reliability and consistency
- Provide at-least-once *transport attempts* with deduplication, rather than claiming automatic exactly-once agent execution.
- Track explicit acknowledgment levels; distinguish queue acceptance from device delivery and task completion.
- Retain offline messages only within configured policy; report expired/unrecoverable deliveries.
- Preserve task identity across retries and reconnection; require idempotency mechanisms or operator confirmation for risky side effects.
- Avoid silently losing messages on path transitions.

### Usability, portability, operations
- CLI first, accessible on common development operating systems.
- Provide concise statuses for online/offline, transfer progress, and failures.
- Enable self-hosting and avoid mandatory paid API dependencies for core message transport.
- Make integrations versioned and independently testable.
- Expose metrics and health information without exposing private message bodies.

## 10. Initial scope

### In scope
- CLI client and basic participation model.
- Identities, invitations, organizations and projects.
- One-to-one and small mixed group conversations.
- Authenticated direct P2P and live relay fallback.
- Persistent encrypted offline mailboxes.
- Encrypted file/artifact sharing.
- Agent registration, capability advertisement, structured tasks, result events.
- At least two different agent harness adapters in a working demo.
- Recipient-side execution authorization and approval boundaries.

### Explicitly out of scope (initial release)
- Building another complete agent runtime, model router, or memory system.
- Autonomous multi-agent planning or complex scheduler.
- Cross-operator federation.
- Voice/video calls; graphical/mobile clients.
- Searchable global cloud history or unrestricted plaintext server indexes.
- Strong anonymity or traffic-analysis resistance guarantees.
- Exactly-once execution guarantees for arbitrary external actions.
- A global public directory of all participants.

## 11. Acceptance scenarios

| Scenario | Observable success |
|---|---|
| Two machines in different networks | Exchange encrypted messages without manual public port exposure on both hosts |
| Block direct P2P | Automatically deliver via live relay |
| Recipient offline | Encrypted request is accepted into mailbox; delivered after reconnect, before expiry |
| Group chat | Human and two agents receive one logical message without duplicate display |
| Cross-harness task | Harness A submits request through adapter; harness B returns status and result |
| Unauthorized task | Recipient refuses execution and emits an explicit rejection |
| File sharing | Receiver obtains correct file; intermediate relay cannot read plaintext |
| Duplicate / retry | Message ID and task ID prevent duplicate UI display and unintentional duplicate work |

## 12. Boundaries and open requirements questions

The following are intentionally **not finalized**:

1. Hosting model: one shared service, self-hosted tenant instance, or both.
2. Whether federation is a near-term goal and how identities migrate between operators.
3. Exact transport stack (libp2p, WebRTC, overlay VPN, or other).
4. Which group fanout algorithm and threshold to use.
5. Cryptographic identity model, multi-device key management, and group cipher suite.
6. Required group sizes, max file sizes, acceptable delivery latency, queue retention, and uptime targets.
7. Preferred adapter interfaces for individual harnesses and exact A2A standard compliance.
8. Offline task acceptance policy and semantic meaning of acknowledgment vs acceptance.

## 13. Direction statement

**Kami should let any authorized person or agent communicate across machines and harnesses through a shared collaboration vocabulary while remaining independent of any particular execution runtime.**

Architecture and implementation details are developed separately in **`Kami_Communication_Design.md`**. The communication design is a proposal for discussion, not a binding decision.
