---
title: Background Service Lifecycle
type: flow
status: intended-architecture
---

# Background Service Lifecycle

```mermaid
stateDiagram-v2
    [*] --> Starting
    Starting --> Ready: load protected identity and local state
    Ready --> Connecting: request delivery or session
    Connecting --> Ready: path established or safely queued
    Ready --> Draining: shutdown requested
    Connecting --> Draining: shutdown requested
    Draining --> Stopped: workers and IPC closed
    Stopped --> Starting: explicit restart
```

The manually started background service owns networking, local state, retry, and graceful shutdown. The CLI communicates through authenticated local IPC and must report queued, connecting, delivered, failed, and unavailable states truthfully. Related: [[06 — Runtime and Deployment/Runtime and Deployment]] and [[03 — Communication Flows/Communication Flows]].