# Kami — Shared Agent Instructions

## Project
Kami is a communication-first platform for secure communication between:
- Humans and humans
- Humans and AI agents
- AI agents and AI agents

Kami aims to support cross-network communication, interoperability between
different AI agent harnesses, and organization-based collaboration.

## Canonical Documentation
Read these documents before planning or implementing changes:
- docs/Kami_Project_Requirements.md
- docs/Kami_Communication_Design.md
- docs/architecture/Identity_And_Encryption.md
- docs/architecture/Messaging_Protocol.md

Do not assume proposed features are approved requirements.

## Workflow Policy

### 1. Investigation and study

- Investigation is read-only by default.
- Report findings, explanations, comparisons, and recommendations in chat.
- Do not create investigation reports, study Markdown, planning documents, Jira issues, or Obsidian notes without explicit authorization.
- Investigation does not authorize implementation or changes to project records.

### 2. Pending decisions

- Present unresolved decisions, options, trade-offs, and recommendations in chat.
- Do not record pending decisions as approved requirements.
- Do not create Jira issues or Markdown documents merely to track pending approvals.
- Never make a decision on the owner's behalf.
- Questions, requested changes, or deferral do not constitute approval.
- Stop work that depends on an unresolved decision.

### 3. Repository file changes — one-stage approval required

#### Before changing repository files

- This applies to creating, modifying, deleting, renaming, or moving any files or directories in the Kami repository.
- Briefly summarize the intended changes, their purpose, and affected files.
- Never present a proposed code diff, patch, or line-by-line change preview.
- Immediately present an interactive approval menu using `clarify(decision=true)` with exactly two options: `Approve` and `Deny`.
- Selecting an option immediately records the decision; no extra confirmation is required.
- This is an immediate, session-local approval. Do not create persistent approval requests or use a dashboard approval queue.

#### Delegated Kanban approval relay

- An interactive `kami-manager` approval may authorize an existing Kanban implementation task only through an explicit recorded approval created after the owner selects `Approve`.
- The recorded approval must name the repository, exact Kanban task ID(s), exact permitted file paths, permitted file operations, and any required serialization constraint. It must identify `kami-manager` as the approving profile and remain bound to the approved task scope.
- Before editing, a worker must read back and verify that approval record. Missing, denied, expired, malformed, ambiguous, or mismatched approval blocks execution. Workers cannot create, approve, extend, or reinterpret their own authorization.
- Approval does not cover future or unrelated tasks. Any path, operation, task, or scope expansion requires a new interactive owner approval and a new recorded approval.
- This relay changes only how the mandatory owner approval reaches an explicitly named worker task; it does not authorize Jira, Obsidian, Git, commits, pushes, merges, deployments, or other external operations, which remain separately authorized.

#### If approved

- Make only the authorized file changes.
- Perform relevant verification or tests.
- Give a concise completion summary.
- Do not show a diff unless explicitly requested.
- Do not request post-change approval.
- Do not ask for approval again for individual files within the already approved scope.

#### If denied or unanswered

- Do not modify any repository files.
- Do not interpret silence, discussion, or timeouts as approval.
- If interactive approval is unavailable, stop dependent work and report the blocker.

#### Version control and deployment

- Never commit, push, merge, or deploy without separate explicit authorization.
- Permission to change repository files does not imply permission to commit, push, merge, or deploy.

### 4. Approved technical decisions

- After the owner approves a requirement or architecture decision, propose which existing Kami Markdown and Obsidian documents should change; obtain authorization before editing them.
- Update relevant technical documentation directly rather than creating separate decision logs.
- Do not turn unapproved proposals into finalized requirements.
- Keep approved documentation consistent with the implementation and agreed architecture.

### 5. Jira

- Jira is only for actionable coding work: implementation epics, stories, tasks, bugs, and tests.
- Do not create Jira issues for research, study, planning, governance, approval tracking, or pending decisions.
- Obtain approval before creating, editing, transitioning, or closing Jira issues.
- Jira approval does not authorize code implementation, and code approval does not authorize Jira changes.
- Do not mark a coding issue complete until its implementation satisfies the shared Definition of Done.
- Each coding issue should include:
  - A clear summary and expected behavior.
  - Concise technical context and actionable implementation guidance.
  - Specific, testable acceptance criteria.
  - Relevant dependencies and references to approved technical documentation.
  - A reference to the shared Definition of Done in `AGENTS.md`.
- Jira implementation guidance must remain short and actionable. Do not duplicate full architecture documentation in Jira.

### 6. Enforcement

- Apply this policy to all Kami agents, including delegated workers and automated Kanban execution.
- If approval is missing, stop dependent work and ask the owner.
- Independent, already-authorized work may continue within its approved scope.
- Do not bypass approval by delegating work; approvals are scoped authorizations, not permanent permissions.
- A delegated worker's completion report or successful test does not replace the shared Definition of Done.
- Work awaiting repository-change approval remains blocked until the owner explicitly approves it. After authorized changes are completed, no additional owner approval is required.
- If an automated worker cannot obtain interactive approval, it reports the pending request and stops dependent work.

## Project Scope
Repository: /home/phu/dev/kami
GitHub: https://github.com/PraxiSophia/kami
Jira project: KAMI
Jira site: https://pql7227.atlassian.net

Only work on Kami when operating under a kami-* profile.
Do not modify unrelated repositories or projects.

## Development Rules
- Use Jira only for approved, actionable coding work.
- Use GitHub branches and pull requests for implementation.
- Do not push directly to main.
- Do not merge pull requests without authorization.
- Keep implementation aligned with approved requirements.
- Add tests for new behavior and bug fixes.
- Update architectural and protocol documentation only after an approved decision and explicit authorization to edit.
- Never commit credentials, tokens, or secrets.
- Report blockers rather than silently changing requirements.

### Architecture-First Development Workflow

- Before substantial implementation, assign an appropriate architecture specialist through Kanban to review cohesive module/file boundaries, responsibilities, public interfaces, dependency direction, test organization, and future extensibility.
- Prefer cohesive Rust modules and appropriately organized crates. Avoid unnecessarily large, highly coupled modules, but do not impose arbitrary file-size limits or split cohesive code solely to reduce line counts.
- Assign substantial implementation work to suitable development specialists through Kanban and use the reviewed architecture plan.
- For each substantially changed crate, maintain a concise `crates/<crate>/README.md` describing its current purpose, modules, major interfaces, dependencies, implemented behavior, limitations, and verified manual-test commands.
- Assign substantial crate README work to an appropriate existing specialist through Kanban; do not create a new agent profile unnecessarily.
- Before reporting completion, run applicable Cargo compilation/checks, tests, formatting, and Clippy; verify acceptance criteria and report unverified checks or remaining failures explicitly.
- `kami-manager` must verify worker execution evidence, actual repository changes, and test results before reporting completion.
- Kanban assignment never bypasses owner authorization. Preserve the existing one-stage `Approve`/`Deny` repository gate and separate repository, Jira, Obsidian, and Git authorization requirements.

### Obsidian Architecture Maintenance

- When an authorized repository change materially affects architecture, identify the affected Obsidian architecture notes and diagrams.
- Update Obsidian only with separate explicit owner authorization; repository-change approval does not authorize vault edits.
- Keep Obsidian focused on components, interfaces, dependencies, runtime behavior, and implementation evidence rather than Jira tasks or trivial code changes.

### Critical-Task Policy

- Classify a task as security-critical or architecturally critical when failure could materially compromise cryptographic confidentiality, integrity, authentication, key management, identity/trust/access control, encrypted persistence or crash recovery, secure transport/relay boundaries, protocol/session/replay invariants, message correctness, delivery guarantees, or durable security state.
- Classify conservatively when uncertain.
- For critical tasks, Kanban must sequence: focused `kami-architect` review → independent `kami-evaluator` security/feasibility review → `kami-senior-dev` implementation → independent `kami-qa` verification → independent `kami-evaluator` final review → `kami-manager` reconciliation.
- Evaluator findings block progression until fixed or explicitly escalated to the owner.
- Routine documentation, formatting, isolated low-risk refactoring, and ordinary CLI presentation are not critical by default.
- Kanban assignment does not authorize repository, Jira, Obsidian, or Git changes; the existing scoped `Approve`/`Deny` repository gate remains mandatory.

### Kanban-First Coordination

- For substantial implementation, architecture, QA, evaluation, and documentation work, create and assign a task on the existing `kami` Kanban board before execution.
- `kami-manager` primarily coordinates, obtains owner authorization, assigns specialists, synthesizes results, and verifies completion evidence. It may perform small mechanical actions, short investigations, and read-only audits directly when a Kanban task would add no value.
- Each Kanban task must identify its owner profile, bounded scope, expected deliverable, verification requirements, applicable approval restrictions, and a Jira issue key when relevant. Jira references provide traceability; do not duplicate Jira's backlog, acceptance criteria, or status model.
- Record actual execution through Kanban worker runs. Do not create retrospective tasks or attribute direct work to historical Kanban workers.
- Prefer `kami-senior-dev` for implementation, `kami-qa` for testing, `kami-architect` for architecture, and `kami-evaluator` for independent review when those roles add value; do not require every task to use every profile.
- Verify worker results and available artifacts before reporting completion.
- Kanban assignment is coordination only, not authorization to edit repository files, Jira, Obsidian, or Git. Preserve the single-stage `Approve`/`Deny` repository workflow and separate repository, Jira, Obsidian, and Git authorizations.

### Git Branch Policy

- Determine the current working Git branch dynamically using `git branch --show-current` before making repository changes.
- Perform authorized repository file changes on the currently checked-out Git branch.
- Do not assume `main` is always the working branch.
- Do not automatically create, switch, or merge Git branches without explicit owner authorization.
- If the repository is in detached HEAD state or the working branch cannot be determined, stop and ask for instructions.
- When using a separate Git worktree, verify the active branch in that specific worktree.

## Agent Collaboration
- Project Manager coordinates task assignments and progress.
- Product Analyst defines requirements and acceptance criteria.
- Architect designs technical solutions and interfaces.
- Senior Developer implements complex features.
- Developer implements assigned features and fixes.
- QA Engineer validates behavior and identifies defects.
- Evaluator independently reviews deliverables.

## Completion Requirements
The shared Definition of Done for coding issues is:

1. Acceptance criteria are satisfied.
2. The implementation follows approved requirements and architecture.
3. Relevant automated tests pass.
4. Code quality, error handling, and security are reviewed as applicable.
5. The agent summarizes completed changes and reports relevant verification results. No code diff or post-implementation owner approval is required.
6. Required technical documentation is updated with authorization.
7. The Jira issue and GitHub PR are linked where applicable.
