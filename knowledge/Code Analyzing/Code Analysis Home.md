---
last_analyzed_branch: "protocol-storage"
last_analyzed_commit: "ebcfc5907a5de17dad2c6643f86730bd52643daf"
last_analyzed_timestamp: "2026-11-10T21:35:00Z"
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
  - **Remaining KAMI-51 Dependencies**: WP-B (production prekey lifecycle), WP-C (durable mutable session recovery & rollback anchor), WP-D (integrated qualification).
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
| [[kami-protocol]] | Bounded Protobuf encoding/decoding and structural frame validation (up to 64 KB). | **100%** of wire frame scope | Verified (unit tests, vectors, fuzz/proptests, memory probe) |
| [[kami-key-provider]] | Synchronous database key custody contract and memory-zeroizing key type. | **100%** of contract scope | Verified (unit tests, fault injection, restart/loss tests) |
| [[kami-identity]] | OS keyring custody, contact trust (KAMI-50), and signed Olm prekey bindings (WP2-A1). | **Not estimable** *(Functionally Implemented)* | Verified on Linux; Windows/macOS compiled & Clippy-checked; native qualification pending |
| [[kami-storage]] | SQLCipher encrypted local database, profile leasing, migrations 001-005, E2EE commits, sender state, and receiver state. | **Not estimable** *(Functionally Implemented & Owner-Accepted Foundations)* | Verified on Linux (migrations 001-005, crash-kill checkpoints, CAS, WAL, sender state, receiver migration); native qualification pending |
| [[kami-e2ee]] | Vodozemac Double Ratchet adapter, NativeAadProvider contract, inner envelope, and receipt boundaries. | **Not estimable** *(WP-A Accepted with Limitations)* | Verified on Linux (unit tests, context contracts, qualification suites); runtime/network deferred |
| [[kami-sender]] | Synchronous outbound message coordination, sequence reservations, attempt leases, crash recovery, and receipt consumption. | **Not estimable** *(Owner-Accepted Bounded Implementation)* | Verified on Linux (coordinator race tests, exponential retry tests); runtime/network transport deferred |
| [[kami-receiver]] | Synchronous inbound frame coordination, duplicate deduplication, presentation ordering, same-session receipt staging, and delivery artifact persistence. | **Not estimable** *(Owner-Accepted Bounded Implementation)* | Verified on Linux (16 receive tests, failure suites, joint storage tests); runtime/network transport deferred |
| [[kami-runtime]] | Tokio async actor engine, bounded channel backpressure, dedicated blocking worker, profile lease retention, and supervised relay seam. | **Not estimable** *(Owner-Accepted Bounded Implementation)* | Verified on Linux (21 unit/integration tests, ownership/privacy tests, join/reconcile suites); live transport capability supervised |
| [[kami-transport]] | Authenticated libp2p QUIC/TCP direct transport, expected-peer dial binding, fresh contact trust checks, and revocable connection capabilities. | **Not estimable** *(Owner-Accepted Bounded Implementation)* | Verified on Linux (automated direct suite, compile-fail doctests, owner-operated manual two-process harness); relay/E2EE deferred |
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
- [[kami-protocol]]: Strict Protobuf network frames
- [[kami-key-provider]]: Injected encryption key contracts
- [[kami-identity]]: OS keyrings, device fingerprints, and Olm bindings
- [[kami-storage]]: Encrypted SQLCipher database, profile leases, migrations 001-005, sender state, and receiver state
- [[kami-e2ee]]: Double Ratchet session engine, authenticated envelopes, typed commits, and receipt boundaries
- [[kami-sender]]: Outbound message coordination, sequence reservations, attempt leases, crash recovery, and receipt consumption
- [[kami-receiver]]: Inbound frame coordination, duplicate deduplication, presentation ordering, and delivery artifacts
- [[kami-runtime]]: Tokio runtime actor engine, dedicated blocking worker, profile lease retention, and supervised relay seam
- [[kami-transport]]: Authenticated libp2p QUIC/TCP direct transport, expected-peer dial binding, and revocable capabilities
- [[kami-cli]]: User commands and daemon scaffold

Architecture & Pattern Synthesis:
- [[KAMI Design Patterns and Algorithms]]: Comprehensive architectural inventory of 31 design patterns and algorithms implemented across all 11 Kami crates

---

## Interactive Architecture Visualizations (Archify)

The Kami codebase includes source-backed, standalone interactive Archify architecture visualizations:

| ID | Title | Diagram Type | Scope & Description |
| :--- | :--- | :--- | :--- |
| **KAMI-VIS-002** | [Rust Crate Dependency Graph](file:///home/phu/dev/kami/knowledge/Code%20Analyzing/Visualizations/01-System-Architecture/KAMI-VIS-002/crate-dependencies.html) | Architecture | Complete 11-crate dependency topology, service coordinators, direct transport substrate, Tokio runtime supervisor, reverse storage boundaries, and cargo relationships. |
| **KAMI-VIS-007** | [E2EE Rust Types and Interfaces](file:///home/phu/dev/kami/knowledge/Code%20Analyzing/Visualizations/02-End-to-End-Encryption/KAMI-VIS-007/e2ee-rust-types.html) | Class | QuarantinedSession, AuthenticatedCommit, NativeAadProvider, InnerEnvelopeV1, and Vodozemac Olm adapter interfaces (KAMI-51 WP-A). |
| **KAMI-VIS-014** | [Authenticated E2EE Persistence](file:///home/phu/dev/kami/knowledge/Code%20Analyzing/Visualizations/03-Storage-And-Persistence/KAMI-VIS-014/authenticated-persistence.html) | Sequence | In-process authenticated staging, SQLCipher CAS verification, and exact-frame retry invariants. |
| **KAMI-VIS-018** | [SQLCipher Database Schema](file:///home/phu/dev/kami/knowledge/Code%20Analyzing/Visualizations/03-Storage-And-Persistence/KAMI-VIS-018/sqlcipher-schema.html) | ERD | Physical database schema across migrations 001-005, strict foreign keys, sender attempts, and receiver artifacts. |
| **KAMI-VIS-022** | [Message Delivery State Machine](file:///home/phu/dev/kami/knowledge/Code%20Analyzing/Visualizations/04-Core-Domain/KAMI-VIS-022/message-delivery-lifecycle.html) | Lifecycle | Pure domain delivery state transitions, receipt reducer, and terminal failure projections in `kami-core`. |
| **KAMI-VIS-025** | [End-to-End Messaging Lifecycle & Receipt Flow](file:///home/phu/dev/kami/knowledge/Code%20Analyzing/Visualizations/01-System-Architecture/KAMI-VIS-025/messaging-lifecycle.html) | Sequence | Runtime-neutral sender preparation, inbound validation, same-session receipt staging, and delivery receipt round-trip. |
| **KAMI-VIS-026** | [Receiver Inbound Reliability & Recovery Flow](file:///home/phu/dev/kami/knowledge/Code%20Analyzing/Visualizations/01-System-Architecture/KAMI-VIS-026/receiver-reliability.html) | Workflow | Inbound ingress, ReplayPermit deduplication, conflict detection, same-session staging, gap ordering, and crash recovery. |
| **KAMI-VIS-027** | [Supervised Tokio Runtime & Task Ownership](file:///home/phu/dev/kami/knowledge/Code%20Analyzing/Visualizations/01-System-Architecture/KAMI-VIS-027/runtime-supervision.html) | Architecture | Supervised Tokio event loop, single-owner Engine actor, dedicated blocking worker, direct transport driver, ProfileLease flock ownership, and bounded channel backpressure. |

---

## Analysis Baseline & Working Tree Manifest

- **Git Branch**: `protocol-storage`
- **Git Commit (HEAD)**: `ebcfc5907a5de17dad2c6643f86730bd52643daf` ("feat(kami): complete KAMI-27 authenticated direct transport")
- **Working Tree State**: Accepted KAMI-51 WP-A implementation sitting on top of the HEAD baseline
- **Analysis Timestamp**: `2026-11-10T21:35:00Z`
- **Toolchain**: Rust 2021 Edition / Cargo workspace

### Working Tree Source Manifest (Uncommitted & Untracked Baseline)
To ensure incremental re-analysis reliably detects modifications even when `HEAD` commit remains unchanged, the following SHA-256 hashes record the exact state of all analyzed working tree files:

| File Path | Working Tree Status | SHA-256 Digest |
| :--- | :--- | :--- |
| `README.md` | Tracked (Modified) | `407627350fcfde500db7e650e02ad5a02ce5ed578358a9b5a96a8d02bbc6f893` |
| `crates/kami-e2ee/README.md` | Tracked (Modified) | `de7710f745ed22e6e30c901dadabeac15f73bb9fe83ed3b5a1833d92239e3b2f` |
| `crates/kami-e2ee/src/authenticated.rs` | Tracked (Modified) | `77b12d687ac4dc7bf950fa374ed8cd43235eaaa0e287e04b69f2a19a6b08c2ad` |
| `crates/kami-e2ee/src/lib.rs` | Tracked (Modified) | `2b1bcdee5bce8e3f21e6e3f223fa3d973d3695fedf01cf68cb5e64427cee7291` |
| `crates/kami-e2ee/src/provider.rs` | Tracked (Modified) | `3e9c970ca65073b0919222870157b637b7fb3704fe1b99f88623b2479747b23c` |
| `crates/kami-e2ee/src/vodozemac.rs` | Tracked (Modified) | `665f765be7bcbce9369aaed089384ac5515aeed1292229ef56d02c592abed278` |
| `crates/kami-e2ee/tests/privacy.rs` | Tracked (Modified) | `2e65cc14de8ef71e5c4fda551b5cf2bb08749127d602de2adcab2801448edaa1` |
| `crates/kami-e2ee/tests/provider_contract.rs` | Tracked (Modified) | `5ac00caf3be0bf22263a021ddbcfdd5b96732f40d4ac1fa8e98258ec6df0c1fd` |
| `crates/kami-e2ee/tests/qualification/authenticated.rs` | Tracked (Modified) | `0614eb6d5e9a27b77553cae250f09d7e46c17c61a0801c192af673bd36eda60e` |
| `crates/kami-e2ee/tests/qualification/context_contract.rs` | Untracked | `836fdacecd43104d1b83e6c71bd1c4cfb57393cd2ede70a058d6c7eb847a0636` |
| `docs/KAMI_DEVELOPMENT_ROADMAP.md` | Tracked (Modified) | `24b1e608bad49ce83e27384c2cf0b6cd13048cdcbc1d388e9fd1e4ea3e53c9de` |
