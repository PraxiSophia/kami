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

Do not assume proposed features are approved requirements.

## Project Scope
Repository: /home/phu/dev/kami
GitHub: https://github.com/PraxiSophia/kami
Jira project: KAMI
Jira site: https://pql7227.atlassian.net

Only work on Kami when operating under a kami-* profile.
Do not modify unrelated repositories or projects.

## Development Rules
- Use Jira to track planned work.
- Use GitHub branches and pull requests for implementation.
- Do not push directly to main.
- Do not merge pull requests without authorization.
- Keep implementation aligned with approved requirements.
- Add tests for new behavior and bug fixes.
- Document architectural decisions and protocol changes.
- Never commit credentials, tokens, or secrets.
- Report blockers rather than silently changing requirements.

## Agent Collaboration
- Project Manager coordinates task assignments and progress.
- Product Analyst defines requirements and acceptance criteria.
- Architect designs technical solutions and interfaces.
- Senior Developer implements complex features.
- Developer implements assigned features and fixes.
- QA Engineer validates behavior and identifies defects.
- Evaluator independently reviews deliverables.

## Completion Requirements
A development task is complete only when:
1. Its acceptance criteria are satisfied.
2. Relevant tests pass.
3. Documentation is updated when needed.
4. The implementation is reviewed.
5. Its Jira issue and GitHub PR are linked.
