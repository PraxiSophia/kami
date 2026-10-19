# Message Delivery, Transport, and Relays

[[Implementation Map]] · [[Milestone 1 Dependency Graph]]

```mermaid
flowchart LR
  A["KAMI-24 — Sender enqueue/recovery"] --> B["KAMI-28 — First encrypted direct message"]
  C["KAMI-25 — Receiver inbox/dedup/receipts"] --> B
  D["KAMI-27 — Direct transport"] --> E["KAMI-30 — Relay reservation/inner sessions"]
  F["KAMI-29 — Live relay service"] --> E
  B --> G["KAMI-31 — Direct-first fallback"]
  E --> G
  D --> G
  G --> H["KAMI-35 — Real-network qualification"]
  I["KAMI-50 — Trust/Peer-ID binding"] --> D
  I --> E
  J["KAMI-51 — Session/ratchet integration"] -. "session contract" .-> B
  J -. "session contract" .-> E
  classDef todo fill:#fff3cd,stroke:#b58105,color:#111
  class A,B,C,D,E,F,G,H,I,J todo
```

The same logical message crosses direct and relay paths; transport success is not recipient delivery. KAMI-31 is the integration gate for automatic direct-first fallback.

Jira links: [KAMI-24](https://pql7227.atlassian.net/browse/KAMI-24) · [KAMI-25](https://pql7227.atlassian.net/browse/KAMI-25) · [KAMI-27](https://pql7227.atlassian.net/browse/KAMI-27) · [KAMI-28](https://pql7227.atlassian.net/browse/KAMI-28) · [KAMI-29](https://pql7227.atlassian.net/browse/KAMI-29) · [KAMI-30](https://pql7227.atlassian.net/browse/KAMI-30) · [KAMI-31](https://pql7227.atlassian.net/browse/KAMI-31)
