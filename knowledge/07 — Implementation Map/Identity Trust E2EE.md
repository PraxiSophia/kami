# Identity, Trust, and E2EE Sessions

[[Implementation Map]] · [[Milestone 1 Dependency Graph]]

```mermaid
flowchart LR
  A["KAMI-23 — Device identity/key custody"] --> B["KAMI-50 — Contact trust/Peer-ID binding"]
  A -. "identity contract" .-> C["KAMI-51 — Authenticated sessions/ratchet"]
  B -. "verified device binding" .-> C
  D["KAMI-21 — Codec/validators"] -. "authenticated context" .-> C
  E["KAMI-22 — Encrypted persistence"] -. "crash-safe state" .-> C
  C --> F["KAMI-24 — Sender recovery"]
  C --> G["KAMI-25 — Receiver validation"]
  C --> H["KAMI-27 — Direct transport"]
  C --> I["KAMI-30 — Relay sessions"]
  classDef todo fill:#fff3cd,stroke:#b58105,color:#111
  class A,B,C,D,E,F,G,H,I todo
```

KAMI-22 → KAMI-23 remains a visible Jira inconsistency in the complete graph; it is intentionally absent here as an engineering prerequisite.

Jira links: [KAMI-23](https://pql7227.atlassian.net/browse/KAMI-23) · [KAMI-50](https://pql7227.atlassian.net/browse/KAMI-50) · [KAMI-51](https://pql7227.atlassian.net/browse/KAMI-51) · [KAMI-21](https://pql7227.atlassian.net/browse/KAMI-21) · [KAMI-22](https://pql7227.atlassian.net/browse/KAMI-22)
