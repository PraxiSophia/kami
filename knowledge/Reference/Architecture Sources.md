---
title: Architecture Sources and Maintenance
type: reference
status: current
---

# Architecture Sources and Maintenance

## Authority

- Requirements: `docs/Kami_Project_Requirements.md`
- Communication design: `docs/Kami_Communication_Design.md`
- Identity and encryption: `docs/architecture/Identity_And_Encryption.md`
- Messaging protocol: `docs/architecture/Messaging_Protocol.md`
- Supporting implementation guidance: `docs/architecture/`, `docs/planning/`
- Actual implementation truth: repository source, manifests, and verification evidence

The canonical documents define intended requirements and architecture. The repository defines what is implemented. This vault describes relationships without promoting planned modules into existing code.

## Maintenance rule

Update only architecture notes affected by material changes to module boundaries, responsibilities, interfaces, dependencies, data flows, security boundaries, persistence, runtime, or deployment. Do not update for trivial formatting or variable renames. Obsidian updates require separate explicit owner authorization under the Kami repository policy in `AGENTS.md`.

Preserve [[Owner-Notes/README]] and [[Owner-Notes/Change Log]] as manual owner content.

## Related architecture areas

- [[00 — Architecture Home/Kami Architecture Home]]
- [[02 — Module Architecture/Module Architecture]]
- [[07 — Implementation Map/Implementation Map]]
- [[Update Workflow]]