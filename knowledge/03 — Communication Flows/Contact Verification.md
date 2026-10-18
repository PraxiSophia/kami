---
title: Contact Verification Flow
type: flow
status: intended-architecture
---

# Contact Verification

Invitations may carry public identity and connectivity hints, but those values are untrusted until both parties compare and verify fingerprints through an independent channel.

```mermaid
sequenceDiagram
    participant A as Participant A
    participant SA as A service
    participant B as Participant B
    participant SB as B service
    A->>SA: create invitation
    SA-->>B: invitation with identity/connectivity hints
    B->>SB: inspect invitation fingerprint
    A->>B: compare fingerprints independently
    B->>SB: confirm verified contact
    SB-->>SA: authorized contact state
    SA->>SA: bind verified device identity to Peer ID
```

Unknown or changed keys block private messaging until reverification. Display names, addresses, relay admission, and self-signatures are not identity proof. Related: [[04 — Security and Identity/Security and Identity]] and [[02 — Module Architecture/Module Architecture]].