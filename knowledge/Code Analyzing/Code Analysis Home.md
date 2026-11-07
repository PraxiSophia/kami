---
last_analyzed_branch: "protocol-storage"
last_analyzed_commit: "0de16c9a395e07aa28a30b1c35e492e90cbbe108"
last_analyzed_timestamp: "2026-11-06T23:05:00Z"
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

Milestone 1 focuses on nine core foundations:
1. **Domain Core**: Pure data structures, message identity, deduplication logic, and delivery state tracking (`kami-core`).
2. **Wire Protocol**: Strict, bounded, zero-allocation-on-bad-input Protobuf serialization (`kami-protocol`).
3. **Key Custody & Contact Trust**: OS keyring integration (Linux Secret Service, macOS Keychain, Windows Credential Manager), bilateral fingerprint verification, and signed Olm prekey bindings (`kami-identity`, `kami-key-provider`).
4. **End-to-End Encryption**: Matrix Vodozemac Olm v1 Double Ratchet engine, canonical inner envelopes, staged authenticated commits, and receipt boundaries (`kami-e2ee`).
5. **Encrypted Storage Engine**: SQLCipher-encrypted SQLite database with profile filesystem leasing, outbox retries, inbox deduplication, contact trust CAS, E2EE session persistence, outbound sender state, and inbound receiver state (`kami-storage`).
6. **Outbound Sender Coordinator**: Synchronous, runtime-neutral message coordination, sequence reservations, 30s attempt leases, exponential backoff, exact-frame crash recovery, and delivery receipt consumption (`kami-sender`).
7. **Inbound Receiver Coordinator**: Synchronous, runtime-neutral inbound frame coordination, duplicate deduplication, presentation ordering, same-session receipt staging, and delivery artifact persistence (`kami-receiver`).
8. **Asynchronous Daemon Supervisor & Runtime**: Tokio multi-threaded async actor loop, bounded channels, dedicated single-thread blocking worker, profile filesystem lease retention across timeouts, pluggable relay reservation scheduling, and private mock ingress boundary (`kami-runtime`).
9. **Background Service & CLI**: The client interface and background daemon (`kami-cli`, currently in scaffolding phase).

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
  - **Owner-Accepted Bounded Implementation** sitting in uncommitted working tree over baseline `0de16c9`.
  - Introduces `crates/kami-runtime` (`Engine`, `EngineHandle`, `Config`, `Status`, `Event`, `RelayAdapter`).
  - Implements single-owner async `Engine` actor loop managing lifecycle states: `Starting`, `Reconciling`, `Running`, `Draining`, `Stopped`, `Failed`.
  - Bounded channel topology: Command queue (capacity 32, returns `Error::Busy` on overload), Event broadcast (capacity 32, best-effort telemetry, dropped event counter), and dedicated blocking worker (capacity 8).
  - Dedicated single-thread blocking worker (`tokio::task::spawn_blocking` + mpsc) executes all synchronous SQLCipher transactions, key custody, and E2EE ratchet commits; no SQLite transaction or mutable crypto borrow crosses an `.await`.
  - **Profile Lease Retention**: `ProfileLease` held by blocking worker is retained across graceful shutdown timeouts (`ShutdownOutcome::TimedOut`) until active kernel I/O finishes or process termination, preventing split-brain database corruption.
  - **Supervised Relay Seam**: Background child task in `tokio::task::JoinSet` managing reservation renewals via pluggable `RelayAdapter` (returns reservation lifetimes, not delivery).
  - **Private Mock Ingress**: Private `AuthenticatedPeer` token minted only under `#[cfg(test)]`. Production `Engine::enqueue` fails closed with `Error::LiveTransportUnavailable` until KAMI-27 live transport connects.
- **Reverse Dependency Pattern (`kami-storage → kami-e2ee`)**: `kami-storage` depends on `kami-e2ee` to consume opaque authenticated commits and delivery artifacts, while `kami-e2ee` never depends on storage. This preserves cryptographic purity and ensures unauthenticated data cannot bypass validation into the database.

### Overall M1 Progress: **Not estimable**
*Milestone 1 acceptance requires two independently verified human CLI endpoints exchanging private text across Linux, Windows, and macOS with direct P2P and genuine automatic relay fallback. While domain, wire protocol, key custody, contact trust, encrypted storage, quarantined E2EE, outbound sender coordinator, inbound receiver coordinator, and runtime engine foundations are implemented and pass on Linux, broader platform qualification, real network fallback, and P2P transport (KAMI-27) remain to be done. Subjective percentage estimates are replaced with evidence-based milestone statuses.*

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
| [[kami-e2ee]] | Vodozemac Double Ratchet adapter, inner envelope, typed storage commits, and receipt boundaries. | **Not estimable** *(Quarantined Foundation Accepted with Conditions)* | Verified on Linux (unit tests, privacy probe, qualification suites, receipts tests); runtime/network deferred |
| [[kami-sender]] | Synchronous outbound message coordination, sequence reservations, attempt leases, crash recovery, and receipt consumption. | **Not estimable** *(Owner-Accepted Bounded Implementation)* | Verified on Linux (coordinator race tests, exponential retry tests); runtime/network transport deferred |
| [[kami-receiver]] | Synchronous inbound frame coordination, duplicate deduplication, presentation ordering, same-session receipt staging, and delivery artifact persistence. | **Not estimable** *(Owner-Accepted Bounded Implementation)* | Verified on Linux (16 receive tests, failure suites, joint storage tests); runtime/network transport deferred |
| [[kami-runtime]] | Tokio async actor engine, bounded channel backpressure, dedicated blocking worker, profile lease retention, and supervised relay seam. | **Not estimable** *(Owner-Accepted Bounded Implementation)* | Verified on Linux (21 unit/integration tests, ownership/privacy tests, join/reconcile suites); live transport deferred |
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
> `kami-runtime` coordinates the background asynchronous runtime and daemon supervision. It orchestrates `kami-sender`, `kami-receiver`, `kami-storage`, `kami-e2ee`, `kami-identity`, `kami-protocol`, and `kami-key-provider` across an async `Engine` actor loop and a dedicated single-thread blocking worker. `kami-storage` retains its reverse dependency on `kami-e2ee` to consume `AuthenticatedCommit`, `AuthenticatedDelivery`, and `AuthenticatedReceiptCommit`, guaranteeing that only cryptographically verified frames reach disk persistence.

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
- [[kami-cli]]: User commands and daemon scaffold

Architecture & Pattern Synthesis:
- [[KAMI Design Patterns and Algorithms]]: Comprehensive architectural inventory of 27 design patterns and algorithms implemented across all 10 Kami crates

---

## Interactive Architecture Visualizations (Archify)

The Kami codebase includes source-backed, standalone interactive Archify architecture visualizations:

| ID | Title | Diagram Type | Scope & Description |
| :--- | :--- | :--- | :--- |
| **KAMI-VIS-002** | [Rust Crate Dependency Graph](file:///home/phu/dev/kami/knowledge/Code%20Analyzing/Visualizations/01-System-Architecture/KAMI-VIS-002/crate-dependencies.html) | Architecture | Complete 10-crate dependency topology, service coordinators, Tokio runtime supervisor, reverse storage boundaries, and cargo relationships. |
| **KAMI-VIS-007** | [E2EE Rust Types and Interfaces](file:///home/phu/dev/kami/knowledge/Code%20Analyzing/Visualizations/02-End-to-End-Encryption/KAMI-VIS-007/e2ee-rust-types.html) | Class | QuarantinedSession, AuthenticatedCommit, Endpoint bindings, and Vodozemac Olm adapter interfaces. |
| **KAMI-VIS-014** | [Authenticated E2EE Persistence](file:///home/phu/dev/kami/knowledge/Code%20Analyzing/Visualizations/03-Storage-And-Persistence/KAMI-VIS-014/authenticated-persistence.html) | Sequence | In-process authenticated staging, SQLCipher CAS verification, and exact-frame retry invariants. |
| **KAMI-VIS-018** | [SQLCipher Database Schema](file:///home/phu/dev/kami/knowledge/Code%20Analyzing/Visualizations/03-Storage-And-Persistence/KAMI-VIS-018/sqlcipher-schema.html) | ERD | Physical database schema across migrations 001-005, strict foreign keys, sender attempts, and receiver artifacts. |
| **KAMI-VIS-022** | [Message Delivery State Machine](file:///home/phu/dev/kami/knowledge/Code%20Analyzing/Visualizations/04-Core-Domain/KAMI-VIS-022/message-delivery-lifecycle.html) | Lifecycle | Pure domain delivery state transitions, receipt reducer, and terminal failure projections in `kami-core`. |
| **KAMI-VIS-025** | [End-to-End Messaging Lifecycle & Receipt Flow](file:///home/phu/dev/kami/knowledge/Code%20Analyzing/Visualizations/01-System-Architecture/KAMI-VIS-025/messaging-lifecycle.html) | Sequence | Runtime-neutral sender preparation, inbound validation, same-session receipt staging, and delivery receipt round-trip. |
| **KAMI-VIS-026** | [Receiver Inbound Reliability & Recovery Flow](file:///home/phu/dev/kami/knowledge/Code%20Analyzing/Visualizations/01-System-Architecture/KAMI-VIS-026/receiver-reliability.html) | Workflow | Inbound ingress, ReplayPermit deduplication, conflict detection, same-session staging, gap ordering, and crash recovery. |
| **KAMI-VIS-027** | [Supervised Tokio Runtime & Task Ownership](file:///home/phu/dev/kami/knowledge/Code%20Analyzing/Visualizations/01-System-Architecture/KAMI-VIS-027/runtime-supervision.html) | Architecture | Supervised Tokio event loop, single-owner Engine actor, dedicated blocking worker, ProfileLease flock ownership, and bounded channel backpressure. |

---

## Analysis Baseline & Working Tree Manifest

- **Git Branch**: `protocol-storage`
- **Git Commit (HEAD)**: `0de16c9a395e07aa28a30b1c35e492e90cbbe108` ("Merge KAMI-25 into main")
- **Working Tree State**: Uncommitted & untracked KAMI-26 implementation sitting on top of the HEAD baseline
- **Analysis Timestamp**: `2026-11-06T23:05:00Z`
- **Toolchain**: Rust 2021 Edition / Cargo workspace

### Working Tree Source Manifest (Uncommitted & Untracked Baseline)
To ensure incremental re-analysis reliably detects modifications even when `HEAD` commit remains unchanged, the following SHA-256 hashes record the exact state of all analyzed working tree files:

| File Path | Working Tree Status | SHA-256 Digest |
| :--- | :--- | :--- |
| `Cargo.toml` | Tracked (Modified) | `012bbeac5c8eedecbcafd6bec4c18fe1dddc8c25d5e657c165cd6bd48446aba2` |
| `Cargo.lock` | Tracked (Modified) | `0a6ed1d289df1e30ca2b1c7754fa210c96f2bfb8b5c9ec0d97a55121404509e5` |
| `README.md` | Tracked (Modified) | `82444d146ad67fd10c627c788e40a857986d13680bfedc604ea87f8c88699cd9` |
| `docs/KAMI_DEVELOPMENT_ROADMAP.md` | Tracked (Modified) | `35146acdb923c475a02dc09aa759a76bf94652733241bda6dd5d2c60342cb744` |
| `crates/kami-receiver/README.md` | Tracked (Modified) | `0c6338a4a4c821bbc4eb309e51cf7a51fe85cc15a7c28d6dd1077084ff2e4eab` |
| `crates/kami-sender/README.md` | Tracked (Modified) | `e34e86a4863faec05414cfb2489ee0642ff7c4c2caac25589b64c5f8c382b7d8` |
| `crates/kami-storage/README.md` | Tracked (Modified) | `3459c4369da8cc369d09b68d24b32614c6352e98773cd968e4326a37d92a93aa` |
| `crates/kami-storage/src/lease.rs` | Tracked (Modified) | `0b6e74aeb661f7657cfdaa8ff5f1d6e5c40664616168970252709bd55b427313` |
| `crates/kami-storage/src/sender_state.rs` | Tracked (Modified) | `434268a36649357fadfe278c1df69bc5f627787264dce18206a0657968a5d115` |
| `crates/kami-storage/tests/authenticated_boundary.rs` | Tracked (Modified) | `d26f16053754fc000655dbf202eb0a5cabef4a0acd6b65997417979a69903e70` |
| `crates/kami-runtime/Cargo.toml` | Untracked | `336800f0b5db2033d999c46b7a9fb59a397a6620a9c723259f07778682138c43` |
| `crates/kami-runtime/README.md` | Untracked | `26a454e1da7ab551ddba599dc78d0ac7f3590950eb5d6312c825751ddc51eb03` |
| `crates/kami-runtime/src/engine.rs` | Untracked | `05805a13dca6f3c6a219020222b1a1462dda3fdead8c6a4cc54417b2d76cdeb4` |
| `crates/kami-runtime/src/ingress.rs` | Untracked | `e1dcaa979e52bfdc158669b84f0e74093893671591329457ba9fc9d6351d4e90` |
| `crates/kami-runtime/src/integration_tests.rs` | Untracked | `02afd18968b90f8b5755a45cb9b2a46350c6258b602477a78e6072a6c1d019b3` |
| `crates/kami-runtime/src/lib.rs` | Untracked | `ea680f30b36d8561199b36215228abf10d6a414c41117b02f0f9fdbd340deab7` |
| `crates/kami-runtime/src/relay.rs` | Untracked | `393c6d638b131e80a78c60a8047e9a7000f4fc9961da34ea1e1d7d2e3c2ffaac` |
| `crates/kami-runtime/src/relay/tests.rs` | Untracked | `fb1fd64f8371d00f525c0376aa00b5dc5bcc4341931f016bb89e8e7f3d5c8c15` |
| `crates/kami-runtime/src/tests.rs` | Untracked | `c090aee56ef195405c1ef5f5c793d87bdb55c4b911f265a1d36b8165e3e03f23` |
| `crates/kami-runtime/src/worker.rs` | Untracked | `06812f3b31d1e2adaec9ed37c41b37943939e77f2da98fc99e1c8d5bc28c4825` |
| `crates/kami-runtime/tests/ownership.rs` | Untracked | `13908dc7d305268b04055c823d80bec4f6b2646c35f5c2112c73fedca2636184` |
| `crates/kami-runtime/tests/privacy.rs` | Untracked | `b251c31c38b116fc1d69141b7fa98e210e6d04d0a661133010746ad33a0d4f7f` |
| `crates/kami-runtime/tests/runtime.rs` | Untracked | `fe8c3d225fdb2619068ac27c35d176cf98844b2452e74e560329a1f25b9d2f49` |
