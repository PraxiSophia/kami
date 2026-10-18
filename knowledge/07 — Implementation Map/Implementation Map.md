---
title: Kami Implementation Map
type: implementation-map
status: verified-against-repository
---

# Implementation Map

This map separates architectural intent from repository evidence. It must be updated when source code materially changes a component boundary or interface.

## Repository evidence

The current `/home/phu/dev/kami` repository contains Markdown documentation and project instructions only. It has no Rust files, `Cargo.toml`, Cargo workspace, crates, modules, database schema, service binary, CLI, or automated tests.

## Capability map

| Capability | Intended owner | Current evidence | Status |
| --- | --- | --- | --- |
| CLI client | `cli` | No Rust source | Architecturally defined |
| Local authenticated IPC | `ipc` | No implementation | Architecturally defined |
| Background service | `service` | No implementation | Architecturally defined |
| Domain message/delivery state | `domain` | No implementation | Architecturally defined |
| Identity verification and Peer ID binding | `identity` | No implementation | Architecturally defined |
| Signal-style E2EE and Double Ratchet | `crypto` / E2EE | No implementation | Architecturally defined |
| Versioned Protobuf protocol | `protocol` | No implementation | Architecturally defined |
| Encrypted SQLite persistence | `storage` | No implementation | Architecturally defined |
| Direct P2P and NAT traversal | `transport` | No implementation | Architecturally defined |
| Live relay forwarding and failover | `relay` | No implementation | Architecturally defined |
| Reproducible M1 qualification | test/qualification boundary | No implementation or test evidence | Architecturally defined |

## Navigation rule

When implementation begins, replace “No implementation” only with evidence from the repository and relevant verification. Link a concept to a crate or module only after that crate/module exists. Do not infer implementation from Jira status, a planning document, or an architecture note.

Related: [[02 — Module Architecture/Module Architecture]], [[03 — Communication Flows/Communication Flows]], [[05 — Data and State/Data and State]], and [[Reference/Architecture Sources]].