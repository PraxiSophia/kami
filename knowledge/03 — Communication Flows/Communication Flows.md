---
title: Kami Communication Flows
type: architecture
status: intended-architecture
---

# Communication Flows

## Direct-first message delivery

```mermaid
sequenceDiagram
    participant A as Sender CLI
    participant SA as Sender service
    participant DA as Sender outbox
    participant N as Direct or live relay path
    participant SB as Recipient service
    participant DB as Recipient inbox
    participant B as Recipient CLI
    A->>SA: send bounded text
    SA->>DA: encrypt and durably enqueue
    SA->>N: attempt authenticated direct delivery
    alt direct unavailable
        SA->>N: select authorized live relay
        N->>SB: forward protected live traffic
    else direct available
        N->>SB: deliver protected traffic directly
    end
    SB->>DB: validate, deduplicate, order, persist
    DB-->>SB: durable commit
    SB-->>N: device_delivered receipt
    N-->>SA: receipt
    SA->>DA: persist delivery fact
    SB->>B: display once
```

## Connected flows

- [[03 — Communication Flows/Contact Verification]] — verified identity precedes private messaging.
- [[03 — Communication Flows/Relay Fallback and Retry]] — direct path, relay candidates, reconnect, and outbox retry.
- [[03 — Communication Flows/Service Lifecycle]] — background service startup and shutdown.
- [[04 — Security and Identity/Security and Identity]] — identity and E2EE constraints applied to every path.
- [[05 — Data and State/Data and State]] — durable state and receipt transitions.

The logical message ID, authorization meaning, E2EE protection, and receipt semantics remain stable when the network path changes. An undelivered message remains in the sender's encrypted local outbox; M1 has no server-side offline mailbox.