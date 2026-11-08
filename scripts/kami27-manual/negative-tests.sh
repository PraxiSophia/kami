#!/usr/bin/env bash
set -euo pipefail
SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
REPO_ROOT="$(cd -- "$SCRIPT_DIR/../.." && pwd -P)"
[[ -f "$REPO_ROOT/Cargo.toml" && -f "$REPO_ROOT/crates/kami-runtime/src/transport_tests.rs" ]] || { printf 'ERROR: repository root not found from %s\n' "$SCRIPT_DIR" >&2; exit 1; }
printf '%s\n' 'OWNER ACTION ONLY — no negative case is automated by this helper.' 'At the endpoint fingerprint prompt, type REJECT; expect “operator rejected; no trust persisted”.' 'Do not type VERIFY for an unverified identity.' 'The two-endpoint harness cannot assert a wrong-Peer socket negative: with only two endpoints, the operator can simply verify the actual peer.' 'The existing automated strict socket-negative fixture uses three identities: kami-transport/tests/direct.rs. This helper does not run it.'
