---
last_analyzed_branch: "protocol-storage"
last_analyzed_commit: "438d466"
last_analyzed_timestamp: "2026-11-11T22:45:00Z"
milestone_1_completion_overall: "Not estimable"
whole_project_completion_overall: "Not estimable"
---

# Kami Code Analysis Home

Welcome to the **Kami Code Analysis** knowledge base. This documentation is designed to help you explore, learn, and track the Rust source code across all crates in the Kami workspace.

Whether you are new to Rust or learning the Kami architecture, these notes break down every module, struct, enum, and function into plain, beginner-friendly English with direct links to the source files.

---

## What is Kami?

**Kami** is a communication-first platform built for secure, end-to-end encrypted messaging between:
- Humans and humans
- Humans and AI agents
- AI agents and AI agents

Kami is built in **Rust** using an architecture-first approach. For Milestone 1 (M1), the goal is private, reliable one-to-one text messaging between two CLI endpoints across Linux, macOS, and Windows.

---

## Milestone 1 Status Overview

Milestone 1 focuses on ten core foundations:
1. **Domain Core**: Pure data structures, message identity, deduplication logic, and delivery state tracking (`kami-core`).
2. **Wire Protocol**: Strict, bounded, zero-allocation-on-bad-input Protobuf serialization (`kami-protocol`).
3. **Key Custody & Contact Trust**: OS keyring integration (Linux Secret Service, macOS Keychain, Windows Credential Manager), bilateral fingerprint verification, and signed Olm prekey bindings (`kami-identity`, `kami-key-provider`).
4. **End-to-End Encryption**: Matrix Vodozemac Olm v1 Double Ratchet engine, canonical inner envelopes, staged authenticated commits, and receipt boundaries (`kami-e2ee`).
5. **Encrypted Storage Engine**: SQLCipher-encrypted SQLite database with profile filesystem leasing, outbox retries, inbox deduplication, contact trust CAS, E2EE session persistence, outbound sender state, and inbound receiver state (`kami-storage`).
6. **Outbound Sender Coordinator**: Synchronous, runtime-neutral message coordination, sequence reservations, 30s attempt leases, exponential backoff, exact-frame crash recovery, and delivery receipt consumption (`kami-sender`).
7. **Inbound Receiver Coordinator**: Synchronous, runtime-neutral inbound frame coordination, duplicate deduplication, presentation ordering, same-session receipt staging, and delivery artifact persistence (`kami-receiver`).
8. **Asynchronous Daemon Supervisor & Runtime**: Tokio multi-threaded async actor loop, bounded channels, dedicated single-thread blocking worker, profile filesystem lease retention across timeouts, pluggable relay reservation scheduling, and private mock ingress boundary (`kami-runtime`).
9. **Authenticated Direct Transport**: Mutually authenticated peer-to-peer libp2p QUIC/TCP connections, expected-peer dial binding, fresh contact trust verification, and revocable connection capabilities (`kami-transport`, KAMI-27).
10. **Background Service & CLI**: The client interface and background daemon (`kami-cli`, currently in scaffolding phase).

### Key Architectural & Jira Distinctions
- **KAMI-22 & KAMI-23 Functional Completion**: Both `kami-storage` (KAMI-22) and `kami-identity` (KAMI-23) are **functionally implemented in Rust**. Their Jira issues remain *In Progress* because native Windows and macOS platform qualification (filesystem ACLs and native keyring runs) remain release gates, not because core code is missing.
- **KAMI-50 (Contact Trust & Fingerprints)**: Fully implemented and tested. Enforces bilateral human verification and device-to-Peer-ID cryptographic binding.
- **KAMI-51 (E2EE Work Packages & Acceptance Scope)**:
  - Overall KAMI-51 Jira parent issue is **To Do**; full production messaging is disabled.
  - **WP1**: Maintained provider foundation (`vodozemac = 0.11.1`) with fail-closed public AAD boundary (`AadUnsupported`).
  - **WP2-A1**: Identity-owned signed Olm account and prekey binding.
  - **WP2-A2**: Canonical authenticated inner envelope (`InnerEnvelopeV1`) and 64 KB bounds.
  - **WP3**: Private persistence snapshots and SQLCipher migration 003 (**Owner Accepted** as a bounded private storage foundation).
  - **WP2-B**: Typed storage composition (`AuthenticatedCommit` via `Storage::commit_authenticated`), **Owner Accepted with Conditions** for quarantined scope only (no public activation, no ACK emission authority, no external rollback anchor, and no production qualification claim).
  - **WP-A (Authenticated Context & Provider Contract)**: **Accepted with Limitations** (runs 187, 188, 189, Kanban comment 176). Enforces `NativeAadProvider` with `supports_aad() = false` and `AadUnsupported` for native AAD calls; authenticates canonical `InnerEnvelopeV1` inside Olm; uses outer SHA-256 strictly as a correspondence commitment; validates negative-security boundaries. Pure E2EE code cannot establish live runtime provenance or snapshot freshness; in-process custodian consumes pickles (trusted custody, not a malicious-custodian or endpoint-compromise defense).
  - **WP-B (Prekey Lifecycle & Endpoint Exchange)**: **Accepted with Limitations** (implementation `t_d364301b` run 204, QA `t_0dcc13ac` run 205 PASS, Evaluator `t_250e6416` run 206 PASS-WITH-CONDITIONS; doc/fixtures `t_e69f80fb` run 207 / `t_3f69df9c` run 208 PASS; manual harness extension `t_a328ea09` run 211, QA `t_c21848ef` run 212).
    - `AccountLifecycle`: generates 32 initial one-time prekeys (OTKs) and 1 fallback prekey. Capped at 50 OTKs, fallback rotation/usage capped at 16, signed validity capped at 5 minutes (`MAX_VALIDITY_MS = 300_000`).
    - Dedicated wire subprotocol: `/kami/prekey-exchange/1` with 16 KiB frame limit, 5s deadline, max 1 stream per connection, max 16 global concurrent streams.
    - SQLCipher migration 007 (`007_prekey_lifecycle.sql`): `prekey_clock`, `prekey_inventory`, `prekey_publication`, `prekey_session`, `prekey_authority`, `prekey_query`, `prekey_received`, trigger bounds (1024 / 32). Enforces atomic CAS freeze-before-send reservation, retention before publication ACK, and exact-byte retry reconciliation.
    - Runtime Engine APIs: `replenish_prekeys`, `rotate_prekey_fallback`, `invalidate_prekeys`, `refresh_prekeys`, `retry_prekeys`.
    - **Owner-Operated Manual Loopback Qualification**:
      - Verified via two independent terminal processes (`scripts/kami51-wpb-manual/verify.sh --endpoint` executing `transport_tests::native::manual_endpoint`).
      - **Owner-Observed QUIC Exchange (Terminal Evidence)**: Live two-terminal loopback run demonstrated authenticated QUIC exchange: `PREKEY_LAST_ATTEMPT=completed`, `QUERY=durable`, `SIGNED_KIND=OneTime`, `FRESHNESS=valid-at-retention`, `RETENTION_BEFORE_ACK=committed`, `PUBLICATION_ACK_RESULT=exact-result-reconciled`. An initial `Transport(Expired)` failure and subsequent successful reconnect were observed and preserved as authentic qualification evidence.
      - **Owner-Reported Scenarios (Pending Log Archival)**: Subsequent TCP, reverse, and retry command sequences reported successful by owner; awaiting local test log archival.
      - **Antigravity Execution Boundary**: Antigravity did **not** execute, simulate, or fabricate interactive manual tests.
  - **Remaining KAMI-51 Dependencies**: WP-C (durable mutable session recovery & external rollback anchor), WP-D (integrated qualification), KAMI-28 (unimplemented application messaging), and production E2EE / M1 remain incomplete.
- **KAMI-24 (Outbound Sender Coordination & Persistence)**:
  - **Owner-Accepted Bounded Implementation** (`t_3caec9e9`, QA `t_d1b015bb` PASS, Evaluator `t_4c8b8ad4` PASS).
  - Consumes accepted WP3 and WP2-B storage foundations.
  - Implements `Coordinator::enqueue`, `claim`, and `resume` in `kami-sender`.
  - Introduces SQLCipher Migration `004_sender_state.sql` (`sender_binding_key`, `sender_prepare`, `sender_attempt`).
  - Keyed HMAC-SHA256 request binding, lane fencing via partial unique index `sender_prepare_lane`, 30s attempt leases, deterministic exponential backoff (1s base, 5min cap, 8 max attempts), cancellation, and identity holds.
  - **Exact-Frame Crash Recovery**: Re-submitting or recovering an already committed request returns exact stored ciphertext without re-encrypting or advancing ratchets.
  - **Scope Boundary**: Synchronous service layer only. Defers Tokio runtime, timers, and live network transport to downstream issues.
- **KAMI-25 (Inbound Receiver Coordination, Persistence & Receipts)**:
  - **Owner-Accepted Bounded Implementation**.
  - Introduces `crates/kami-receiver` (`Receiver::receive`, `Receiver::receive_first`, `ReceiveContext`, `ReceiveOutcome`, `DeliveryArtifact`).
  - Introduces SQLCipher Migration `005_receiver_state.sql` (`delivery_artifact`, `receiver_gap`).
  - Implements same-session receipt staging (`prepare_delivery` / `inbound_delivery`) advancing ratchet session generations by 2.
  - Outbound receipt consumption implemented in `Coordinator::receive_receipt` (`kami-sender`), advancing sender session generation by 1 and transitioning messages to `DeviceDelivered`.
  - Exact duplicate recovery via `recover_delivery` / `recover_delivery_receipt`.
  - Presentation ordering with 60-second gap aging from unresolved to permanent in `receiver_gap`.
  - **Scope Boundary**: Synchronous service layer only. Defers Tokio runtime, socket I/O, and outbound receipt wire emission to downstream transport issues.
- **KAMI-26 (Supervised Tokio Runtime, Engine Actor & Background Workers)**:
  - **Owner-Accepted Bounded Implementation** sitting at HEAD `ebcfc5907a5de17dad2c6643f86730bd52643daf`.
  - Introduces `crates/kami-runtime` (`Engine`, `EngineHandle`, `Config`, `Status`, `Event`, `RelayAdapter`).
  - Single-owner async `Engine` actor loop managing lifecycle states: `Starting`, `Reconciling`, `Running`, `Draining`, `Stopped`, `Failed`.
  - Bounded channel topology: Command queue (capacity 32, returns `Error::Busy` on overload), Event broadcast (capacity 32, best-effort telemetry, dropped event counter), and dedicated blocking worker (capacity 8).
  - Dedicated single-thread blocking worker (`tokio::task::spawn_blocking` + mpsc) executes all synchronous SQLCipher transactions, key custody, and E2EE ratchet commits; no SQLite transaction or mutable crypto borrow crosses an `.await`.
  - **Profile Lease Retention**: `ProfileLease` held by blocking worker is retained across graceful shutdown timeouts (`ShutdownOutcome::TimedOut`) until active kernel I/O finishes or process termination, preventing split-brain database corruption.
  - **Supervised Relay Seam**: Background child task in `tokio::task::JoinSet` managing reservation renewals via pluggable `RelayAdapter` (returns reservation lifetimes, not delivery).
  - **Private Mock Ingress**: Private `AuthenticatedPeer` token minted only under `#[cfg(test)]`. Production `Engine::enqueue` fails closed with `Error::LiveTransportUnavailable` until KAMI-28 wires live transport provenance into E2EE.
- **KAMI-27 (Authenticated Direct Transport)**:
  - **Owner-Accepted Bounded Implementation** (Linux localhost qualified).
  - Introduces `crates/kami-transport` (`Driver`, `Handle`, `Config`, `AuthenticatedConnection`, `Revocation`).
  - Direct peer-to-peer libp2p QUIC (with TCP/Noise/Yamux fallback) mutual authentication.
  - Expected-peer dial binding and live Peer-ID validation upon `ConnectionEstablished` against fresh `ContactTrust` records (`send_decision(...) == SendDecision::Allow`).
  - Unforgeable, non-cloneable `AuthenticatedConnection` capability model with atomic monotonic revocation and TTL expiry.
  - Supervised within `kami-runtime`'s Tokio `JoinSet`.
  - **Missing Integration Boundary**: Transport connection capability is **not yet connected** to production E2EE sessions or application message payloads (`Engine::enqueue` continues to fail closed). Wire transmission of encrypted application messages and receipt round-trips belongs to **KAMI-28**.
  - **Deferred Scope**: NAT traversal / DCUtR / hole punching, live relay network (KAMI-29/30), automatic path fallback (KAMI-31), cross-network qualification (KAMI-35), Windows/macOS native qualification.
- **Historical Milestone Terminology Reconciliation**:
  - Original **WP4** (runtime and delivery, covering KAMI-24 through KAMI-32) and **WP5** (qualification and evidence, covering KAMI-33 through KAMI-39) are high-level internal planning groups, not Jira issues or work packages.
  - **WP-A**, **WP-B**, **WP-C**, and **WP-D** belong strictly to the **KAMI-51** end-to-end encryption work package sequence. WP-A does not replace WP4 or WP5.
- **Reverse Dependency Pattern (`kami-storage → kami-e2ee`)**: `kami-storage` depends on `kami-e2ee` to consume opaque authenticated commits and delivery artifacts, while `kami-e2ee` never depends on storage. This preserves cryptographic purity and ensures unauthenticated data cannot bypass validation into the database.

### Overall M1 Progress: **Not estimable**
*Milestone 1 acceptance requires two independently verified human CLI endpoints exchanging private text across Linux, Windows, and macOS with direct P2P and genuine automatic relay fallback. While domain, wire protocol, key custody, contact trust, encrypted storage, quarantined E2EE, outbound sender coordinator, inbound receiver coordinator, runtime engine, and direct transport foundations are implemented and pass on Linux localhost, broader platform qualification, real network fallback, live E2EE message wiring (KAMI-28), and relay transport (KAMI-29/30) remain to be done. Subjective percentage estimates are replaced with evidence-based milestone statuses.*

*Whole-project progress: **Not estimable** (broader roadmap items like multi-device synchronization, offline mailbox relays, and agent harness bridges are not yet fully specified).*

---

## Workspace Crates Map

| Crate Note | Primary Purpose | Milestone 1 Status | Test Status |
| :--- | :--- | :--- | :--- |
| [[kami-core]] | Pure domain facts, state machine transitions, and queue limits (no I/O or dependencies). | **100%** of domain scope | Verified (unit tests, 5 proptests, 7 golden vectors, 99.6% coverage) |
| [[kami-protocol]] | Bounded Protobuf encoding/decoding, structural frame validation (up to 64 KB), and prekey exchange framing (16 KiB limit). | **100%** of wire frame scope | Verified (unit tests, vectors, fuzz/proptests, prekey exchange framing tests) |
| [[kami-key-provider]] | Synchronous database key custody contract and memory-zeroizing key type. | **100%** of contract scope | Verified (unit tests, fault injection, restart/loss tests) |
| [[kami-identity]] | OS keyring custody, contact trust (KAMI-50), and bounded per-selection signed Olm prekey bindings (WP-B). | **Not estimable** *(Functionally Implemented)* | Verified on Linux (single and batch Olm binding proofs, custody checks); Windows/macOS compiled & Clippy-checked; native qualification pending |
| [[kami-storage]] | SQLCipher encrypted local database, profile leasing, migrations 001-007, E2EE commits, sender state, receiver state, and prekey lifecycle. | **Not estimable** *(Functionally Implemented & Owner-Accepted Foundations)* | Verified on Linux (migrations 001-007, crash-kill checkpoints, CAS, WAL, sender state, receiver migration, prekey reservation CAS, exact-byte retry reconciliation); native qualification pending |
| [[kami-e2ee]] | Vodozemac Double Ratchet adapter, NativeAadProvider contract, inner envelope, receipt boundaries, and prekey lifecycle staging (WP-B). | **Not estimable** *(WP-A & WP-B Accepted with Limitations)* | Verified on Linux (unit tests, context contracts, qualification suites, prekey lifecycle tests); runtime/network deferred |
| [[kami-sender]] | Synchronous outbound message coordination, sequence reservations, attempt leases, crash recovery, and receipt consumption. | **Not estimable** *(Owner-Accepted Bounded Implementation)* | Verified on Linux (coordinator race tests, exponential retry tests); runtime/network transport deferred |
| [[kami-receiver]] | Synchronous inbound frame coordination, duplicate deduplication, presentation ordering, same-session receipt staging, and delivery artifact persistence. | **Not estimable** *(Owner-Accepted Bounded Implementation)* | Verified on Linux (16 receive tests, failure suites, joint storage tests); runtime/network transport deferred |
| [[kami-runtime]] | Tokio async actor engine, bounded channel backpressure, dedicated blocking worker, profile lease retention, supervised relay seam, and prekey engine orchestration. | **Not estimable** *(Owner-Accepted Bounded Implementation)* | Verified on Linux (21 unit/integration tests, ownership/privacy tests, join/reconcile suites, transport tests); live transport capability supervised |
| [[kami-transport]] | Authenticated libp2p QUIC/TCP direct transport, expected-peer dial binding, fresh contact trust checks, revocable connection capabilities, and `/kami/prekey-exchange/1` subprotocol. | **Not estimable** *(Owner-Accepted Bounded Implementation)* | Verified on Linux (automated direct and prekey suites, compile-fail doctests, owner-operated manual two-process harness); relay/application E2EE deferred |
| [[kami-cli]] | Command-line client and background service daemon. | **Scaffold only** *(15% placeholder)* | Scaffold tests only; IPC and commands pending |

---

## Architecture & Dependency Flow

```mermaid
graph TD
    CLI[crates/kami-cli] --> CORE[crates/kami-core]
    CLI --> PROTO[crates/kami-protocol]
    CLI --> STORAGE[crates/kami-storage]
    CLI --> IDENTITY[crates/kami-identity]
    CLI --> SENDER[crates/kami-sender]
    CLI --> RECEIVER[crates/kami-receiver]
    CLI --> RUNTIME[crates/kami-runtime]

    RUNTIME --> CORE
    RUNTIME --> PROTO
    RUNTIME --> KEYPROV[crates/kami-key-provider]
    RUNTIME --> IDENTITY
    RUNTIME --> STORAGE
    RUNTIME --> E2EE[crates/kami-e2ee]
    RUNTIME --> SENDER
    RUNTIME --> RECEIVER
    RUNTIME --> TRANSPORT[crates/kami-transport]

    TRANSPORT --> IDENTITY

    SENDER --> CORE
    SENDER --> STORAGE
    SENDER --> E2EE
    SENDER --> IDENTITY

    RECEIVER --> CORE
    RECEIVER --> STORAGE
    RECEIVER --> E2EE
    RECEIVER --> IDENTITY

    STORAGE --> CORE
    STORAGE --> KEYPROV
    STORAGE --> E2EE

    E2EE --> CORE
    E2EE --> IDENTITY
    E2EE --> PROTO

    IDENTITY --> KEYPROV
    PROTO --> CORE
```

> [!NOTE]
> `kami-runtime` coordinates the background asynchronous runtime and daemon supervision. It orchestrates `kami-sender`, `kami-receiver`, `kami-storage`, `kami-e2ee`, `kami-identity`, `kami-protocol`, `kami-transport`, and `kami-key-provider` across an async `Engine` actor loop and a dedicated single-thread blocking worker.
>
> **Critical Missing Integration Boundary**: While `kami-runtime` supervises the `kami-transport` driver, the resulting connection capability is **not yet connected** to production E2EE (`kami-e2ee`) or message coordination (`kami-sender`/`kami-receiver`). Production ingress remains fail-closed until KAMI-28 bridges live connection provenance into E2EE.

---

## How to Navigate These Notes

Each crate note is organized into three beginner-friendly sections:
1. **Overview**: Explains the crate's purpose, what real-world problem it solves in Kami, its dependencies, and relevant Jira/roadmap requirements.
2. **Source Code Explanation**: Hierarchical breakdown of every module, struct, enum, and method. Explains what each type represents, why Kami needs it, key fields, and how functions work with their error conditions and security implications.
3. **Completion & Verification**: An evidence-based scorecard showing implemented behavior, verified tests, missing behavior, known limitations, and exact `cargo` commands you can run to test the code yourself.

Explore the individual crate documentation:
- [[kami-core]]: Pure message identity and delivery state machine
- [[kami-protocol]]: Strict Protobuf network frames and prekey exchange framing
- [[kami-key-provider]]: Injected encryption key contracts
- [[kami-identity]]: OS keyrings, device fingerprints, and per-selection signed Olm bindings
- [[kami-storage]]: Encrypted SQLCipher database, profile leases, migrations 001-007, sender state, receiver state, and prekey lifecycle
- [[kami-e2ee]]: Double Ratchet session engine, authenticated envelopes, typed commits, receipt boundaries, and prekey lifecycle staging
- [[kami-sender]]: Outbound message coordination, sequence reservations, attempt leases, crash recovery, and receipt consumption
- [[kami-receiver]]: Inbound frame coordination, duplicate deduplication, presentation ordering, and delivery artifacts
- [[kami-runtime]]: Tokio runtime actor engine, dedicated blocking worker, profile lease retention, supervised relay seam, and prekey engine orchestration
- [[kami-transport]]: Authenticated libp2p QUIC/TCP direct transport, expected-peer dial binding, revocable capabilities, and prekey exchange subprotocol
- [[kami-cli]]: User commands and daemon scaffold

Architecture & Pattern Synthesis:
- [[KAMI Design Patterns and Algorithms]]: Comprehensive architectural inventory of 34 design patterns and algorithms implemented across all 11 Kami crates

---

## Interactive Architecture Visualizations (Archify)

The Kami codebase includes source-backed, standalone interactive Archify architecture visualizations:

| ID | Title | Diagram Type | Scope & Description |
| :--- | :--- | :--- | :--- |
| **KAMI-VIS-002** | [Rust Crate Dependency Graph](file:///home/phu/dev/kami/knowledge/Code%20Analyzing/Visualizations/01-System-Architecture/KAMI-VIS-002/crate-dependencies.html) | Architecture | Complete 11-crate dependency topology, service coordinators, direct transport substrate, Tokio runtime supervisor, reverse storage boundaries, prekey exchange lifecycle, and cargo relationships. |
| **KAMI-VIS-007** | [E2EE Rust Types and Interfaces](file:///home/phu/dev/kami/knowledge/Code%20Analyzing/Visualizations/02-End-to-End-Encryption/KAMI-VIS-007/e2ee-rust-types.html) | Class | QuarantinedSession, AuthenticatedCommit, NativeAadProvider, InnerEnvelopeV1, AccountLifecycle, PrekeyUse, LifecycleCommit, and Vodozemac Olm adapter interfaces (KAMI-51 WP-A & WP-B). |
| **KAMI-VIS-014** | [Authenticated E2EE Persistence](file:///home/phu/dev/kami/knowledge/Code%20Analyzing/Visualizations/03-Storage-And-Persistence/KAMI-VIS-014/authenticated-persistence.html) | Sequence | In-process authenticated staging, SQLCipher CAS verification, and exact-frame retry invariants. |
| **KAMI-VIS-018** | [SQLCipher Database Schema](file:///home/phu/dev/kami/knowledge/Code%20Analyzing/Visualizations/03-Storage-And-Persistence/KAMI-VIS-018/sqlcipher-schema.html) | ERD | Physical database schema across migrations 001-007, strict foreign keys, sender attempts, receiver artifacts, and prekey lifecycle tables. |
| **KAMI-VIS-022** | [Message Delivery State Machine](file:///home/phu/dev/kami/knowledge/Code%20Analyzing/Visualizations/04-Core-Domain/KAMI-VIS-022/message-delivery-lifecycle.html) | Lifecycle | Pure domain delivery state transitions, receipt reducer, and terminal failure projections in `kami-core`. |
| **KAMI-VIS-025** | [End-to-End Messaging Lifecycle & Receipt Flow](file:///home/phu/dev/kami/knowledge/Code%20Analyzing/Visualizations/01-System-Architecture/KAMI-VIS-025/messaging-lifecycle.html) | Sequence | Runtime-neutral sender preparation, inbound validation, same-session receipt staging, and delivery receipt round-trip. |
| **KAMI-VIS-026** | [Receiver Inbound Reliability & Recovery Flow](file:///home/phu/dev/kami/knowledge/Code%20Analyzing/Visualizations/01-System-Architecture/KAMI-VIS-026/receiver-reliability.html) | Workflow | Inbound ingress, ReplayPermit deduplication, conflict detection, same-session staging, gap ordering, and crash recovery. |
| **KAMI-VIS-027** | [Supervised Tokio Runtime & Task Ownership](file:///home/phu/dev/kami/knowledge/Code%20Analyzing/Visualizations/01-System-Architecture/KAMI-VIS-027/runtime-supervision.html) | Architecture | Supervised Tokio event loop, single-owner Engine actor, dedicated blocking worker, direct transport driver, prekey exchange stream tasks, ProfileLease flock ownership, and bounded channel backpressure. |

---

## Analysis Baseline & Working Tree Manifest

- **Git Branch**: `protocol-storage`
- **Git Commit (HEAD)**: `438d466` ("feat(kami): complete KAMI-51 WP-A authenticated context boundary")
- **Working Tree State**: Accepted KAMI-51 WP-B implementation sitting on top of the `438d466` baseline
- **Analysis Timestamp**: `2026-11-11T22:45:00Z`
- **Toolchain**: Rust 2021 Edition / Cargo workspace

### Working Tree Source Manifest (Uncommitted & Untracked Baseline)
To ensure incremental re-analysis reliably detects modifications even when `HEAD` commit remains unchanged, the following SHA-256 hashes record the exact state of all analyzed working tree files:

| File Path | Working Tree Status | SHA-256 Digest |
| :--- | :--- | :--- |
| `Cargo.lock` | Tracked (Modified) | `cfcc7578ba1549c838d019556095e924ce0412f1821a439a9de12d89b9ca2536` |
| `README.md` | Tracked (Modified) | `5e086dc048bb3322b6632490b99f8904663d42c16f213bea7617c0a66418b311` |
| `crates/kami-e2ee/README.md` | Tracked (Modified) | `38aa6c765aa732d14b718bb8dbed8b66ab6a8bef1789004609f07793d0cfe31d` |
| `crates/kami-e2ee/src/authenticated.rs` | Tracked (Modified) | `15ace1406afb4cf072392d3f56ed8abd9c20a01c30123b81be23cff789b5b747` |
| `crates/kami-e2ee/src/lib.rs` | Tracked (Modified) | `3319afb43a4459b8aaf4317a9f6359655a7eca7858c7bf6d4e8f8968bac88a6c` |
| `crates/kami-e2ee/src/persistence.rs` | Tracked (Modified) | `8d63ef7f7e19b8feed67c9022577a1b4f1231dbb7cb8d3a0fbb2505c4323c7fd` |
| `crates/kami-e2ee/src/prekeys.rs` | Untracked | `ddad86def783c83a615338ae5d219e02aa6dd9dfdda8a3bf1ccde4d206358f1a` |
| `crates/kami-e2ee/tests/prekeys.rs` | Untracked | `1a3bac31dd0d72782c81c22c480a1e91d61f1ea970d67e55534134f45c68fabe` |
| `crates/kami-e2ee/tests/privacy.rs` | Tracked (Modified) | `8e75ff7a92af6c24bac8f696b6aa5c24be3e73055ba1972c84544000c7b3292c` |
| `crates/kami-e2ee/tests/qualification/authenticated.rs` | Tracked (Modified) | `b50df233eb14579b6220989325647d9ef5d10d67c858dcdbeb58dec267366aa1` |
| `crates/kami-identity/README.md` | Tracked (Modified) | `85fb2db861d560496062574b6564a51beb816debdf90270e1dd0f47539ef0446` |
| `crates/kami-identity/src/custody.rs` | Tracked (Modified) | `8a925905de665adc1299e6ed8ec76732f2beea1ab6262e2f6d8a335e127b335e` |
| `crates/kami-identity/src/olm_binding.rs` | Tracked (Modified) | `08f8bd78016405f92968c18e5863703e845d17e1e7ef81d6c0d7d1aa676617fa` |
| `crates/kami-protocol/README.md` | Tracked (Modified) | `4655307ffcddc32665794a5eeed3471350fcd908aa2576a837dec348c39fc6f6` |
| `crates/kami-protocol/proto/kami/m1/prekey_exchange.proto` | Untracked | `1d95145fea71f8c27f6f9b4cf82886c69a28bb7d62c169df2130fca6d1b12054` |
| `crates/kami-protocol/src/lib.rs` | Tracked (Modified) | `552ddae132113937b9a2746bc5f55b914603160a78babe8f11bde44b22c2bdee` |
| `crates/kami-protocol/src/prekey_exchange.rs` | Untracked | `af41159ad12c516353e21e663ec234dfcd4ed143cbabcd18ef70ac28f6424308` |
| `crates/kami-protocol/tests/prekey_exchange.rs` | Untracked | `60126f55ad7ed745e1fd9da372f51404d7add437d799303a8348156db9e3cd4b` |
| `crates/kami-runtime/README.md` | Tracked (Modified) | `9c024bcda35a6a55a50d990c2784415eeae6d11361553a512a832bd584c2d0a7` |
| `crates/kami-runtime/src/engine.rs` | Tracked (Modified) | `cb4944c1ad95c51f8515c6b179d5bdd32a985a6690e73da398fd82d140c0e121` |
| `crates/kami-runtime/src/transport_tests.rs` | Tracked (Modified) | `7082bbdf40f3f5b7697405e82ab2ef7b6b6e44021f8a77e14404f08e53655589` |
| `crates/kami-runtime/src/worker.rs` | Tracked (Modified) | `f2a3ac761dd3f4a2fd0cfd66ab4ff4d86e59793ab983c62c7cde3c7d49f4c83b` |
| `crates/kami-storage/README.md` | Tracked (Modified) | `744f05dfab0014caa2ef99af762e8fae56112fd404d77a6d8ad81f9d38220e82` |
| `crates/kami-storage/migrations/007_prekey_lifecycle.sql` | Untracked | `a6eab465cd1b4de71b905c76a6d818dfca5ac4e3f6fe8e038b38cd5c0aa9bc05` |
| `crates/kami-storage/src/lib.rs` | Tracked (Modified) | `2470ae6c0bf470f4aaaacfb3c1dc9850b0b9b4e814106811f13e49563a599b1f` |
| `crates/kami-storage/src/migrations.rs` | Tracked (Modified) | `f37bff14e17ef4f33cf8178f41c489ee99fd1c2104da8d4a044dbacf02adff5f` |
| `crates/kami-storage/src/prekeys.rs` | Untracked | `48253e26e19cee7ebf16e92a094350679e178278a232d85a67785fb179bbe51f` |
| `crates/kami-storage/src/session_state.rs` | Tracked (Modified) | `05f8527179eb42cce70b240b63282175819fb3345841d6e4c376445ac6c12bd8` |
| `crates/kami-storage/src/worker.rs` | Tracked (Modified) | `7395a2e33237bfc920dcdec90c62c86b5ccc4c8724fb8ddd347ac3b7825a32a1` |
| `crates/kami-storage/tests/authenticated_boundary.rs` | Tracked (Modified) | `b31bd56f021335dc6f04756f93d8db4e374c6e484b8dcccf4eebe25cfe0fc0e6` |
| `crates/kami-storage/tests/contact_trust.rs` | Tracked (Modified) | `9af97fa074bed3a6fe55e025d1d9428144416492cca066539b46b77c6f902699` |
| `crates/kami-storage/tests/e2ee_state.rs` | Tracked (Modified) | `07c46f33d98acb4578deac460a846b8aeeb118d6c8dabf143379431219ff0996` |
| `crates/kami-storage/tests/prekeys.rs` | Untracked | `dccec6404269e7a158e7edbe8bc3eeefbf14d8ba2cc9e00526e7b3fc515a99a7` |
| `crates/kami-storage/tests/receiver_migration.rs` | Tracked (Modified) | `2d50595ca3cab50ece207f7db6de7791365ef766014cb82333fed3cb2d8004c0` |
| `crates/kami-storage/tests/sender_state.rs` | Tracked (Modified) | `d33f59ad4674fd9c364e725cc74623b6f863072e98e5cc84e17fab3596986148` |
| `crates/kami-transport/Cargo.toml` | Tracked (Modified) | `b1f7d7bdfe32eb0260764ff18a350d08e3af8904efb1b4316c55c921da05da4d` |
| `crates/kami-transport/README.md` | Tracked (Modified) | `b6f6aeb98a0dcae3dc5a681d0fd4ea9b991968f6747c236e3f117b74b69ec141` |
| `crates/kami-transport/src/connection.rs` | Tracked (Modified) | `4790822b95e883d5565612ca18c81eea546c2ea7192b10bd3e8f420f9465afc6` |
| `crates/kami-transport/src/driver.rs` | Tracked (Modified) | `2fa4855176108c945ffda0f2dfd96602206fa2b4a2cd15421c2fbd8cfe891761` |
| `crates/kami-transport/src/lib.rs` | Tracked (Modified) | `8bc53550454a703876cac113f079584b5e721b41931d7bd988488c9f37dfee6c` |
| `crates/kami-transport/src/prekey_exchange.rs` | Untracked | `7105f011cc598a1d8eaa516d2d5a33af32ea40badf3f0934123e81eedc62186f` |
| `crates/kami-transport/tests/direct.rs` | Tracked (Modified) | `c938447e22415029352409d6dbb1f304bba40ffefdd7f18b5ab151e257083069` |
| `docs/KAMI_DEVELOPMENT_ROADMAP.md` | Tracked (Modified) | `f3b4c58cafbed39a307c0fe5eca33e45b35e020f605109953659acd29688da04` |
| `scripts/kami51-wpb-manual/README.md` | Untracked | `a00302b6e7df61412d48a77e01aeffd1dd92cda73253e400f34ffd935bb4a10b` |
| `scripts/kami51-wpb-manual/verify.sh` | Untracked | `b32c4674b86da200ec82618daab293cfdc7290f7bb69908a5f742c393561b802` |
