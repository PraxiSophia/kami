# kami-key-provider

Small synchronous, injected database-key custody contract. Dependencies: only
`zeroize = 1.9.1`; no OS, SQL, protocol, or async runtime dependency.

## Modules and API

- `profile`: `ProfileKeyId`, an explicit stable 128-bit namespace represented as
  exactly 32 lowercase hex digits. Generate it randomly at service enrollment;
  persist it, never substitute a path, display name, or implicit default.
- `secret`: `DatabaseKey`, a non-Clone/non-Copy, redacted, drop-zeroizing 32-byte
  owned key. `from_bytes` takes `Zeroizing<[u8; 32]>`; `with_bytes` borrows at the
  SQLCipher keying boundary. Callers must not copy/log/export that borrow.
- `error`: `KeyError::{Locked, Unavailable, Missing, Corrupt, PermissionDenied,
  Other}`. Unit variants retain no native messages or error sources.
- Root `KeyProvider: Send + Sync`: object-safe, blocking, **load only**
  `load_database_key(&ProfileKeyId) -> Result<DatabaseKey, KeyError>`.
- Opt-in `test_provider`: seeded in-memory fixture with per-profile queued
  one-call faults, synchronized counters and simulated key loss. A clone shares
  its slot map to simulate reopening. It never generates keys on load. Restrict
  this feature to dev-dependencies/tests; there is no production fallback or
  runtime environment/config selector for it.

`kami-storage -> kami-key-provider <- kami-identity`. No identity secrets are
available through this trait. OS providers must validate retrieved byte lengths
and zeroize temporary buffers. Wrong-key SQLCipher failures remain storage errors;
never reinterpret them as permission to generate a replacement key.

## Blocking and secrecy rules

Call on the service's bounded blocking initialization/storage worker, never on a
network executor or while a database transaction is open. Key errors fail closed.
Confirmed missing credentials require explicit recovery/re-enrollment, not a blank
database, plaintext fallback, or silent key rotation. Zeroization is best-effort
memory hygiene, not protection from native copies, crash dumps or a compromised
unlocked endpoint.

## Verified commands

From repository root (after serialized workspace/lockfile integration):

```sh
cargo check --locked -p kami-key-provider --all-targets --all-features
cargo build --locked -p kami-key-provider --all-features
cargo test --locked -p kami-key-provider --all-features
cargo fmt -p kami-key-provider -- --check
cargo clippy --locked -p kami-key-provider --all-targets --all-features -- -D warnings
```

Tests cover validation, redaction, compile-time non-Clone/non-Copy contracts,
load-only seeded fixtures, every typed fault, simulated restart/loss, independent
slots and concurrent call accounting. An in-memory fixture is not OS durability
qualification.
