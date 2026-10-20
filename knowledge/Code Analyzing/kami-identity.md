# kami-identity

## Overview

- **Crate Name**: `kami-identity`
- **Simple Purpose**: Manages your device's cryptographic identity (its Ed25519 digital signature keys) and securely stores them in your computer's built-in password manager/keyring (Linux Secret Service, macOS Keychain, or Windows Credential Manager).
- **Problem it Solves in Kami**: Kami needs private keys to sign messages and unlock its database. If private keys were saved in plain text files, malware or other users could easily steal them. `kami-identity` ensures keys are generated using cryptographically secure random numbers and handed directly to the operating system's secure credential vault.
- **Dependencies**: Depends internally on [[kami-key-provider]] for the database key type. Externally uses `ed25519-dalek` (v3.0.0, digital signatures), `getrandom` (v0.4.3, OS random number generator), `zeroize` (memory wiping), and platform-specific keyring libraries (`secret-service` / `zbus` on Linux, `apple-native-keyring-store` on macOS, `windows-native-keyring-store` on Windows).
- **Requirements & Jira Traceability**: Implements Milestone 1 identity and key custody boundary (KAMI-24 / KAMI-31).
- **Git Context**: Branch `protocol-storage`, commit `732156f6f13cbbe4d897f663c1f2546182619704`.

---

## Source Code Explanation

Re-exported publicly from [lib.rs](file:///home/phu/dev/kami/crates/kami-identity/src/lib.rs).

### 1. Module `store` ([crates/kami-identity/src/store.rs](file:///home/phu/dev/kami/crates/kami-identity/src/store.rs))

Defines abstract storage slots for credentials.

- **Enum**: `SlotNamespace`
  - **Variants**: `Identity` (holds the device's private signing key) and `Database` (holds the SQLCipher encryption key).
  - **Why Kami needs it**: Keeps identity keys completely separate from storage keys so they can never be confused or substituted.

- **Struct**: `KeySlot`
  - **What it represents**: A unique handle/address for a key stored in the OS keyring.
  - **Methods**:
    - `pub fn to_persistent_string(&self) -> String`: Formats the handle for storage in an enrollment record.
    - `pub fn from_persistent_string(s: &str) -> Result<Self, KeyError>`: Parses a stored handle back into a verified slot.

- **Trait**: `SecretStore: Send + Sync`
  - **What it is**: The abstraction layer for keyrings.
  - **Methods**:
    - `fn save_secret(&self, slot: &KeySlot, secret: &[u8]) -> Result<(), KeyError>`
    - `fn load_secret(&self, slot: &KeySlot) -> Result<Zeroizing<Vec<u8>>, KeyError>`
    - `fn has_secret(&self, slot: &KeySlot) -> Result<bool, KeyError>`

---

### 2. Module `enrollment` ([crates/kami-identity/src/enrollment.rs](file:///home/phu/dev/kami/crates/kami-identity/src/enrollment.rs))

Tracks the initial setup ("enrollment") of a new Kami identity.

- **Struct**: `EnrollmentRecord`
  - **What it represents**: The public metadata of an enrolled identity.
  - **Fields**: `profile_id` (128-bit ID), `identity_slot` (KeySlot handle), `database_slot` (KeySlot handle), and `public_key` (32-byte Ed25519 public key).
  - **Security Sensitivity**: Contains **zero** private keys or secrets. It can be safely stored in plain text or printed on screen.

- **Enum**: `EnrollmentState`
  - **Variants**: `Fresh` (device has no identity yet), `Pending` (keys are being generated), `Enrolled` (keys verified and ready).

- **Trait**: `EnrollmentJournal`
  - **What it is**: An interface that the background service implements to guarantee atomic transitions between enrollment states on disk.

---

### 3. Module `custody` ([crates/kami-identity/src/custody.rs](file:///home/phu/dev/kami/crates/kami-identity/src/custody.rs))

The orchestrator that generates keys and implements the `KeyProvider` trait.

- **Struct**: `Custody<S: SecretStore>`
  - **What it represents**: The active key custodian holding the profile and secret store handle.
  - **Methods**:
    - `pub fn initialize(...) -> Result<Self, KeyError>`
      - **What it does**: Generates a new 32-byte Ed25519 signing seed using the OS random number generator (`getrandom`), computes the public key, generates an independent random 32-byte database encryption key, and writes both into the OS keyring.
      - **Security Sensitivity**: Never reuses an identity key as a database key. Verifies both keys by reading them back from the OS keyring before finalizing.
      - **Status**: Implemented ([custody.rs#L40-L115](file:///home/phu/dev/kami/crates/kami-identity/src/custody.rs#L40-L115)).
    - `pub fn open_existing(...) -> Result<Self, KeyError>`
      - **What it does**: Opens an already enrolled identity, verifying that both keys exist and that the public key matches the saved record.
      - **Status**: Implemented ([custody.rs#L118-L150](file:///home/phu/dev/kami/crates/kami-identity/src/custody.rs#L118-L150)).
    - `impl<S: SecretStore> KeyProvider for Custody<S>`
      - **What it does**: Implements the [[kami-key-provider]] interface so [[kami-storage]] can retrieve the database key without knowing about OS keyrings.

---

### 4. Module `native` ([crates/kami-identity/src/native/](file:///home/phu/dev/kami/crates/kami-identity/src/native/))

Platform-specific backend implementations:
- **Linux** (`linux.rs`): Connects to Freedesktop Secret Service over D-Bus using `zbus`. Uses an encrypted Diffie-Hellman session to protect secrets in transit across D-Bus. Rejects locked collections without prompting UI.
- **macOS** (`macos.rs`): Stores secrets in the Apple Login Keychain via `security-framework`. Disables interactive UI prompts.
- **Windows** (`windows.rs`): Stores secrets in Windows Credential Manager with `Local` persistence.

---

## Completion and Verification

- **Milestone 1 Completion Estimate**: **85%**
- **Whole-Project Completion Estimate**: **Not estimable** (multi-device key syncing and contact trust graphs belong to future milestones).
- **Status Breakdown**:
  - **Implemented Behavior**: Complete three-stage enrollment lifecycle (`Fresh` -> `Pending` -> `Enrolled`), dual-namespace slot management, OS CSPRNG key generation, and native keyring drivers for Linux, macOS, and Windows.
  - **Tested Behavior**: Linux is fully verified via unit tests, absent-slot probes, and a disposable GNOME Keyring daemon integration test. macOS and Windows targets are cross-compiled and strictly verified with Clippy.
  - **Missing Behavior**: Native host execution tests on real Windows and macOS machines (cross-compilation verified, but native runtime qualification is an M1 release gate).
  - **Known Limitations**: Does not sign messages or perform Double Ratchet key exchanges (those belong to protocol encryption).

### Exact Verification Commands

```sh
# Run all standard tests in kami-identity
cargo test -p kami-identity

# Cross-compile check for Windows and macOS targets
cargo check -p kami-identity --all-targets --target x86_64-pc-windows-msvc
cargo check -p kami-identity --all-targets --target x86_64-apple-darwin

# Run Linux native headless D-Bus probe (requires gnome-keyring-daemon and dbus-run-session)
cargo test -p kami-identity --test linux_collection -- --ignored --nocapture
```
