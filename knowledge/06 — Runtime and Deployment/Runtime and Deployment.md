---
title: Kami Runtime and Deployment
type: architecture
status: intended-architecture
---

# Runtime and Deployment

## Deployment topology

```mermaid
flowchart TB
    subgraph HostA["Computer A — Linux, Windows, or macOS"]
        CLI_A[CLI]
        SVC_A[Background service]
        STATE_A[(Encrypted local state)]
        CLI_A <-->|Unix socket or named pipe| SVC_A
        SVC_A --> STATE_A
    end
    subgraph Network["Network paths"]
        P2P[Direct authenticated P2P]
        R1[Independent relay A]
        R2[Independent relay B]
    end
    subgraph HostB["Computer B — Linux, Windows, or macOS"]
        CLI_B[CLI]
        SVC_B[Background service]
        STATE_B[(Encrypted local state)]
        CLI_B <-->|Unix socket or named pipe| SVC_B
        SVC_B --> STATE_B
    end
    SVC_A --> P2P --> SVC_B
    SVC_A -. fallback .-> R1 -.-> SVC_B
    SVC_A -. alternate fallback .-> R2 -.-> SVC_B
```

## Runtime responsibilities

- The service is manually started and owns networking, durable state, sessions, retries, and shutdown.
- Linux and macOS use Unix domain sockets for local IPC; Windows uses named pipes.
- Direct P2P is preferred, with QUIC preferred and TCP available as an alternative within the approved design.
- Supported NAT traversal is attempted; universal reachability is not assumed.
- Multiple independently operated relays provide live fallback without inter-relay federation.
- Shutdown drains or safely persists work without claiming delivery that was not acknowledged.

Related: [[03 — Communication Flows/Service Lifecycle]], [[01 — System Architecture/System Boundaries]], and [[07 — Implementation Map/Implementation Map]].