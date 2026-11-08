#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
REPO_ROOT="$(cd -- "$SCRIPT_DIR/../.." && pwd -P)"
if [[ ! -f "$REPO_ROOT/Cargo.toml" || ! -f "$REPO_ROOT/crates/kami-runtime/src/transport_tests.rs" ]]; then
  printf 'ERROR: cannot resolve Kami repository from %s\n' "$SCRIPT_DIR" >&2
  exit 1
fi
cd -- "$REPO_ROOT"

for tool in cargo git dbus-run-session gnome-keyring-daemon; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    printf 'ERROR: required command not found: %s (install nothing automatically)\n' "$tool" >&2
    exit 1
  fi
done
if [[ "$(uname -s)" != Linux ]]; then
  printf 'ERROR: this owner-operated harness is currently Linux-only.\n' >&2
  exit 1
fi
if ! git rev-parse --show-toplevel >/dev/null 2>&1; then
  printf 'ERROR: resolved path is not inside a Git worktree.\n' >&2
  exit 1
fi
if ! grep -Fq 'fn manual_endpoint()' crates/kami-runtime/src/transport_tests.rs || ! grep -Fq 'LOCAL_VERIFIED_DURABLE' crates/kami-runtime/src/transport_tests.rs; then
  printf 'ERROR: expected KAMI-27 manual harness symbols are missing.\n' >&2
  exit 1
fi
printf 'Checking the existing ignored harness and native dependencies without running it...\n'
cargo check --locked -p kami-runtime --tests
printf 'Prerequisites and harness check passed. Repository: %s\n' "$REPO_ROOT"
printf 'Each endpoint must run start-a.sh or start-b.sh in its own terminal. No keys, profiles, or keyrings are touched by setup.\n'
