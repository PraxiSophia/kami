#!/usr/bin/env bash
set -euo pipefail
SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
REPO_ROOT="$(cd -- "$SCRIPT_DIR/../.." && pwd -P)"
[[ -f "$REPO_ROOT/Cargo.toml" && -f "$REPO_ROOT/crates/kami-runtime/src/transport_tests.rs" ]] || { printf 'ERROR: repository root not found from %s\n' "$SCRIPT_DIR" >&2; exit 1; }
for tool in cargo dbus-run-session gnome-keyring-daemon; do command -v "$tool" >/dev/null 2>&1 || { printf 'ERROR: required command not found: %s\n' "$tool" >&2; exit 1; }; done
[[ "$(uname -s)" == Linux ]] || { printf 'ERROR: harness is Linux-only.\n' >&2; exit 1; }
printf '%s\n' 'AUTOMATIC: starting the existing ignored test harness only.' 'OWNER: follow prompts; compare fingerprints over an independent trusted channel and type decisions locally. Never paste secrets.'
cd -- "$REPO_ROOT"
exec cargo test --locked -p kami-runtime transport_tests::native::manual_endpoint -- --exact --ignored --nocapture
