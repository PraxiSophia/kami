# Kami — Communication System Design

> **Companion to:** `Kami_Project_Requirements.md`  
> **Version:** 0.1 — architecture proposal  
> **Status:** For design review; technologies and thresholds are *not* final  
> **Last updated:** 2026-10-16

## 1. Objective and non-objectives

Kami's communication subsystem must deliver private messages, group events, task requests/results, and artifacts among humans and AI agents on different machines and networks. Transport should be independent of the agent harness and, as far as practicable, independent of whether a recipient is directly reachable.

**The core rule:** *a message's identity, permission rules, encryption and semantics do not change just because it travels by P2P, through a live relay, or via a durable offline mailbox.*

The communication subsystem is **not** the agent's model runtime, task planner, shell execution engine, or memory store. Reliable *message delivery* is not automatically reliable *task execution*.

## 2. Terminology: four often-confused server roles

| Mechanism | What it does | Does it store offline messages? | Typical role |
|---|---|---|---|
| Discovery / signaling | Helps participants learn identity, reachability, connection candidates, or rendezvous points | No | Connection coordination |
| NAT traversal | Attempts to establish a direct connection across NAT/firewalls | No | Reachability optimization |
| Live packet/stream relay | Forwards traffic between peers that cannot connect directly | **No** (not by itself) | Online fallback |
| Durable application mailbox | Stores encrypted messages for later delivery | **Yes** | Offline reliability |
| Group fanout service | Distributes one logical group event to its authorized recipients | Maybe, if backed by queues | Distribution policy |

**Critical distinction:** A libp2p Circuit Relay or WebRTC TURN server is not an offline inbox. A broker such as RabbitMQ/NATS can provide message routing and persistence, but does not by itself give peers direct P2P connectivity or NAT traversal.

## 3. Logical topology

```text
  HUMAN CLI                      AGENT ADAPTER                HUMAN CLI
  Alice / device A               Test agent / device B      Bob / device C
       |                                 |                         |
       +---------------+-----------------+-------------------------+
                       |  authenticated communication
                 +-----v-----------------------------+
                 | KAMI MESSAGE / COLLABORATION API  |
                 | identities, recipients, messages |
                 | task envelopes, acknowledgments  |
                 +----------------+------------------+
                                  |
                      DELIVERY POLICY / ROUTER
                                  |
                     +------------+-------------+
                     |            |             |
                Direct P2P     Live relay   Durable mailbox
                if reachable   if needed    if unavailable
                     |            |             |
                     +------------+-------------+
                                  |
                          Recipient device(s)
```

The drawing is **logical**. In practice, the client performs encryption/decryption and attempts direct connections; coordination, relay, and mailbox services can run on one self-hosted server in the first deployment but should remain independently defined responsibilities.

## 4. Architectural responsibilities

### 4.1 Client communication engine

Runs beside the CLI or agent adapter. Responsible for:

- local participant/device identity and cryptographic keys;
- constructing and validating messages;
- endpoint encryption/decryption;
- authenticated P2P sessions, relay sessions, and mailbox submission;
- persistent **outbox** and **inbox** in local storage;
- message retry, acknowledgment, duplicate suppression, and path selection;
- device reconnection and local state reconciliation;
- exposing a small, stable local API to human CLI and agent adapters.

Clients must not equate successful socket transmission with successful recipient delivery.

### 4.2 Discovery / coordination service

Responsible for:

- authenticated participant and device discovery, subject to organization/project permissions;
- signed endpoint information, peer addresses/candidates, and relay availability;
- optional presence hints (not authoritative guarantees of reachability);
- facilitating connection setup, including NAT traversal rendezvous.

It should not need plaintext chat/task payloads. It may nonetheless see identity, network address, and timing metadata; privacy claims must reflect this.

### 4.3 P2P connection manager

Attempts an authenticated direct connection, potentially through NAT traversal and hole punching. Connection establishment is opportunistic: some firewalls, CGNAT arrangements, enterprise policies, and network outages prevent it.

**Candidate to evaluate:** libp2p, which provides peer identities, encrypted connections, connection management, hole punching, and Circuit Relay v2. This is a *technology option*, not a final decision. For WebRTC-based designs, ICE/STUN/TURN is a distinct possible stack; do not treat TURN and libp2p Circuit Relay as interchangeable implementations.

### 4.4 Live relay

Forwards encrypted live streams when endpoints cannot directly connect. It may have bandwidth quotas, connection limits, routing metadata, and availability requirements. Relaying should still use authenticated **end-to-end** sessions so the relay cannot read application content. It is not automatically a durable queue.

### 4.5 Durable mailbox / delivery service

Receives encrypted per-device (or explicitly defined group-recipient) envelopes, persists them, and allows authorized recipients to fetch or stream them when they reconnect.

Required concerns: durable writes, acknowledgment protocol, expiry/TTL, device revocation, backpressure, quota enforcement, replay protection, poison messages, dead-letter policy, and recovery from server crashes.

**Implementation options to evaluate:** a modest gateway with a transactional mailbox database for the MVP; NATS JetStream or RabbitMQ when deployment scale/operations justify broker-based routing. A broker is not automatically a historical chat database. Store long-term user history primarily on endpoint devices unless product requirements explicitly call for encrypted server history.

### 4.6 Group distribution service

Determines which **authorized devices** must receive a group event. The service can assist with relay-side fanout and offline delivery without decrypting message bodies. Distribution membership information is sensitive metadata; group membership checks must apply at message creation and delivery, with explicit semantics for members joining/leaving.

## 5. The three principal delivery paths

### 5.1 Both endpoints online; direct P2P works

```text
 Alice  ---- encrypted authenticated P2P session ----> Bob
```

1. Alice resolves Bob's authorized device(s).
2. Alice establishes or reuses an authenticated connection.
3. Alice encrypts and stores the outbound logical message.
4. Alice sends to each intended device over the chosen connection.
5. Bob validates origin, membership, message ID, and crypto state; persists to local inbox.
6. Bob returns a device-delivery ACK. Alice marks delivery *only after* the corresponding ACK.

The coordination service may still be used during initial discovery; "P2P" does not necessarily mean completely server-free setup.

### 5.2 Both endpoints online; direct P2P fails

```text
 Alice ---- encrypted stream ----> Live relay ----> Bob
```

1. Client attempts direct connectivity for a bounded interval or concurrently with a relay path.
2. A valid relay path is established using authenticated end-to-end sessions.
3. The **same logical message ID** is sent via relay; a path change does not create a new user message.
4. Bob acknowledges after local durable receipt.
5. If an in-flight retry arrives by two paths, Bob displays/processes it once.

Avoid allowing a prolonged direct-connection attempt to block critical user/agent messages indefinitely. Path selection can favor low latency or low relay cost based on policy.

### 5.3 One or more recipients offline

```text
 Alice -> encrypted mailbox submission -> durable queue
                                      ... Bob reconnects ...
                            -> Bob fetches -> device ACK
```

1. Sender encrypts an envelope for authorized recipient device(s).
2. Sender submits ciphertext to the mailbox; server persists it transactionally.
3. Server sends an **accepted-for-queue** receipt, not a delivered receipt.
4. On reconnection Bob authenticates, fetches ordered available messages, decrypts, validates, and durably persists them locally.
5. Bob sends a device ACK; server can remove or mark the queued envelope according to policy.
6. TTL expiry, device revocation, quota exceeded, or unrecoverable crypto state must result in a visible failure state.

**Important availability caveat:** If all senders and potential holders of a message are offline and no durable service retains it, offline delivery cannot be guaranteed. "No third-party interception" is addressed by owning the mailbox and using E2EE, not by claiming zero server involvement.

## 6. How group chat should work

### 6.1 Separate group membership, encryption, and fanout

A group conversation has distinct problems:

- **Membership:** Who is authorized to send/receive?
- **Group key state:** Who is cryptographically able to decrypt each epoch of content?
- **Fanout:** How does each recipient device get its ciphertext?
- **Offline sync:** How do offline devices catch up, including membership/key changes?
- **Ordering:** How are concurrent messages displayed consistently?

One P2P connection reaches one remote endpoint. A group must build fanout using repeated direct sends, forwarding by peers, or a server-side distributor.

### 6.2 Fanout options

| Option | Behavior | Benefits | Weaknesses |
|---|---|---|---|
| Full mesh | Sender sends over direct connections to all available members/devices | Little online server bandwidth; simple for very small groups | Sender upload scales with recipients; connection count grows roughly O(N²) for complete mesh |
| Gossip/pubsub mesh | Peers forward to other peers | Reduced direct peer degree; interesting for large live peer networks | Harder delivery guarantees, duplicates, membership enforcement, offline sync |
| Relay-assisted fanout | Sender uploads encrypted event to distributor once | Predictable group delivery, easier offline queues | More server traffic, exposes routing/group metadata |
| Adaptive hybrid | Mix direct, live relay, and queued delivery by recipient | Optimizes reachability and cost | Most complex acknowledgments and duplicate handling |

**Proposed initial choice:** start with **server-assisted group fanout for correctness**, allow **direct P2P for small, online groups** as an optimization, and **do not put gossip in the MVP**. This is a proposal, not a fixed rule; measure group sizes and network patterns first. GossipSub is useful research for later peer-assisted dissemination, but is not equivalent to durable group chat.

### 6.3 Example: mixed-path group message

Group `#engineering`: Alice, Bob, Carol, David.

```text
 Alice creates logical message M-42, once
     |
     +------ P2P ----------> Bob (online, reachable) ---> ACK(B)
     |
     +------ Live relay ---> Carol (online, blocked) --> ACK(C)
     |
     +------ Mailbox -----> David (offline) ----------> ACK(D) later
```

This logical model is possible, but **recipient fanout must avoid double-publishing** when the same message is sent both to direct peers and a group mailbox. Exactly which entity tracks expected recipients is an explicit design decision.

**Critical group crypto nuance:** Do not assume the exact same ciphertext is always valid across every recipient or device. Per-recipient encryption, multi-device keys, or group encryption state can require separate envelopes. The logical message ID remains stable despite varying encrypted envelopes.

### 6.4 Encryption proposal

Evaluate **Messaging Layer Security (MLS; RFC 9420)** for asynchronous E2EE group key management. MLS specifies group key establishment and authenticated encrypted messages; it assumes external Authentication and Delivery Services. It does **not** solve NAT traversal, durable queuing, or application task permissions. Its epoch/commit ordering and offline catch-up semantics need careful design.

For 1:1 communication, evaluate a mature authenticated E2EE construction rather than inventing a new protocol. Device verification, multi-device membership, trust-on-first-use vs stronger verification, backup, lost keys, and post-compromise recovery need dedicated requirements.

## 7. Message model and separation of concerns

Every event uses a common logical envelope, independent of P2P or relay path. Example **illustrative schema** (not wire protocol):

```json
{
  "version": "kami.event.v1",
  "message_id": "01JEXAMPLEUNIQUEID",
  "conversation_id": "projA:engineering",
  "sender_participant_id": "alice",
  "sender_device_id": "alice-laptop",
  "recipient_scope": { "kind": "conversation", "id": "projA:engineering" },
  "event_type": "chat.message",
  "created_at": "2026-10-16T20:00:00Z",
  "encryption": { "scheme": "negotiated-e2ee", "key_epoch": "example" },
  "encrypted_payload": "<ciphertext-or-reference>"
}
```

Illustrative event types:

- `chat.message`
- `task.request`, `task.accepted`, `task.rejected`, `task.status`, `task.result`
- `approval.request`, `approval.decision`
- `artifact.reference`
- `group.membership_changed`, `crypto.key_epoch_changed`
- `delivery.receipt` (protocol event, not necessarily visible chat content)

An **envelope** carries routing and security data; the **encrypted application payload** carries the text, task arguments, private metadata, and artifact references where possible. A server must learn only what routing requires. If the design exposes conversation IDs or sender identities to servers, document the privacy consequence.

Do not put secrets, private prompts, or unrestricted file paths into plaintext routing fields.

## 8. Acknowledgments, ordering, retries, and deduplication

### 8.1 Separate acknowledgment levels

1. **Locally queued** — sender has durably written the event to local outbox.
2. **Server accepted** — durable mailbox persisted it (when applicable).
3. **Device delivered** — recipient device durably stored/processed envelope.
4. **Application accepted** — agent explicitly accepted a task; *not the same as device delivered*.
5. **Completed** — task result available, if it is a task.
6. **Read** — optional human UX signal; not implied by delivery.

### 8.2 Retry rules

- Assign a stable `message_id` before the first send; maintain it across retries and transport switches.
- Deduplicate incoming messages by authorized sender/device + message ID, and validate cryptographic authenticity.
- Keep a durable sender outbox until terminal delivery outcome or explicit expiration.
- Backoff and rate limit retries; avoid unlimited queue growth.
- Model queue pressure and errors visibly.

### 8.3 Group ordering

Network delivery is not inherently globally ordered. In the initial release, prefer **per-conversation or per-sender best-effort ordering with explicit deterministic display rules**. Do not promise a perfect universal total order without an authoritative sequencer/consensus design. MLS membership commits may demand additional ordered processing even when application messages tolerate reordering.

### 8.4 Agent task execution

**Transport retry is not execution retry.** Separate:

- `message_id` for network transport dedup;
- `task_id` for agent work identity;
- `attempt_id` for execution attempt (if supported).

Recipient adapter must enforce task authorization, optionally human approval, and idempotency or deduplication before performing side effects. Network failures can still make execution outcome ambiguous; do not promise universal exactly-once task execution.

## 9. Message lifecycle / state machine

```text
DRAFT -> ENCRYPTED -> LOCAL_OUTBOX -> IN_FLIGHT
                                  |       |
                                  |       +--> DIRECT_P2P
                                  |       +--> LIVE_RELAY
                                  |       +--> MAILBOX_ACCEPTED
                                  |
                                  +----------------------------+
                                                               v
                                                   DEVICE_ACKNOWLEDGED
                                                               |
                                                 (task-specific, optional)
                                                               v
                                                 TASK_ACCEPTED / REJECTED
                                                               |
                                                  RESULT / FAILURE

Expiry, revocation, permanent authorization failure, or quota failure
can terminate delivery before DEVICE_ACKNOWLEDGED.
```

Track recipient-device state separately for group messages. One group's event is not necessarily fully delivered because one member acknowledged it.

## 10. File and artifact transport

- Encrypt files client-side with per-object encryption keys and integrity protection.
- Share keys only with authorized recipients via the chosen E2EE scheme.
- Small files can travel directly over peer streams; larger files may benefit from chunked encrypted upload to organization-controlled blob storage.
- Transfer **references/metadata** in chat or task events; do not place large binary blobs directly in the durable text-message mailbox.
- Include object ID, content length, chunk manifest, integrity hash (applied at an appropriate layer), file name/metadata visibility policy, retention, and authorization.
- Ensure resume works after disconnection and revoked members cannot fetch future artifacts. Note that revocation cannot erase plaintext already downloaded.
- Limit untrusted files and enforce size, malware-scanning policy (compatible with encryption), quotas, and safe destination paths.

## 11. Identity, trust, and adversaries

### 11.1 Who may be untrusted?

- External network observers.
- A compromised or curious relay/mailbox operator.
- Unauthorized project participants.
- Malicious or compromised agent adapters.
- Malicious content injected into chat/task/artifact payloads.

E2EE protects *content confidentiality* from transport intermediaries, but does not automatically prevent endpoint compromise, metadata analysis, denial of service, malicious authorized peers, or unsafe agent execution.

### 11.2 Trust rules

- Each participant/device has authenticated credentials with defined lifecycle.
- Organization/project authorization is checked at submission and delivery; crypto membership must also remain current.
- Do not rely on a device's unverified display name.
- Agent adapters advertise capabilities, but **receiving endpoint** verifies authorization before invoking tools.
- Deletion/revocation must propagate to mailbox rights and crypto group membership, while acknowledging existing received copies cannot be remotely erased.
- Operator privileges are separated from plaintext access.

### 11.3 Server metadata

At minimum, the network may observe connection IPs, device identifiers or pseudonyms, queue addresses, timing, payload sizes, and delivery patterns. Avoid describing the design as anonymous unless a separate anonymity threat model is developed.

## 12. Multi-device participation

A human may use laptop + desktop; an agent may run on a workstation with a separate adapter process. Requirements to settle:

1. Are devices independently addressable and independently queued?
2. Must each device receive each message, or will one device sync decrypted history to another?
3. How are new devices verified and provisioned with group keys?
4. What happens if one device is lost or revoked?

**Recommended v0.1 baseline:** model devices explicitly and prefer per-device acknowledgment state. Defer seamless historical backfill until secure device onboarding and synchronization semantics are defined.

## 13. Agent adapter boundary

```text
Other participant --> Kami transport --> locally authenticated adapter
                                           |
                                   Policy and approvals
                                           |
                                   Harness-specific API
                                           |
                                 Existing agent execution
                                           |
                      Task status / results / artifacts via Kami
```

Kami defines common task messages, not the execution runtime. Adapter responsibilities:

- advertise supported operations and authentication requirements;
- map permitted `task.request` into an existing harness API/CLI/interface;
- manage task IDs, status, timeouts, and cancellation if supported;
- reject unsupported or unauthorized operations explicitly;
- surface results without leaking unrelated workspace data;
- distinguish receipt from execution acceptance;
- avoid granting remote users general shell access merely because a task envelope exists.

**A2A compatibility:** Where useful, preserve A2A task semantics and expose standard A2A endpoints over private networking or implement a carefully specified compatible binding. Arbitrary custom packet transport is not automatically A2A-compatible.

## 14. Infrastructure boundaries: one shared service vs federation

**Initial proposal:** one self-hosted logical deployment can host multiple organizations and projects; use tenant-aware access control and encrypted per-device mailboxes. An organization is *not* the same as a physical relay instance.

**Potential future:** independently operated deployments exchange authenticated, encrypted events through federation while preserving recipient ownership and trust policies. Federation adds cross-server identity, group membership, delivery receipts, key distribution, moderation, and failure recovery complexity. It is a future direction rather than an MVP requirement.

## 15. Failure cases that the implementation must test

| Situation | Expected behavior |
|---|---|
| Both users behind restrictive NAT | Attempt direct, fall back to authenticated live relay |
| Recipient shuts laptop | Queue encrypted messages; report `server accepted` not `delivered` |
| Relay disconnects mid-message | Retry through available paths with stable ID; deduplicate |
| Mailbox database restarts | Previously acknowledged durable messages remain available |
| Recipient reconnects on another IP | Reauthenticate and resume delivery independent of old address |
| Duplicate P2P and queue delivery | One visible message, multiple transport receipts possible |
| Group member removed | Reject future sends/delivery and rotate cryptographic state as needed |
| Group key epoch missed while offline | Recover ordered crypto state or surface explicit rejoin/recovery path |
| Agent gets same task twice | Do not cause unintended duplicate side effects |
| Agent lacks approval | Reject or pause task rather than execute |
| Artifact truncated/corrupted | Integrity check fails; resume or retry |
| Malicious content claims to be a system command | Treat as untrusted payload, not trusted instruction |
| Mailbox TTL expires | Surface failure or expiration; do not falsely claim delivery |

## 16. Candidate implementation components (not committed)

| Concern | Early candidate | Alternative / notes |
|---|---|---|
| Implementation language | Rust for client/relay/mailbox components | Owner-selected language; technical stack remains draft pending approval and validation |
| Async/runtime boundary | Tokio for the client and relay processes | Exact runtime profile, crate versions and feature graph remain unverified |
| CLI interaction | Rust command-mode CLI using `clap` | Plain command mode first; interactive UI remains deferred |
| P2P | `rust-libp2p` over TCP with Noise and Yamux | Exact crate/API compatibility and endpoint identity binding require validation |
| Live fallback | `rust-libp2p` Circuit Relay v2 in a separate relay process | Relay authorization, reservation/circuit limits and forwarding behavior remain unverified; QUIC/DCUtR only if an approved NAT profile justifies them |
| Mailbox persistence | Deferred transactional encrypted application mailbox | Not part of the online FM relay; server datastore, async E2EE and receipt contract remain open |
| Local client history | SQLite with a separately approved local-at-rest/key-storage strategy | SQLite, WAL, backups and private keys are not encrypted by this choice alone |
| Group cryptography | MLS implementation evaluation | Assess practical libraries and membership semantics |
| File blobs | Organization-owned encrypted object storage | Direct chunked P2P for appropriate cases |
| Agent interfaces | Local adapters + A2A-compatible semantics | Adapter-specific limits advertised |

**Do not choose multiple overlapping transports or brokers before demonstrating one end-to-end path.**

## 17. Phased communication implementation proposal

### Phase C0 — Delivery contracts and test harness

Define participant/device IDs, event envelope, task IDs, delivery receipts, expiration, encryption boundaries, and transport-independent API. Implement simulators for offline recipients, duplicates, retries, and partial failure.

### Phase C1 — Private one-to-one over one reliable path

CLI A and CLI B authenticate, exchange encrypted text, persist local history, and acknowledge delivery. A simple self-hosted relay/connection path may be used before adding direct P2P optimization.

### Phase C2 — P2P plus live fallback

Add authenticated peer discovery, direct connection attempts, live relay, path migration, and end-to-end tests across different residential networks/CGNAT conditions.

### Phase C3 — Offline mailbox

Durable ciphertext queue, recipient reconnection, quotas, expiry, ACKs, dedup, replay behavior, server restart recovery.

### Phase C4 — Groups and artifacts

Small mixed group chats, chosen group crypto method, group fanout strategy, encrypted file references and blob/direct transfer. Demonstrate different recipient connectivity states.

### Phase C5 — Harness adapters

Local adapter API and two independent harness integrations; task request/accept/result flow; approvals, idempotency, task/artifact transport. Prove two users on different networks can collaborate through different harnesses.

These are milestones to validate assumptions; not a calendar schedule.

## 18. Evaluation / success measures

Record results in a reproducible test matrix instead of promising arbitrary numerical SLAs at this stage.

- Direct P2P establishment success rate by NAT environment.
- Live relay failover time and message-loss behavior.
- End-to-end device delivery time on direct vs relay paths.
- Queued delivery success after client/server restart.
- Duplicate suppression across retries and path switches.
- Group delivery coverage and sender/server bandwidth for varying N.
- Offline backlog catch-up time and bounded storage footprint.
- Unauthorized task execution attempts blocked.
- Ciphertext-only storage assertions for relays/mailboxes.
- Cross-harness task status and artifact correctness.

Define numerical acceptance thresholds after baseline measurements and target deployment constraints are known.

## 19. Open architecture decisions

1. The exact choice of P2P stack and whether gateway-based service transport should precede P2P.
2. Live path preference: direct-first vs happy-eyeballs parallel attempt.
3. Group fanout policy, membership distribution, and ordering semantics.
4. MLS suitability, chosen 1:1 E2EE protocol, and crypto library compatibility.
5. Metadata minimization versus efficient routing and organization auditing.
6. Explicit max group size, file size, queue TTL, and storage quotas.
7. Multi-device sync, identity recovery, and lost-key recovery semantics.
8. Broker vs transactional mailbox and whether to deploy separate infrastructure components.
9. Future federation and trust model.
10. A2A bindings and adapter support variability.

## 20. Technical references

- libp2p, Hole Punching: https://docs.libp2p.io/concepts/hole-punching/
- libp2p, Circuit Relay: https://libp2p.io/docs/circuit-relay/
- libp2p, Gossipsub specification: https://github.com/libp2p/specs/tree/master/pubsub/gossipsub
- IETF RFC 9420, Messaging Layer Security (MLS): https://www.rfc-editor.org/rfc/rfc9420.html
- Japanese expression *ishin-denshin*, Nichiren Buddhist glossary: https://www.nichiren.or.jp/glossary/id47/

---

**Summary:** Kami can support P2P, live relay, group fanout, and durable offline delivery without confusing them as the same mechanism. The most important architectural boundary is between *how a message reaches another device* and *what that message means to a human or agent*. Both must remain secure and independently testable.
