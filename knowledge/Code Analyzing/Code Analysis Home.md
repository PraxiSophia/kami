---
last_analyzed_branch: "identity-security"
last_analyzed_commit: "eedbb744565cb8af6c80c78b1b76b431c7bf0562"
last_analyzed_timestamp: "2026-11-03T18:30:00Z"
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

Milestone 1 focuses on five core foundations:
1. **Domain Core**: Pure data structures, message identity, deduplication logic, and delivery state tracking (`kami-core`).
2. **Wire Protocol**: Strict, bounded, zero-allocation-on-bad-input Protobuf serialization (`kami-protocol`).
3. **Key Custody & Contact Trust**: OS keyring integration (Linux Secret Service, macOS Keychain, Windows Credential Manager), bilateral fingerprint verification, and signed Olm prekey bindings (`kami-identity`, `kami-key-provider`).
4. **End-to-End Encryption**: Matrix Vodozemac Olm v1 Double Ratchet engine, canonical inner envelopes, and staged authenticated commits (`kami-e2ee`).
5. **Encrypted Storage Engine**: SQLCipher-encrypted SQLite database with profile filesystem leasing, outbox retries, inbox deduplication, contact trust CAS, and E2EE session persistence (`kami-storage`).
6. **Background Service & CLI**: The client interface and background daemon (`kami-cli`, currently in scaffolding phase).

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
- **Reverse Dependency Pattern (`kami-storage → kami-e2ee`)**: `kami-storage` depends on `kami-e2ee` to consume opaque authenticated commits, while `kami-e2ee` never depends on storage. This preserves cryptographic purity and ensures unauthenticated data cannot bypass validation into the database.

### Overall M1 Progress: **Not estimable**
*Milestone 1 acceptance requires two independently verified human CLI endpoints exchanging private text across Linux, Windows, and macOS with direct P2P and genuine automatic relay fallback. While domain, wire protocol, key custody, contact trust, encrypted storage, and quarantined E2EE foundations are implemented and pass on Linux, broader platform qualification, real network fallback, message lifecycle actors, and daemon integration remain to be done. Subjective percentage estimates are replaced with evidence-based milestone statuses.*

*Whole-project progress: **Not estimable** (broader roadmap items like multi-device synchronization, offline mailbox relays, and agent harness bridges are not yet fully specified).*

---

## Workspace Crates Map

| Crate Note | Primary Purpose | Milestone 1 Status | Test Status |
| :--- | :--- | :--- | :--- |
| [[kami-core]] | Pure domain facts, state machine transitions, and queue limits (no I/O or dependencies). | **100%** of domain scope | Verified (unit tests, 5 proptests, 7 golden vectors, 99.6% coverage) |
| [[kami-protocol]] | Bounded Protobuf encoding/decoding and structural frame validation (up to 64 KB). | **100%** of wire frame scope | Verified (unit tests, vectors, fuzz/proptests, memory probe) |
| [[kami-key-provider]] | Synchronous database key custody contract and memory-zeroizing key type. | **100%** of contract scope | Verified (unit tests, fault injection, restart/loss tests) |
| [[kami-identity]] | OS keyring custody, contact trust (KAMI-50), and signed Olm prekey bindings (WP2-A1). | **Not estimable** *(Functionally Implemented)* | Verified on Linux; Windows/macOS compiled & Clippy-checked; native qualification pending |
| [[kami-storage]] | SQLCipher encrypted local database, profile leasing, migrations 001-003, and E2EE commits. | **Not estimable** *(Functionally Implemented)* | Verified on Linux (migrations, crash-kill checkpoints, CAS, WAL); native qualification pending |
| [[kami-e2ee]] | Vodozemac Double Ratchet adapter, inner envelope, and typed storage commits (KAMI-51). | **Not estimable** *(Quarantined Foundation Accepted with Conditions)* | Verified on Linux (93 tests, privacy probe, qualification suites); runtime/network deferred |
| [[kami-cli]] | Command-line client and background service daemon. | **Scaffold only** *(15% placeholder)* | Scaffold tests only; IPC and commands pending |

---

## Architecture & Dependency Flow

```mermaid
graph TD
    CLI[crates/kami-cli] --> CORE[crates/kami-core]
    CLI --> PROTO[crates/kami-protocol]
    CLI --> STORAGE[crates/kami-storage]
    CLI --> IDENTITY[crates/kami-identity]

    STORAGE --> CORE
    STORAGE --> KEYPROV[crates/kami-key-provider]
    STORAGE --> E2EE[crates/kami-e2ee]

    E2EE --> CORE
    E2EE --> IDENTITY
    E2EE --> PROTO

    IDENTITY --> KEYPROV
    PROTO --> CORE
```

> [!NOTE]
> Notice the acyclic dependency direction between `kami-storage` and `kami-e2ee`: `kami-storage` imports `kami-e2ee` to consume `AuthenticatedCommit`, guaranteeing that only cryptographically verified frames reach disk persistence.

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
- [[kami-storage]]: Encrypted SQLCipher database, profile leases, and E2EE commits
- [[kami-e2ee]]: Double Ratchet session engine, authenticated envelopes, and typed commits
- [[kami-cli]]: User commands and daemon scaffold

---

## Analysis Baseline & Working Tree Manifest

- **Git Branch**: `identity-security`
- **Git Commit (HEAD)**: `eedbb744565cb8af6c80c78b1b76b431c7bf0562`
- **Analysis Timestamp**: `2026-11-03T18:30:00Z`
- **Toolchain**: Rust 2021 Edition / Cargo workspace

### Working Tree Source Manifest (Uncommitted & Untracked Baseline)
To ensure incremental re-analysis reliably detects modifications even when `HEAD` commit remains unchanged, the following SHA-256 hashes record the exact state of all analyzed working tree files:

| File Path | Working Tree Status | SHA-256 Digest |
| :--- | :--- | :--- |
| `Cargo.toml` | Tracked (Modified) | `18c77249e3816627bdc478d6da19f881044f85d4f2ad0a3942bb826275ba2416` |
| `Cargo.lock` | Tracked (Modified) | `4554c953a589ddd102dfcb0cdba053ba56a819fc160d83017c9a885aac142e9b` |
| `crates/kami-identity/src/custody.rs` | Tracked (Modified) | `9204ec9ca888a695064ef9032faa0533fcfd5dacbf4b9ec78de3a442d2871cc5` |
| `crates/kami-identity/src/lib.rs` | Tracked (Modified) | `536876f0b0cdb3ef916534b97f82ca90940a0f92c61b673386434389beaae45c` |
| `crates/kami-identity/src/olm_binding.rs` | Untracked | `42ca0761bac86b61428f31a6bf5b08103c0b25ee0c7d5c349ad8fe92212bce08` |
| `crates/kami-identity/tests/olm_binding.rs` | Untracked | `1e65b221267cfcb2681bfbeff33480f1521786d1f438f712101aeb1f63a472f3` |
| `crates/kami-storage/Cargo.toml` | Tracked (Modified) | `d815170eda0d4150ffbf9fb973efd36d706f59309371aeb5aafa07da161abfe3` |
| `crates/kami-storage/migrations/003_e2ee_state.sql` | Untracked | `439bf3af22c60ac28394fab6c8e3e42f9244db800384b6744f97087f62f2a2c2` |
| `crates/kami-storage/src/inbox.rs` | Tracked (Modified) | `467fee923951f146d170f2f48edcb8b348f8f3363d8e353f44c638ca430cb1e7` |
| `crates/kami-storage/src/lib.rs` | Tracked (Modified) | `ea3806c2259528eae52b3beb6d5713bf8020d993fbd5fed3b20360b34b0f1072` |
| `crates/kami-storage/src/migrations.rs` | Tracked (Modified) | `bedadf8ac03bad141d273e77f228ca2acb0158d5157ac5bbd59111ecfe672593` |
| `crates/kami-storage/src/session_state.rs` | Untracked | `1608de96354b86c7e989c15a31207198b60811b79838e379e91409a47e6b34e9` |
| `crates/kami-storage/src/transactions.rs` | Tracked (Modified) | `e88f96c7df9e0d178179543a9b333b16b46401b511d5122fa0ead617a486742c` |
| `crates/kami-storage/src/worker.rs` | Tracked (Modified) | `274b7b5ea6c9de1bb5327312a782fbeac5af525bfb2633a85197b04d98e0df14` |
| `crates/kami-storage/src/worker/tests.rs` | Tracked (Modified) | `d62720b69ee76105b6d14137690a1f36f089a0ab7ed23f51465848ae83ecfb84` |
| `crates/kami-storage/tests/authenticated_boundary.rs` | Untracked | `b61b59105dc7c0db14f5224b261372808c874b6c6e35c7799327e9e963153716` |
| `crates/kami-storage/tests/contact_trust.rs` | Tracked (Modified) | `7ac0f175043650c1c9fdc06c2468c0eb9ec51bbb11b319e2583fcfaed03d0ee5` |
| `crates/kami-storage/tests/e2ee_state.rs` | Untracked | `1028c4a43cb5c4042a107c01ba567cef0cf5af745b9b2765f4a03f16773ce53b` |
| `crates/kami-e2ee/Cargo.toml` | Untracked | `952e516e217d51de22e371c3420420c4bd2f8ccced5ef24d7b53f6bc9e41134c` |
| `crates/kami-e2ee/src/authenticated.rs` | Untracked | `e0e1ba780d4c5a734b04409b9cdb5d050d86fea3ebfd988bfe46cb8537dd894e` |
| `crates/kami-e2ee/src/envelope.rs` | Untracked | `2eb84a28d3a056bc2c5b67587334adc50b94fc6230f5c5e2bb5eeb888b05e543` |
| `crates/kami-e2ee/src/errors.rs` | Untracked | `7dc030598a70dca657c6106d757e6485d5eb905d54291f7c559c61e8c1c0adb3` |
| `crates/kami-e2ee/src/lib.rs` | Untracked | `d7cb107544d70afb3d56723d1425e956e51e84234c555ab1fa5ba5f4d74a07b9` |
| `crates/kami-e2ee/src/persistence.rs` | Untracked | `10cf23f5ea7a02398c765f94fa39f12933f807c6fad335312b6f1c72f3edd371` |
| `crates/kami-e2ee/src/profile.rs` | Untracked | `4ec9df7f76bef118059d1c7cf509ece1f4d632e3eaffff9e80faccd25b20885d` |
| `crates/kami-e2ee/src/provider.rs` | Untracked | `ccdac27d3663f3fb99c2e9b94c51cb92e3ba3bbc77bbe629d9e532f305c9a428` |
| `crates/kami-e2ee/src/state.rs` | Untracked | `42e62f7d90d0db4f2c49975b82590c4f7d04d0359aa49aacffc9382cab488231` |
| `crates/kami-e2ee/src/types.rs` | Untracked | `109124243a3bebabcd86e49afd4a0eedeff95558456c533f2bc46efd43decc16` |
| `crates/kami-e2ee/src/vodozemac.rs` | Untracked | `6142acad24bf214dba7b1e330b74b01b5f05c7bd853350c5f34a5e87ccbaf964` |
