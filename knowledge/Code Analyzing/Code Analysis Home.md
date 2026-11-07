---
last_analyzed_branch: "protocol-storage"
last_analyzed_commit: "7bffc6fe8523161e2af3fd8d87b6fb3aadb92bc8"
last_analyzed_timestamp: "2026-11-06T18:38:00Z"
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

Milestone 1 focuses on eight core foundations:
1. **Domain Core**: Pure data structures, message identity, deduplication logic, and delivery state tracking (`kami-core`).
2. **Wire Protocol**: Strict, bounded, zero-allocation-on-bad-input Protobuf serialization (`kami-protocol`).
3. **Key Custody & Contact Trust**: OS keyring integration (Linux Secret Service, macOS Keychain, Windows Credential Manager), bilateral fingerprint verification, and signed Olm prekey bindings (`kami-identity`, `kami-key-provider`).
4. **End-to-End Encryption**: Matrix Vodozemac Olm v1 Double Ratchet engine, canonical inner envelopes, staged authenticated commits, and receipt boundaries (`kami-e2ee`).
5. **Encrypted Storage Engine**: SQLCipher-encrypted SQLite database with profile filesystem leasing, outbox retries, inbox deduplication, contact trust CAS, E2EE session persistence, outbound sender state, and inbound receiver state (`kami-storage`).
6. **Outbound Sender Coordinator**: Synchronous, runtime-neutral message coordination, sequence reservations, 30s attempt leases, exponential backoff, exact-frame crash recovery, and delivery receipt consumption (`kami-sender`).
7. **Inbound Receiver Coordinator**: Synchronous, runtime-neutral inbound frame coordination, duplicate deduplication, presentation ordering, same-session receipt staging, and delivery artifact persistence (`kami-receiver`).
8. **Background Service & CLI**: The client interface and background daemon (`kami-cli`, currently in scaffolding phase).

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
- **Reverse Dependency Pattern (`kami-storage → kami-e2ee`)**: `kami-storage` depends on `kami-e2ee` to consume opaque authenticated commits and delivery artifacts, while `kami-e2ee` never depends on storage. This preserves cryptographic purity and ensures unauthenticated data cannot bypass validation into the database.

### Overall M1 Progress: **Not estimable**
*Milestone 1 acceptance requires two independently verified human CLI endpoints exchanging private text across Linux, Windows, and macOS with direct P2P and genuine automatic relay fallback. While domain, wire protocol, key custody, contact trust, encrypted storage, quarantined E2EE, outbound sender coordinator, and inbound receiver coordinator foundations are implemented and pass on Linux, broader platform qualification, real network fallback, daemon supervisor integration (KAMI-26), and P2P transport (KAMI-27) remain to be done. Subjective percentage estimates are replaced with evidence-based milestone statuses.*

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

    SENDER --> CORE
    SENDER --> STORAGE
    SENDER --> E2EE[crates/kami-e2ee]
    SENDER --> IDENTITY

    RECEIVER --> CORE
    RECEIVER --> STORAGE
    RECEIVER --> E2EE
    RECEIVER --> IDENTITY

    STORAGE --> CORE
    STORAGE --> KEYPROV[crates/kami-key-provider]
    STORAGE --> E2EE

    E2EE --> CORE
    E2EE --> IDENTITY
    E2EE --> PROTO

    IDENTITY --> KEYPROV
    PROTO --> CORE
```

> [!NOTE]
> `kami-sender` and `kami-receiver` coordinate outbound and inbound messaging workflows by orchestrating `kami-storage`, `kami-e2ee`, `kami-identity`, and `kami-core`. `kami-storage` retains its reverse dependency on `kami-e2ee` to consume `AuthenticatedCommit`, `AuthenticatedDelivery`, and `AuthenticatedReceiptCommit`, guaranteeing that only cryptographically verified frames reach disk persistence.

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
- [[kami-cli]]: User commands and daemon scaffold

---

## Interactive Architecture Visualizations (Archify)

The Kami codebase includes source-backed, standalone interactive Archify architecture visualizations:

| ID | Title | Diagram Type | Scope & Description |
| :--- | :--- | :--- | :--- |
| **KAMI-VIS-002** | [Rust Crate Dependency Graph](file:///home/phu/dev/kami/knowledge/Code%20Analyzing/Visualizations/01-System-Architecture/KAMI-VIS-002/crate-dependencies.html) | Architecture | Complete 9-crate dependency topology, service coordinators, reverse storage boundaries, and cargo relationships. |
| **KAMI-VIS-007** | [E2EE Rust Types and Interfaces](file:///home/phu/dev/kami/knowledge/Code%20Analyzing/Visualizations/02-End-to-End-Encryption/KAMI-VIS-007/e2ee-rust-types.html) | Class | QuarantinedSession, AuthenticatedCommit, Endpoint bindings, and Vodozemac Olm adapter interfaces. |
| **KAMI-VIS-014** | [Authenticated E2EE Persistence](file:///home/phu/dev/kami/knowledge/Code%20Analyzing/Visualizations/03-Storage-And-Persistence/KAMI-VIS-014/authenticated-persistence.html) | Sequence | In-process authenticated staging, SQLCipher CAS verification, and exact-frame retry invariants. |
| **KAMI-VIS-018** | [SQLCipher Database Schema](file:///home/phu/dev/kami/knowledge/Code%20Analyzing/Visualizations/03-Storage-And-Persistence/KAMI-VIS-018/sqlcipher-schema.html) | ERD | Physical database schema across migrations 001-005, strict foreign keys, sender attempts, and receiver artifacts. |
| **KAMI-VIS-022** | [Message Delivery State Machine](file:///home/phu/dev/kami/knowledge/Code%20Analyzing/Visualizations/04-Core-Domain/KAMI-VIS-022/message-delivery-lifecycle.html) | Lifecycle | Pure domain delivery state transitions, receipt reducer, and terminal failure projections in `kami-core`. |
| **KAMI-VIS-025** | [End-to-End Messaging Lifecycle & Receipt Flow](file:///home/phu/dev/kami/knowledge/Code%20Analyzing/Visualizations/01-System-Architecture/KAMI-VIS-025/messaging-lifecycle.html) | Sequence | Runtime-neutral sender preparation, inbound validation, same-session receipt staging, and delivery receipt round-trip. |
| **KAMI-VIS-026** | [Receiver Inbound Reliability & Recovery Flow](file:///home/phu/dev/kami/knowledge/Code%20Analyzing/Visualizations/01-System-Architecture/KAMI-VIS-026/receiver-reliability.html) | Workflow | Inbound ingress, ReplayPermit deduplication, conflict detection, same-session staging, gap ordering, and crash recovery. |

---

## Analysis Baseline & Working Tree Manifest

- **Git Branch**: `protocol-storage`
- **Git Commit (HEAD)**: `7bffc6fe8523161e2af3fd8d87b6fb3aadb92bc8` ("Merge KAMI-24 into main")
- **Working Tree State**: Uncommitted & untracked KAMI-25 implementation sitting on top of the HEAD baseline
- **Analysis Timestamp**: `2026-11-06T18:45:00Z`
- **Toolchain**: Rust 2021 Edition / Cargo workspace

### Working Tree Source Manifest (Uncommitted & Untracked Baseline)
To ensure incremental re-analysis reliably detects modifications even when `HEAD` commit remains unchanged, the following SHA-256 hashes record the exact state of all analyzed working tree files:

| File Path | Working Tree Status | SHA-256 Digest |
| :--- | :--- | :--- |
| `Cargo.toml` | Tracked (Modified) | `b0b7cbf95e56678e3786cc44859fcaf51ef293262df837f0706483d6dd87eb2e` |
| `Cargo.lock` | Tracked (Modified) | `bde093449cc1b25ce077b43afff2a957498e32a2d1d405e5430ee41b8d17d170` |
| `docs/KAMI_DEVELOPMENT_ROADMAP.md` | Tracked (Modified) | `e4dfb885cd92d7901ffd7643de968137813ec3c061dc0bc2dbed1cd9939819b5` |
| `crates/kami-e2ee/Cargo.toml` | Untracked | `952e516e217d51de22e371c3420420c4bd2f8ccced5ef24d7b53f6bc9e41134c` |
| `crates/kami-e2ee/README.md` | Untracked | `7f6210cb3d0505c090392c931fa458c1c8c9daf25a484c160628413b53057998` |
| `crates/kami-e2ee/src/authenticated.rs` | Untracked | `8b1f45257cc13a582e36e7a81cd75787e58a2edd1848053ea35895484d4d5a33` |
| `crates/kami-e2ee/src/authenticated/receipts.rs` | Untracked | `54b1ac48abc2da425efc2244b0635f550d19ce2d0d6bfb3716fdc9934439d176` |
| `crates/kami-e2ee/src/envelope.rs` | Untracked | `fec8366fc044922c5ab0bb6c412c4b4dafe7268e6c7d749444515f35f2f85909` |
| `crates/kami-e2ee/src/errors.rs` | Untracked | `7dc030598a70dca657c6106d757e6485d5eb905d54291f7c559c61e8c1c0adb3` |
| `crates/kami-e2ee/src/lib.rs` | Untracked | `7353ebd14e66ef0caa17dfa6d681e7c302f1a94855ed8299165f6b61079723b5` |
| `crates/kami-e2ee/src/persistence.rs` | Untracked | `10cf23f5ea7a02398c765f94fa39f12933f807c6fad335312b6f1c72f3edd371` |
| `crates/kami-e2ee/tests/qualification/receipts.rs` | Untracked | `33c151edef46841493229599a112018faab8db3b2ab2ca5d826cfd3aca6c84ea` |
| `crates/kami-identity/src/custody.rs` | Tracked (Modified) | `9204ec9ca888a695064ef9032faa0533fcfd5dacbf4b9ec78de3a442d2871cc5` |
| `crates/kami-identity/src/lib.rs` | Tracked (Modified) | `536876f0b0cdb3ef916534b97f82ca90940a0f92c61b673386434389beaae45c` |
| `crates/kami-identity/src/olm_binding.rs` | Untracked | `42ca0761bac86b61428f31a6bf5b08103c0b25ee0c7d5c349ad8fe92212bce08` |
| `crates/kami-identity/tests/olm_binding.rs` | Untracked | `1e65b221267cfcb2681bfbeff33480f1521786d1f438f712101aeb1f63a472f3` |
| `crates/kami-receiver/Cargo.toml` | Untracked | `84ad2f95a51cf9583f0e4b322768f1bf0a5be77f576a41735c4d5a02cb3fc922` |
| `crates/kami-receiver/README.md` | Untracked | `478a221eb9158480d00e8f3c7ce0ff0182cb51459142d9991676c1d53a34c85e` |
| `crates/kami-receiver/src/context.rs` | Untracked | `a705746a21a493cf7bece790e0b7062ba3737caf0d3711db83306b581d395ee1` |
| `crates/kami-receiver/src/error.rs` | Untracked | `7fb58bb1105108d748b1d7b8ae19bbf266113f5830b37ef29ff1af824f071b16` |
| `crates/kami-receiver/src/lib.rs` | Untracked | `a62897113b2171acdccb6c20b4d2c1dbce84b21f71476f9600c823ed6692aead` |
| `crates/kami-receiver/src/receive.rs` | Untracked | `af5af6f4a6ed6453ca8af514c10681853a9753a07f2745fef905a288603abbaf` |
| `crates/kami-receiver/tests/failures.rs` | Untracked | `fe6b83abad18e6dea25caa658fa655399ef6596137a2a2cfdc263b7dbffb4ab7` |
| `crates/kami-receiver/tests/receive.rs` | Untracked | `8d8698d300e2c5d6e3af258bf5f0820ce128086f50ad7f0408f694a31e5eec7e` |
| `crates/kami-sender/README.md` | Untracked | `6ad68d485af9d375a77b2558c8dd3c2e43b446ebed97cca3989856341d494b77` |
| `crates/kami-sender/src/coordinator.rs` | Untracked | `b88d2f9938cfa701f0d132930c477bf1e8fd284e79aa667ef2588a4a628f9422` |
| `crates/kami-sender/src/error.rs` | Untracked | `385ac1a3c362c019ef3236b8af934b45e5957572c792facd5f1e53e1f7a2a3d9` |
| `crates/kami-sender/src/lib.rs` | Untracked | `64a7625924460db1dd0ef71a2060a6ffb8d65ecabd6c7044d3cdbb65286380c5` |
| `crates/kami-sender/src/receipt.rs` | Untracked | `a9a0eeea825599e8fe856c1451d5feef416ff7bb1cff30666e970bee2ba1c95d` |
| `crates/kami-sender/tests/coordinator.rs` | Untracked | `a3d4c3f765e768558d61e2bd7dff95b34ee3610f5a38cb376fd4b8eb410d3f3d` |
| `crates/kami-storage/README.md` | Tracked (Modified) | `60b2f981c9b350374bfafcc8128129da6186c012308c7cee9d555d629d5b8cd8` |
| `crates/kami-storage/migrations/005_receiver_state.sql` | Untracked | `edcbd562a9de8201a5e4c7e0710143c6aa9df1cae56169942788c0b1082cf124` |
| `crates/kami-storage/src/lib.rs` | Tracked (Modified) | `555550c42d1e2b498a5d39243fed3eb71bb5a50a490104efe26ebeae295f54a4` |
| `crates/kami-storage/src/migrations.rs` | Tracked (Modified) | `8b4b7df04d4fb5feedb92cdcf3288d6b7d56f86ace30067f24aca5ce9e298adb` |
| `crates/kami-storage/src/ordering.rs` | Tracked (Modified) | `e98593bd6bc3120a9eba528f3f04d7959cfe360acff48a1c7bfaa7eda2759352` |
| `crates/kami-storage/src/receiver_state.rs` | Untracked | `5e40f8585e05d4b561b99bbd1e69be91d3a4c69e5411c36a58706ef844e7647c` |
| `crates/kami-storage/src/transactions.rs` | Tracked (Modified) | `f3fb80b18b817a5daf8275910547f10b08662820f6054ecc0dbce50334f6bf20` |
| `crates/kami-storage/src/worker.rs` | Tracked (Modified) | `91082f2596bfcb7192e7fd94c3b2d4e7a654c3d7ff5e5399ecf6232fc77f0f0b` |
| `crates/kami-storage/tests/receiver_migration.rs` | Untracked | `65b8b6c9491a8080feaa59953ff5f482db92c5d66a2524dd7a747946510af7f1` |
