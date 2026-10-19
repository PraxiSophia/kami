# Core, Protocol, and Storage

[[Implementation Map]] · [[Milestone 1 Dependency Graph]]

```mermaid
flowchart LR
  A["KAMI-19 — Workspace scaffold"] --> B["KAMI-20 — Core IDs/reducer"]
  B --> C["KAMI-21 — Codec/validators"]
  B --> D["KAMI-22 — SQLite/profile lease"]
  C --> E["KAMI-24 — Sender enqueue/recovery"]
  C --> F["KAMI-25 — Receiver inbox/dedup/receipts"]
  D --> E
  D --> F
  D --> G["KAMI-26 — Supervised engine"]
  E --> G
  F --> G
  C -. "wire/session interface" .-> H["KAMI-51 — Session/ratchet integration"]
  D -. "session-state persistence" .-> H
  classDef done fill:#d4edda,stroke:#2e7d32,color:#111
  classDef todo fill:#fff3cd,stroke:#b58105,color:#111
  class A,B done
  class C,D,E,F,G,H todo
```

Parallel start: KAMI-21 and KAMI-22 can proceed after KAMI-20. KAMI-24 and KAMI-25 converge on the supervised engine after their explicit prerequisites.

Jira links: [KAMI-19](https://pql7227.atlassian.net/browse/KAMI-19) · [KAMI-20](https://pql7227.atlassian.net/browse/KAMI-20) · [KAMI-21](https://pql7227.atlassian.net/browse/KAMI-21) · [KAMI-22](https://pql7227.atlassian.net/browse/KAMI-22) · [KAMI-24](https://pql7227.atlassian.net/browse/KAMI-24) · [KAMI-25](https://pql7227.atlassian.net/browse/KAMI-25) · [KAMI-26](https://pql7227.atlassian.net/browse/KAMI-26)
