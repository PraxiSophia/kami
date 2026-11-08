#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
REPO_ROOT="$(cd -- "$SCRIPT_DIR/../.." && pwd -P)"
[[ -f "$REPO_ROOT/Cargo.toml" && -f "$REPO_ROOT/crates/kami-runtime/src/transport_tests.rs" ]] || {
  printf 'ERROR: repository root not found from %s\n' "$SCRIPT_DIR" >&2
  exit 1
}

printf '%s\n' \
  'OWNER ACTION: in each endpoint terminal, type quit and wait for:' \
  '  CLEAN_SHUTDOWN LEASE_RELEASED' \
  'Do not confirm cleanup until both terminals displayed that exact shutdown result.'
read -r -p 'Type CLEAN_SHUTDOWN LEASE_RELEASED for endpoint A: ' endpoint_a
read -r -p 'Type CLEAN_SHUTDOWN LEASE_RELEASED for endpoint B: ' endpoint_b
if [[ "$endpoint_a" != 'CLEAN_SHUTDOWN LEASE_RELEASED' || "$endpoint_b" != 'CLEAN_SHUTDOWN LEASE_RELEASED' ]]; then
  printf 'ERROR: cleanup not confirmed; no process or file was changed.\n' >&2
  exit 1
fi

if command -v pgrep >/dev/null 2>&1; then
  remaining="$(pgrep -af 'cargo test.*manual_endpoint' || true)"
  if [[ -n "$remaining" ]]; then
    printf 'WARNING: possible manual endpoint process remains; inspect it manually:\n%s\n' "$remaining" >&2
    printf '%s\n' 'No process was signalled or killed. Cleanup is not confirmed while it remains.' >&2
    exit 1
  fi
  printf '%s\n' 'No matching manual endpoint process was detected.'
else
  printf '%s\n' 'NOTICE: pgrep is unavailable; process absence could not be checked.'
fi

printf '%s\n' 'Cleanup confirmed. The harness owns its disposable temporary resources; this helper deletes nothing and never touches personal profiles or keyrings.'
