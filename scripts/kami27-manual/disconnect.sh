#!/usr/bin/env bash
set -euo pipefail
SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
REPO_ROOT="$(cd -- "$SCRIPT_DIR/../.." && pwd -P)"
[[ -f "$REPO_ROOT/Cargo.toml" && -f "$REPO_ROOT/crates/kami-runtime/src/transport_tests.rs" ]] || { printf 'ERROR: repository root not found from %s\n' "$SCRIPT_DIR" >&2; exit 1; }
printf '%s\n' 'OWNER ACTION ONLY — type at the endpoint terminal, never here or into another process.' 'At the endpoint that owns the active connection, type: disconnect' 'Expect DISCONNECTED: old capability revoked.' 'Wait for both endpoints to finish disconnecting before selecting the next transport or reconnecting.'
