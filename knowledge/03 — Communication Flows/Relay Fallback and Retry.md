---
title: Relay Fallback and Retry
type: flow
status: intended-architecture
---

# Relay Fallback and Retry

```mermaid
stateDiagram-v2
    [*] --> Enqueued: encrypted outbox commit
    Enqueued --> DirectAttempt
    DirectAttempt --> Delivered: authenticated direct path
    DirectAttempt --> RelayAttempt: direct unavailable
    RelayAttempt --> Delivered: authorized live relay succeeds
    RelayAttempt --> Waiting: all live paths unavailable
    Waiting --> DirectAttempt: bounded reconnect/retry
    Delivered --> Acknowledged: receipt validated
    Acknowledged --> [*]
```

Relays forward live end-to-end-protected traffic. They do not hold offline mail, authenticate the recipient on their own, or alter message identity. Multiple independently operated relay candidates may be selected and retried without inter-relay federation. Related: [[03 — Communication Flows/Communication Flows]], [[04 — Security and Identity/Security and Identity]], and [[05 — Data and State/Data and State]].