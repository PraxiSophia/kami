# kami-key-provider

## Overview

- **Crate Name**: `kami-key-provider`
- **Simple Purpose**: A clean, secure interface for supplying the secret encryption key needed to open Kami's local database. It ensures that encryption keys are zeroed out of computer memory when no longer needed.
- **Problem it Solves in Kami**: Kami encrypts all stored messages using SQLCipher. However, the database layer should not know *how* or *where* the operating system stores secret keys (e.g., Linux Secret Service, macOS Keychain, or Windows Credential Manager). `kami-key-provider` acts as a clean bridge: [[kami-storage]] requests a key, and [[kami-identity]] provides it.
- **Dependencies**: Only `zeroize` (version 1.9.1, used to securely wipe key bytes from RAM). No database, network, or async runtime dependencies.
- **Requirements & Jira Traceability**: Implements the Milestone 1 secure key custody contract for SQLCipher database encryption.
- **Git Context**: Branch `protocol-storage`, commit `732156f6f13cbbe4d897f663c1f2546182619704`.

---

## Source Code Explanation

Re-exported publicly from [lib.rs](file:///home/phu/dev/kami/crates/kami-key-provider/src/lib.rs).

### 1. Module `profile` ([crates/kami-key-provider/src/profile.rs](file:///home/phu/dev/kami/crates/kami-key-provider/src/profile.rs))

- **Struct**: `ProfileKeyId`
  - **What it represents**: A stable 128-bit profile identifier represented as 32 lowercase hex characters.
  - **Why Kami needs it**: Identifies which user profile's encryption key is being requested from the OS keyring.
  - **Methods**:
    - `pub fn from_hex(s: &str) -> Result<Self, KeyError>`: Parses a 32-character hexadecimal string.
    - `pub const fn from_u128(value: u128) -> Self`: Constructs directly from a 128-bit integer.
    - `pub fn as_str(&self) -> &str`: Returns the 32-character hex slice.

---

### 2. Module `secret` ([crates/kami-key-provider/src/secret.rs](file:///home/phu/dev/kami/crates/kami-key-provider/src/secret.rs))

- **Struct**: `DatabaseKey`
  - **What it represents**: An owned, highly protected 32-byte (256-bit) encryption key used to unlock SQLCipher.
  - **Security Sensitivity**:
    - **No Clone / No Copy**: You cannot accidentally duplicate the key in memory.
    - **Redacted Debug**: Printing this struct with `{:?}` prints `DatabaseKey([REDACTED])`—it will never leak keys into logs or error dumps.
    - **Automatic Memory Wiping**: Implements `Drop` via `zeroize::Zeroizing`. As soon as the key falls out of scope, its 32 bytes in RAM are overwritten with zeros.
  - **Methods**:
    - `pub fn from_bytes(bytes: Zeroizing<[u8; 32]>) -> Self`: Wraps an existing zeroizing buffer.
    - `pub fn with_bytes<R>(&self, f: impl FnOnce(&[u8; 32]) -> R) -> R`: Temporarily borrows the raw 32-byte key at the exact moment the database is opened. Callers are forbidden from copying or persisting this borrow.

---

### 3. Module `error` ([crates/kami-key-provider/src/error.rs](file:///home/phu/dev/kami/crates/kami-key-provider/src/error.rs))

- **Enum**: `KeyError`
  - **Variants**: `Locked` (keyring is locked), `Unavailable` (keyring service not running), `Missing` (no key found for this profile), `Corrupt` (key data is malformed or wrong size), `PermissionDenied` (OS denied access), `Other` (generic failure).
  - **Security Sensitivity**: Simple unit variants that contain no raw error strings or native system messages, preventing sensitive information leaks.

---

### 4. Trait `KeyProvider` ([crates/kami-key-provider/src/lib.rs](file:///home/phu/dev/kami/crates/kami-key-provider/src/lib.rs))

- **Trait**: `KeyProvider: Send + Sync`
  - **What it is**: The contract that any key-storage system must implement.
  - **Methods**:
    - `fn load_database_key(&self, profile: &ProfileKeyId) -> Result<DatabaseKey, KeyError>`
      - **What it does**: Blocking call that queries the underlying secure storage and returns the 32-byte encryption key.
      - **Security Sensitivity**: Must be called on a dedicated blocking thread, never on an async network event loop.

---

### 5. Module `test_provider` ([crates/kami-key-provider/src/test_provider.rs](file:///home/phu/dev/kami/crates/kami-key-provider/src/test_provider.rs))

- **Struct**: `TestKeyProvider`
  - **What it represents**: A test-only in-memory key provider used in unit and integration test suites.
  - **Features**: Allows simulating simulated key loss, corrupted keys, locked keyrings, and fault injection to ensure error handling works properly.

---

## Completion and Verification

- **Milestone 1 Completion Estimate**: **100%**
- **Whole-Project Completion Estimate**: **100%** (for this interface crate)
- **Status Breakdown**:
  - **Implemented Behavior**: Complete trait interface, zeroizing 32-byte secret wrapper, hex profile identifiers, typed error categories, and test fixture provider.
  - **Tested Behavior**: 100% unit test coverage for redaction, non-clone properties, memory zeroization, fault injection, and multi-threaded key loading.
  - **Missing Behavior**: None within this crate. Concrete implementations live in [[kami-identity]].
  - **Known Limitations**: Does not generate keys or communicate with the OS directly (intentional separation of concerns).

### Exact Verification Commands

```sh
# Run all tests in kami-key-provider with all features enabled
cargo test -p kami-key-provider --all-features

# Check formatting and strict clippy lints
cargo fmt -p kami-key-provider -- --check
cargo clippy -p kami-key-provider --all-targets --all-features -- -D warnings
```
