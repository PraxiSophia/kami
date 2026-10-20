# kami-cli

## Overview

- **Crate Name**: `kami-cli`
- **Simple Purpose**: The user-facing command-line tool for Kami. In the future, this is what you will run in your terminal to initialize your profile, invite friends, send and receive encrypted messages, and manage the background daemon.
- **Problem it Solves in Kami**: Most users and developers interact with Kami via terminal commands or scripts. `kami-cli` will provide an easy-to-use interface that speaks to the background Kami service over a secure local connection (Unix domain sockets or Windows named pipes).
- **Dependencies**: Currently references [[kami-core]] and [[kami-protocol]]. Production CLI will also integrate `clap` for argument parsing and Tokio for local IPC.
- **Requirements & Jira Traceability**: Implements the user-facing CLI and background service daemon outlined in Milestone 1 and `docs/planning/CLI_Implementation_Plan.md`.
- **Git Context**: Branch `protocol-storage`, commit `732156f6f13cbbe4d897f663c1f2546182619704`.

---

## Source Code Explanation

### 1. Module `main` ([crates/kami-cli/src/main.rs](file:///home/phu/dev/kami/crates/kami-cli/src/main.rs))

This crate is currently in its initial **scaffolding phase**.

- **Standalone Function**: `fn main()`
  - **What it does**: Entry point of the executable. Currently exercises the workspace dependency boundary by referencing `kami_core::MessageId` and `kami_protocol::M1_PROTOCOL_VERSION`.
  - **Inputs & Outputs**: Takes no arguments; returns `()`.
  - **Status**: Scaffold / Minimal Stub ([main.rs#L3-L8](file:///home/phu/dev/kami/crates/kami-cli/src/main.rs#L3-L8)).

- **Test Module**: `mod tests`
  - `fn cli_scaffold_exposes_the_protocol_boundary()`: Asserts that `M1_PROTOCOL_VERSION == 1` ([main.rs#L10-L16](file:///home/phu/dev/kami/crates/kami-cli/src/main.rs#L10-L16)).

---

## Planned Architecture (From Approved Roadmap)

Per [CLI_Implementation_Plan.md](file:///home/phu/dev/kami/docs/planning/CLI_Implementation_Plan.md), the full implementation will feature:

1. **Local IPC Client**: Communicates with the daemon over local Unix domain sockets (Linux/macOS) or Named Pipes (Windows). No TCP ports are opened by default.
2. **Commands**:
   - `kami init`: Initialize profile and generate public keys.
   - `kami invite export` / `import`: Exchange contact invitations.
   - `kami verify <fingerprint>`: Verify contact security fingerprints.
   - `kami send --to <contact> --message <text>`: Enqueue and send an encrypted message.
   - `kami queue`: Inspect pending, in-flight, delivered, and blocked messages.
   - `kami cancel <message-id>`: Cancel an undelivered queued message.
   - `kami history`: View decrypted message logs.
   - `kami service start|stop|status`: Manage background daemon process.

---

## Completion and Verification

- **Milestone 1 Completion Estimate**: **15%** (Scaffolding only)
- **Whole-Project Completion Estimate**: **Not estimable**
- **Status Breakdown**:
  - **Implemented Behavior**: Workspace binary build and core/protocol dependency integration.
  - **Tested Behavior**: Scaffold assertion test.
  - **Missing Behavior**: Full CLI argument parser (`clap`), IPC daemon, background service lifecycle, command handlers, and JSON output formatting.
  - **Known Limitations**: Only a placeholder binary currently exists.

### Exact Verification Commands

```sh
# Run CLI scaffold tests
cargo test -p kami-cli

# Run CLI binary build check
cargo build -p kami-cli
```
