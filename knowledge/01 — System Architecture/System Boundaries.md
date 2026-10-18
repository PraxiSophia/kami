---
title: Kami System Boundaries
type: architecture
status: intended-architecture
---

# System Boundaries

## Components

| Component | Responsibility | Boundary |
| --- | --- | --- |
| CLI client | User-facing commands, status, and local requests | Does not own networking or durable message state |
| Background service | Identity, persistence, sessions, transport, retry, and delivery orchestration | Runs locally for one active device |
| Encrypted SQLite state | Durable inbox, outbox, history, receipts, deduplication, ordering, and session state | Local endpoint only |
| Direct transport | Authenticated endpoint-to-endpoint delivery | Preferred path; depends on reachability |
| Live relay | Forwards protected online traffic | No offline mailbox and no recipient authentication authority |
| Identity/E2EE boundary | Verified participant/device identity and application encryption | Applies across direct and relayed paths |

```mermaid
flowchart TB
    subgraph EndpointA["Endpoint A — one active device"]
        CLI_A[CLI client]
        SVC_A[Background service]
        DB_A[(Encrypted SQLite)]
        CLI_A <-->|authenticated local IPC| SVC_A
        SVC_A <--> DB_A
    end
    subgraph Network["Communication boundary"]
        DIRECT[Authenticated direct P2P]
        RELAY[Authorized live relay]
    end
    subgraph EndpointB["Endpoint B — one active device"]
        CLI_B[CLI client]
        SVC_B[Background service]
        DB_B[(Encrypted SQLite)]
        CLI_B <-->|authenticated local IPC| SVC_B
        SVC_B <--> DB_B
    end
    SVC_A -->|direct first| DIRECT --> SVC_B
    SVC_A -.->|fallback for live traffic| RELAY -.-> SVC_B
    E2EE[Application E2EE] -. protects logical messages on both paths .- DIRECT
    E2EE -. protects logical messages on both paths .- RELAY
```

## Current versus future

M1 covers verified one-to-one human-operated CLI endpoints, one active device per participant, bounded private text, encrypted local state, direct P2P, supported NAT traversal, and authorized live-relay fallback. Server-side offline mailboxes, groups, artifacts, agent task execution, federation, and multi-device recovery are future extensions, not current components.

Related: [[02 — Module Architecture/Module Architecture]], [[03 — Communication Flows/Communication Flows]], [[04 — Security and Identity/Security and Identity]], [[06 — Runtime and Deployment/Runtime and Deployment]].