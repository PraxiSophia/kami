# Kami Rust Architecture Decision Record

Status: Draft — Pending Approval
Decision authority: Owner language choice only; this record authorizes no implementation.

## 1. Purpose and decision boundary

This record reconciles the Rust direction for the online First-Message Milestone (FM): two independently initialized and explicitly authorized human-operated CLI endpoints exchange one private UTF-8 text message while online; the client attempts authenticated direct P2P and automatically uses an authenticated live Circuit Relay v2 path when direct delivery is unavailable; the recipient durably records the message before emitting a device-delivery receipt; retries and path changes retain one logical message ID and produce at most one visible recipient message.

The proposed full v0.1 capabilities remain separately gated: durable offline mailboxes, organizations and projects, permission-aware discovery, groups, artifacts, multi-device synchronization, agent adapters, structured tasks, A2A posture, federation, and production identity recovery/revocation. A live relay is not an offline mailbox. No item in this record approves an unverified capability, releases a protocol, or authorizes implementation.

## 2. Owner-approved decision

| Decision | State | Boundary |
|---|---|---|
| Implementation language: Rust | Owner approved | This is the sole approved language choice. It does not approve crates, APIs, versions, targets, cryptographic protocols, packaging, or implementation work. |

The Rust choice is recorded as an owner decision. Every technical selection and validation outcome below remains Draft — Pending Approval until the named approval gates and tests produce evidence.

## 3. Proposed technical direction

| Concern | Proposed direction | State and required evidence |
|---|---|---|
| Async runtime | Tokio-owned asynchronous client and separate relay process | Draft — Pending Approval; validate task ownership, cancellation, liveness, and target builds. |
| Peer networking | rust-libp2p | Draft — Pending Approval; validate exact API and feature compatibility on the pinned toolchain. |
| FM live transport | TCP with libp2p Noise and Yamux | Draft — Pending Approval; validate endpoint authentication, device-to-Peer-ID binding, framing, and interoperability. |
| Live fallback | Circuit Relay v2 in a separate relay process | Draft — Pending Approval; validate reservation, circuit establishment, inner endpoint authentication, admission, limits, and restart behavior. |
| Local durable facts | SQLite for outbox, inbox, deduplication, and receipt state | Draft — Pending Approval; validate transaction ordering, sync/journal behavior, locks, crash recovery, storage-full behavior, and worker isolation. |
| CLI | clap-based command-mode Rust CLI | Draft — Pending Approval; validate profile isolation, safe rendering, exit/status semantics, and cancellation. |
| Mailbox | Deferred separate transactional encrypted application mailbox | Draft — Pending Approval; not part of FM and not supplied by Circuit Relay v2. Requires a separately reviewed asynchronous E2EE and receipt contract. |
| NAT traversal | Direct-first bounded policy for the FM baseline | Draft — Pending Approval; measure actual named network profiles. DCUtR or QUIC may be investigated only when an approved profile justifies the additional path and security review. |

No exact crate version, feature set, wire encoding, protocol identifier, cryptographic suite, keyring backend, database encryption mechanism, target OS, package format, or deployment profile is selected by this table.

## 4. Proposed FM boundaries

The client should persist the sender outbox record before network I/O. The recipient should validate the authenticated endpoint, message binding, bounds, and deduplication state, then atomically commit inbox, deduplication, receipt, and display facts before sending a device-delivery ACK. The sender should report `device_delivered` only after validating the correlated recipient ACK. A direct-to-relay retry retains the same logical message ID and content binding; fresh Noise sessions may produce different transport ciphertext.

The relay forwards live encrypted traffic and may observe the routing metadata required to operate. Authentication to the relay alone does not authenticate the recipient. The inner endpoint-to-endpoint secure session must bind the authenticated Peer ID to the explicitly verified device record. Reservation, circuit, source, and destination authorization must be enforced before useful forwarding under the approved FM policy.

SQLite is a proposed local state mechanism, not an at-rest confidentiality guarantee. Local history, pending outbox data, WAL files, backups, temporary pages, and private transport keys require a separately approved protection policy. Blocking storage and key-provider operations must not stall the continuously polled Tokio/libp2p transport owner.

## 5. Validation register — all unverified

The following remain unverified and cannot be described as capabilities, guarantees, or release evidence:

- exact Rust toolchain, rust-libp2p/Tokio/SQLite/clap crate versions, feature graph, API compatibility, and dependency security review;
- TCP + Noise + Yamux interoperability, endpoint/session termination, device-to-Peer-ID binding, and protocol/framing behavior;
- separate relay process packaging, identity protection, reservation renewal, health, restart behavior, and operator responsibilities;
- pre-forward relay authorization for reservations and source-to-destination circuits;
- circuit and connection limits for reservations, duration, bytes, rates, concurrency, and overhead from Noise, Yamux, framing, and ACKs;
- protected key storage, local history confidentiality, SQLite journaling/synchronization, WAL and backup handling, profile locking, and headless key-provider behavior;
- direct connectivity, NAT traversal, unsupported egress behavior, and the conditions under which DCUtR or QUIC are justified;
- cross-network delivery, failover timing, resource usage, and performance under an approved topology and sample policy;
- packaging, native linkage, claimed operating systems/architectures, reproducible builds, and installation/upgrade behavior;
- any asynchronous mailbox, group, artifact, adapter, task, federation, or broad revocation capability.

A documentation example, library default, localhost test, forced-relay run, or sentinel scan is not evidence that these items are validated.

## 6. Approval and evidence gates

1. Scope approval freezes the online FM boundary and preserves all deferred v0.1 obligations.
2. Security and identity approval freezes the enrollment/verification ceremony, changed-key behavior, endpoint binding, live-session configuration, protected key policy, and local-history policy.
3. Protocol and storage approval freezes the bounded frame, logical ID/content binding, receipt schema, state reducer, retry/expiry rules, dedup retention, SQLite durability assumptions, and late-ACK behavior.
4. Transport and operations approval pins the Rust toolchain and dependency features, confirms separate relay operation, proves pre-forward authorization and configured circuit limits, and names supported network/OS profiles.
5. QA qualification must demonstrate direct delivery on a recorded eligible profile and automatic live-relay fallback after a genuine direct-path failure on separate endpoint networks with a distinct third relay process. It must also cover duplicate/path races, ACK loss, restart, malformed input, changed identity, no-route, safe diagnostics, and storage/key faults.

Passing a gate authorizes only the bounded, approved claim for that gate. It does not authorize implementation, production deployment, a cryptographic certification, universal NAT reachability, an SLA, or any deferred v0.1 capability.

## 7. Relationship to source documents

- `docs/planning/Master_Execution_Roadmap.md` is the sequencing authority for the proposed FM path and remains Draft — Pending Approval.
- `docs/Kami_Communication_Design.md` section 16 records the nonbinding Rust technical guidance; its separation of direct P2P, live relay, and durable mailbox remains authoritative for the design proposal.
- `docs/planning/First_Communication_MVP.md`, `docs/planning/CLI_Implementation_Plan.md`, and `docs/planning/Test_And_Validation_Plan.md` provide aligned Rust planning proposals and test gates, not approvals.
- `docs/architecture/Architecture_Review.md` and the historical language evaluations remain unchanged historical evidence. Their unresolved risks are carried forward here rather than rewritten.
- `docs/Kami_Project_Requirements.md` remains language-agnostic, proposed, and unchanged.
