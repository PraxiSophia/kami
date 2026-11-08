#!/usr/bin/env bash
set -euo pipefail
SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
REPO_ROOT="$(cd -- "$SCRIPT_DIR/../.." && pwd -P)"
[[ -f "$REPO_ROOT/Cargo.toml" && -f "$REPO_ROOT/crates/kami-runtime/src/transport_tests.rs" ]] || { printf 'ERROR: repository root not found from %s\n' "$SCRIPT_DIR" >&2; exit 1; }
printf '%s\n' 'OWNER ACTION ONLY — type at the endpoint terminal, never here or into another process.' 'After BOTH endpoints print LOCAL_VERIFIED_DURABLE, at endpoint A type: quic' 'Paste endpoint B LISTEN address for QUIC when prompted. Expect AUTHENTICATED_REMOTE_PEER=<peer-id> TRANSPORT=quic.' 'Do not claim success from a printed address alone; the initiating endpoint must print AUTHENTICATED_REMOTE_PEER and the selected TRANSPORT.'
