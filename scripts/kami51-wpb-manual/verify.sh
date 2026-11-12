#!/usr/bin/env bash
# Test-only owner endpoint or explicitly separate automated fixtures.
set -euo pipefail

usage() {
  printf '%s\n' \
    'Usage: bash verify.sh [--check|--endpoint|--fixtures|--help]' \
    '--check     Compile runtime test interfaces; no endpoint/keyring starts (default).' \
    '--endpoint  Launch one isolated ignored manual endpoint; run separately in two terminals.' \
    '--fixtures  Run existing automated tests, including isolated native loopback engines.' \
    'Only the owner may compare fingerprints and type VERIFY/REJECT; no mode automates trust.'
}
if (( $# > 1 )); then usage >&2; exit 2; fi
mode="${1:---check}"
case "$mode" in
  --help) usage; exit 0 ;;
  --check|--endpoint|--fixtures) ;;
  *) usage >&2; exit 2 ;;
esac

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
REPO_ROOT="$(cd -- "$SCRIPT_DIR/../.." && pwd -P)"
for file in Cargo.toml Cargo.lock crates/kami-runtime/src/transport_tests.rs \
  crates/kami-e2ee/tests/prekeys.rs crates/kami-protocol/tests/prekey_exchange.rs \
  crates/kami-storage/tests/authenticated_boundary.rs; do
  if [[ ! -f "$REPO_ROOT/$file" ]]; then
    printf 'ERROR: incomplete Kami checkout: %s\n' "$file" >&2
    exit 1
  fi
done
if [[ "$(uname -s)" != Linux ]]; then
  printf 'ERROR: native endpoint/fixture is Linux-only.\n' >&2
  exit 1
fi
for tool in cargo git dbus-run-session gnome-keyring-daemon; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    printf 'ERROR: missing %s; nothing will be installed automatically.\n' "$tool" >&2
    exit 1
  fi
done
cd -- "$REPO_ROOT"
if [[ "$(git rev-parse --show-toplevel)" != "$REPO_ROOT" ]]; then
  printf 'ERROR: script is not in the resolved Kami worktree root.\n' >&2
  exit 1
fi
printf 'Repository: %s\n' "$REPO_ROOT"
printf '%s\n' 'WP-B ignored-test interface only; owner manual acceptance is NOT asserted.' \
  'Automated test assertions are not owner fingerprint verification or manual acceptance.'
if [[ "$mode" == --check ]]; then
  cargo check --locked --offline -p kami-runtime --tests
  printf '%s\n' 'CHECK ONLY: compilation passed; no endpoint or native keyring was started.'
  exit 0
fi

# Never inherit fixture child markers into the owner endpoint launcher.
if [[ "$mode" == --endpoint ]]; then
  unset KAMI27_MANUAL_PRIVATE_BUS KAMI_RUNTIME_PRIVATE_BUS
  printf '%s\n' 'OWNER ENDPOINT: independent full-fingerprint comparison and local VERIFY/REJECT required.'
  exec cargo test --locked --offline -p kami-runtime transport_tests::native::manual_endpoint -- --exact --ignored --nocapture
fi

# Force the existing native fixture through its private D-Bus/HOME launcher.
# An inherited child marker must never redirect provisioning to a desktop bus.
unset KAMI_RUNTIME_PRIVATE_BUS
printf '%s\n' 'AUTOMATED FIXTURES: simulated trust; disposable native profiles; no application messages.'
cargo test --locked --offline -p kami-protocol --test prekey_exchange
cargo test --locked --offline -p kami-e2ee --test prekeys
cargo test --locked --offline -p kami-storage --test prekeys
cargo test --locked --offline -p kami-storage --test authenticated_boundary authenticated_positive_boundary -- --exact --nocapture
cargo test --locked --offline -p kami-runtime --lib transport_tests::native::supervised_native_transport_preserves_lease_and_never_delivers_payloads -- --exact --nocapture
printf '%s\n' 'AUTOMATED FIXTURES completed successfully; WP-B OWNER MANUAL RESULT: NOT RUN by this mode.'
