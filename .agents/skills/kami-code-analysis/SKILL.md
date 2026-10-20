---
name: kami-code-analysis
description: Comprehensive workflow and procedures for analyzing Kami Rust source code, tracking crate progress, and generating beginner-friendly Obsidian documentation in both initial and incremental modes.
---

# Kami Code Analysis Skill

This skill guides the analysis of Rust crates within the Kami workspace and the generation of structured, beginner-friendly Obsidian documentation under `knowledge/Code Analyzing/`.

## Workflow Overview

```
                      +-----------------------------+
                      |   Check Existing Metadata   |
                      | (Code Analysis Home.md exists?)
                      +--------------+--------------+
                                     |
                     +---------------+---------------+
                     |                               |
              [No / Baseline reset]          [Yes / Valid Metadata]
                     |                               |
                     v                               v
             +---------------+               +---------------+
             |    MODE A     |               |    MODE B     |
             | Initial Full  |               |  Incremental  |
             |   Analysis    |               |    Update     |
             +---------------+               +---------------+
```

---

## Mode A: Initial Full Workspace Analysis

Execute Mode A on initial setup or when resetting the baseline:

1. **Repository Context Discovery**:
   - Determine current branch: `git branch --show-current`.
   - Determine current commit: `git rev-parse HEAD`.
   - Check working tree status: `git status --short`.
2. **Crate Discovery**:
   - Read workspace members from root `Cargo.toml`.
   - Inspect each crate's `Cargo.toml`, `README.md`, `src/`, and `tests/`.
3. **Requirements & Architecture Baseline**:
   - Read approved requirements in `docs/Kami_Project_Requirements.md` and `docs/Kami_Communication_Design.md`.
   - Read relevant architecture documents in `docs/architecture/` (`Identity_And_Encryption.md`, `Messaging_Protocol.md`, `Rust_Architecture_Decisions.md`, etc.).
   - Read execution plans in `docs/planning/` (`Master_Execution_Roadmap.md`, `Test_And_Validation_Plan.md`).
4. **Code Structure Analysis**:
   - For every crate, parse modules, traits, structs, enums, standalone functions, and methods.
   - Categorize status (Implemented, Partial/Incomplete, Stub).
   - Evaluate against Milestone 1 criteria and intended contracts.
5. **Approval & Documentation Generation**:
   - Formulate proposed files under `knowledge/Code Analyzing/`:
     - `Code Analysis Home.md`
     - `<crate-name>.md` for each workspace crate.
   - Present proposed files to user and request approval via `AGENTS.md` single-stage `Approve`/`Deny` menu before writing.
   - Write files upon approval.

---

## Mode B: Incremental Updates

Execute Mode B for ongoing maintenance when code has changed:

1. **Baseline Detection**:
   - Determine active Git branch: `git branch --show-current`.
   - Read `last_analyzed_commit` and `last_analyzed_branch` from YAML frontmatter in `knowledge/Code Analyzing/Code Analysis Home.md`.
   - If the baseline is missing or the branch has changed significantly, notify the user and handle branch reconciliation or fall back to Mode A.
2. **Diff & Impact Identification**:
   - Identify changes since baseline commit:
     - Committed changes: `git diff --name-status <baseline_commit>..HEAD`
     - Staged & unstaged changes: `git diff --name-status HEAD` and `git status --porcelain`
   - Map modified, added, renamed, or deleted files to their respective crates and modules.
   - If no Rust code, tests, or requirements changed, report that the documentation is up to date without making unnecessary edits.
3. **Targeted Reanalysis**:
   - Re-analyze only affected source files, types, and functions.
   - Check if changes alter public interfaces affecting dependent crates.
   - Update unit/integration test status and execution commands.
4. **Documentation Updates**:
   - Propose updates to affected `<crate-name>.md` files and `Code Analysis Home.md`.
   - Request approval per `AGENTS.md`.
   - Apply updates:
     - Revise descriptions of modified types and functions.
     - Add new structures and remove references to deleted code.
     - Update progress estimates and test results.
     - Update `last_analyzed_commit` to current HEAD (or working tree state hash).

---

## Documentation Structure & Note Format

### 1. `Code Analysis Home.md`
Located at `knowledge/Code Analyzing/Code Analysis Home.md`:
- **Frontmatter**:
  ```yaml
  ---
  last_analyzed_branch: "<branch-name>"
  last_analyzed_commit: "<commit-sha>"
  last_analyzed_timestamp: "<iso8601-date>"
  milestone_1_completion_overall: "<percentage>%"
  whole_project_completion_overall: "Not estimable" # or percentage if roadmap allows
  ---
  ```
- **Contents**:
  - Executive summary of the codebase for beginners.
  - Overall progress against Milestone 1 (Delivery Protocol, Identity Custody, Storage Engine, CLI).
  - Workspace Crates Table: Crate Name (linked to `[[<crate-name>]]`), Purpose, Milestone 1 Status, Tested Status.
  - Dependency Graph (Mermaid diagram showing crate dependencies).
  - Instructions on navigating notes and running verification tests.

### 2. Crate Notes (`<crate-name>.md`)
Located at `knowledge/Code Analyzing/<crate-name>.md`:

#### A. Overview
- **Crate Name**: Display exact crate name.
- **Purpose**: Clear, beginner-friendly explanation of what the crate does in Kami.
- **Problem Solved**: The real-world or architectural problem this crate addresses.
- **Dependencies**: Workspace crates and key external crates it relies on.
- **Requirements & Jira Traceability**: Which Milestone 1 requirements or Jira issues it addresses.
- **Git Context**: Current Git branch and analyzed commit SHA.

#### B. Source Code Explanation
Use hierarchical Markdown bullet points. Explain technical terms simply.
Group by module, then type:

- `module_name` (`crates/<crate>/src/<file>.rs`)
  - **Struct / Enum / Trait**: `TypeName`
    - **What it represents**: Plain English explanation.
    - **Why Kami needs it**: Architectural role.
    - **Key fields / variants**: Clear explanation of each significant field/variant.
    - **Relationships**: How it connects to other types in this or other crates.
    - **Methods & Associated Functions**:
      - `fn method_name(&self, arg: Type) -> Result<Ret, Error>`:
        - **What it does**: 1-2 sentence beginner-friendly summary.
        - **Inputs & Outputs**: Plain-language explanation of parameters and return value.
        - **Errors**: Notable failure conditions.
        - **Security Sensitivity**: Note zeroization, cryptographic boundaries, or secret handling if applicable.
        - **Status**: Implemented / Partial / Stub.
        - **Source Reference**: `[crates/.../file.rs#L...](file:///home/phu/dev/kami/crates/...)`
  - **Standalone Functions**:
    - Documented under their parent module with the same function detail format.

#### C. Completion and Verification
- **Milestone 1 Completion Estimate**: Evidence-based percentage with breakdown.
- **Whole-Project Completion Estimate**: Percentage if documented in roadmap, or `Not estimable`.
- **Status Breakdown**:
  - Implemented behavior (what works today).
  - Tested behavior (what is verified by automated tests).
  - Missing behavior (features planned for M1 but not yet built).
  - Known limitations (stubs, temporary mock providers, missing persistence).
- **Verification Commands**: Exact `cargo test -p <crate-name>` or clippy commands to run.

---

## Scoring & Progress Rubric

Never guess completion percentages based on line counts or compilation success.
Use this weighted rubric:

1. **Contract Completeness (50%)**:
   - Are the core data models, traits, state transitions, and business logic implemented?
   - Stubbed functions (`todo!()`, `unimplemented!()`, dummy returns) count as 0%.
2. **Automated Test Coverage (30%)**:
   - Are there unit tests verifying valid, edge, and error conditions?
   - Are there integration tests verifying end-to-end workflows within crate scope?
3. **Robustness & Error Handling (10%)**:
   - Are domain errors defined, handled without unwrap/panics, and properly propagated?
4. **Integration Readiness (10%)**:
   - Are public interfaces aligned with dependent crates and workspace requirements?

If full-project scope is undefined, always use `Not estimable` for the whole-project field.

---

## Safety & Style Directives
- **Zero Secrets**: Never print, document, or log real private keys, passwords, or seed phrases.
- **No Source Edits**: Never touch `*.rs`, `Cargo.toml`, or source code during code analysis.
- **Approval First**: Always follow `AGENTS.md` approval protocol before writing Obsidian files.
- **Beginner-Friendly**: Avoid dense compiler/type theory jargon unless explained gently first.
