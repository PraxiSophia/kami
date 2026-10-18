---
title: Kami Data and State
type: architecture
status: intended-architecture
---

# Data and State

No database schema or persistence implementation exists in the current repository. This is the intended durable-state model from the approved architecture.

## Conceptual entities

```mermaid
erDiagram
    IDENTITY ||--o{ DEVICE : owns
    DEVICE ||--o{ CONTACT : verifies
    CONTACT ||--o{ CONVERSATION : participates
    CONVERSATION ||--o{ MESSAGE : contains
    MESSAGE ||--o| OUTBOX_ENTRY : queued_as
    MESSAGE ||--o| INBOX_ENTRY : received_as
    MESSAGE ||--o{ DELIVERY_RECEIPT : produces
    DEVICE ||--o{ SESSION : establishes
```

## Message state

```mermaid
stateDiagram-v2
    [*] --> Prepared
    Prepared --> Enqueued: encrypted durable outbox commit
    Enqueued --> InFlight: network attempt
    InFlight --> Enqueued: retryable path failure
    InFlight --> Delivered: recipient durable commit + receipt
    Delivered --> [*]
    InFlight --> Failed: bounded terminal failure
    Failed --> [*]
```

The recipient validates authorization, device binding, E2EE, protocol bounds, and content binding before atomically committing inbox, deduplication, ordering, receipt, and history facts. Same-ID duplicates are re-acknowledged without redisplay; same-ID conflicting content is rejected. Related: [[03 — Communication Flows/Communication Flows]], [[03 — Communication Flows/Relay Fallback and Retry]], and [[02 — Module Architecture/Module Architecture]].