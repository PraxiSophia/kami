#!/usr/bin/env bash
set -euo pipefail
SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
REPO_ROOT="$(cd -- "$SCRIPT_DIR/../.." && pwd -P)"
[[ -f "$REPO_ROOT/Cargo.toml" && -f "$REPO_ROOT/crates/kami-runtime/src/transport_tests.rs" ]] || { printf 'ERROR: repository root not found from %s\n' "$SCRIPT_DIR" >&2; exit 1; }
printf '%s\n' 'OWNER ACTION ONLY — this helper does not type into either endpoint.' 'For each endpoint, copy its INVITATION hex to the other endpoint using the existing prompt.' 'Read LOCAL_FINGERPRINT locally; compare the full values via a separate trusted channel (not the invitation or this terminal).' 'At the remote fingerprint prompt, type that independently obtained full value, or REJECT.' 'Only after the independent comparison matches, type VERIFY locally at the explicit confirmation prompt. Otherwise type REJECT.' 'Wait until BOTH terminals print LOCAL_VERIFIED_DURABLE before any dialing.'
