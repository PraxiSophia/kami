# Milestone 1 Jira Dependency Graph

[[Implementation Map]] · Jira snapshot: 2026-10-19T02:02:28-04:00

Solid arrows are current Jira `Blocks` links. The red dotted arrow is intentionally not accepted as a prerequisite.

```mermaid
flowchart LR
  K19["KAMI-19 — Workspace scaffold"] --> K20["KAMI-20 — Core IDs and reducer"]
  K20 --> K21["KAMI-21 — Codec and validators"]
  K20 --> K22["KAMI-22 — SQLite and profile lease"]
  K20 --> K33["KAMI-33 — Harness and evidence"]
  K21 --> K24["KAMI-24 — Sender enqueue/recovery"]
  K21 --> K25["KAMI-25 — Receiver inbox/dedup/receipts"]
  K22 --> K24
  K22 --> K25
  K22 --> K26["KAMI-26 — Supervised engine"]
  K22 --> K37["KAMI-37 — Privacy verification"]
  K22 -. "UNRESOLVED Jira link; not a valid prerequisite" .-> K23["KAMI-23 — Device identity/key custody"]
  K23 --> K24
  K23 --> K25
  K23 --> K26
  K23 --> K27["KAMI-27 — Direct transport"]
  K23 --> K30["KAMI-30 — Relay sessions"]
  K23 -. "interface coordination" .-> K51["KAMI-51 — Authenticated sessions/ratchet"]
  K21 -. "wire/session interface" .-> K51
  K50["KAMI-50 — Trust and Peer-ID binding"] --> K24
  K50 --> K25
  K50 --> K27
  K50 --> K30
  K50 -. "verified identity input" .-> K51
  K22 -. "session-state persistence interface" .-> K51
  K24 --> K25
  K24 --> K26
  K24 --> K31["KAMI-31 — Direct-first fallback"]
  K25 --> K26
  K25 --> K31
  K26 --> K31
  K26 --> K32["KAMI-32 — IPC/CLI/diagnostics"]
  K27 --> K30
  K27 --> K31
  K27 --> K35["KAMI-35 — Real-network qualification"]
  K28["KAMI-28 — First encrypted direct message"] --> K31
  K28 --> K35
  K29["KAMI-29 — Live relay service"] --> K30
  K29 --> K31
  K29 --> K35
  K29 --> K37
  K30 --> K31
  K30 --> K35
  K31 --> K35
  K32 --> K35
  K32 --> K37
  K33 --> K35
  K34["KAMI-34 — Local test matrix"] --> K37
  K34 --> K39["KAMI-39 — Independent M1 review"]
  K35 --> K39
  K36["KAMI-36 — Regression/failure matrix"] --> K39
  K38["KAMI-38 — Test contract and traceability (Done)"]
  K37 --> K39
  classDef done fill:#d4edda,stroke:#2e7d32,color:#111
  classDef todo fill:#fff3cd,stroke:#b58105,color:#111
  classDef gate fill:#dbeafe,stroke:#2563eb,color:#111
  classDef inconsistency fill:#f8d7da,stroke:#b71c1c,color:#111
  class K19,K20,K38 done
  class K21,K22,K23,K24,K25,K26,K27,K28,K29,K30,K31,K32,K33,K34,K35,K36,K37,K50,K51 todo
  class K39 gate
  class K23 inconsistency
```

## Navigation

[[Core Protocol Storage]] · [[Identity Trust E2EE]] · [[Delivery Transport Relays]] · [[Integration Runtime Qualification]]

Jira links: [KAMI-21](https://pql7227.atlassian.net/browse/KAMI-21) · [KAMI-22](https://pql7227.atlassian.net/browse/KAMI-22) · [KAMI-23](https://pql7227.atlassian.net/browse/KAMI-23) · [KAMI-50](https://pql7227.atlassian.net/browse/KAMI-50) · [KAMI-51](https://pql7227.atlassian.net/browse/KAMI-51)
