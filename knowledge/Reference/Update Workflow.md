---
title: Architecture Knowledge Base Maintenance
type: process
status: current
---

# Architecture Knowledge Base Maintenance

Obsidian is a derived architecture knowledge base, not a Jira backlog or requirements authority.

## When to update

Update only when an authorized repository change materially affects:

- module or crate boundaries;
- component responsibilities or public interfaces;
- cross-module dependencies;
- message, data, or control flows;
- security boundaries or identity behavior;
- persistence models;
- runtime, deployment, or platform behavior; or
- implementation evidence and capability status.

Do not update for formatting, variable renames, isolated refactors with no architectural effect, or routine Jira changes.

## Update process

1. Read the four canonical documents and inspect the actual repository source.
2. Identify the smallest affected architecture notes and diagrams.
3. Preserve the distinction between intended architecture and implemented behavior.
4. Update meaningful internal links when boundaries or relationships change.
5. Preserve `Owner-Notes/` completely.
6. Obtain separate explicit owner authorization before editing Obsidian.
7. Validate that internal links, Mermaid blocks, and implementation-status claims remain correct.

Jira remains the source of truth for coding tasks, progress, assignments, acceptance criteria, and completion status. Do not reproduce Jira issues or statuses here.

Related: [[Architecture Sources]], [[00 — Architecture Home/Kami Architecture Home]], and [[07 — Implementation Map/Implementation Map]].
