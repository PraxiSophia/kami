# Integration, Runtime, and Qualification

[[Implementation Map]] · [[Milestone 1 Dependency Graph]]

```mermaid
flowchart LR
  A["KAMI-26 — Supervised engine"] --> B["KAMI-32 — IPC/CLI/diagnostics"]
  C["KAMI-33 — Harness/evidence recorder"] --> D["KAMI-34 — Local matrix"]
  B --> E["KAMI-35 — Real-network qualification"]
  C --> E
  D --> F["KAMI-37 — Privacy/ciphertext boundaries"]
  E --> G["KAMI-39 — Independent M1 review"]
  F --> G
  H["KAMI-36 — Regression/failure matrix"] --> G
  I["KAMI-38 — Test contract/traceability (Done)"] -. "evidence coordination" .-> C
  classDef done fill:#d4edda,stroke:#2e7d32,color:#111
  classDef todo fill:#fff3cd,stroke:#b58105,color:#111
  classDef gate fill:#dbeafe,stroke:#2563eb,color:#111
  class A,B,C,D,E,F,H todo
  class G gate
  class I done
```

KAMI-39 is the final evidence gate. Qualification claims must remain bounded to the actually tested OS, network, transport, and relay configurations.

Jira links: [KAMI-26](https://pql7227.atlassian.net/browse/KAMI-26) · [KAMI-32](https://pql7227.atlassian.net/browse/KAMI-32) · [KAMI-33](https://pql7227.atlassian.net/browse/KAMI-33) · [KAMI-34](https://pql7227.atlassian.net/browse/KAMI-34) · [KAMI-35](https://pql7227.atlassian.net/browse/KAMI-35) · [KAMI-36](https://pql7227.atlassian.net/browse/KAMI-36) · [KAMI-37](https://pql7227.atlassian.net/browse/KAMI-37) · [KAMI-38](https://pql7227.atlassian.net/browse/KAMI-38) · [KAMI-39](https://pql7227.atlassian.net/browse/KAMI-39)
