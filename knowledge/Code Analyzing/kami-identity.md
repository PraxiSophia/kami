# kami-identity

## Overview

- **Crate Name**: `kami-identity`
- **Simple Purpose**: Manages your device's cryptographic identity (its Ed25519 digital signature keys), securely stores them in your computer's built-in password manager/keyring (Linux Secret Service, macOS Keychain, or Windows Credential Manager), and handles contact verification fingerprints, Peer-ID bindings, and Olm prekey signatures.
- **Problem it Solves in Kami**: Kami needs private keys to sign messages and unlock its database. If private keys were saved in plain text files, malware or other users could easily steal them. Furthermore, Kami needs to guarantee that a network peer is truly who they claim to be: `kami-identity` binds device keys to libp2p `PeerId`s, generates human-verifiable security fingerprints, enforces bilateral contact trust transitions, and signs Olm prekey bundles without ever exposing private identity keys.
- **Dependencies**: Depends internally on [[kami-key-provider]] for the database key type. Externally uses `ed25519-dalek` (v3.0.0, digital signatures), `getrandom` (v0.4.3, OS random number generator), `zeroize` (memory wiping), `sha2` (v0.11.1, fingerprint hashing), `libp2p-identity` (v0.3.0, PeerId encodings), and platform-specific keyring libraries (`secret-service` / `zbus` on Linux, `apple-native-keyring-store` on macOS, `windows-native-keyring-store` on Windows).
- **Requirements & Jira Traceability**:
  - **KAMI-23** (Protected identity initialization and peer trust): **Functionally implemented in the Rust codebase.** Jira reports its workflow status as *In Progress* because native Windows/macOS host qualification remains pending, not because core Rust behavior is missing.
  - **KAMI-50** (Contact fingerprint verification and identity trust binding): Fully implemented and tested. Includes bilateral verification, trust transitions, and device-to-Peer-ID binding.
  - **KAMI-51 (WP2-A1)**: Signed device-to-Olm account/prekey binding (`olm_binding`), linking identity custody to [[kami-e2ee]].
- **Git Context**: Branch `identity-security`, commit `eedbb744565cb8af6c80c78b1b76b431c7bf0562`.

---

## Source Code Explanation

Re-exported publicly from [lib.rs](file:///home/phu/dev/kami/crates/kami-identity/src/lib.rs).

### 1. Module `store` ([crates/kami-identity/src/store.rs](file:///home/phu/dev/kami/crates/kami-identity/src/store.rs))

Defines abstract storage slots for credentials.

- **Enum**: `KeyPurpose` (previously `SlotNamespace`)
  - **Variants**: `Identity` (holds the device's private signing key) and `Database` (holds the SQLCipher encryption key).
  - **Why Kami needs it**: Keeps identity keys completely separate from database storage keys so they can never be confused or substituted.

- **Struct**: `KeySlot`
  - **What it represents**: A unique handle/address for a key stored in the OS keyring.
  - **Methods**:
    - `pub fn to_persistent_string(&self) -> String`: Formats the handle for storage in an enrollment record.
    - `pub fn from_persistent_string(s: &str) -> Result<Self, KeyError>`: Parses a stored handle back into a verified slot.

- **Trait**: `SecretStore: Send + Sync`
  - **What it is**: The abstraction layer for keyrings.
  - **Methods**: `save_secret`, `load_secret`, and `has_secret`.

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

The orchestrator that generates keys, signs protocol bindings, and implements the `KeyProvider` trait.

- **Struct**: `Custody<S: SecretStore>`
  - **What it represents**: The active key custodian holding the profile and secret store handle.
  - **Core Methods**:
    - `pub fn initialize(...) -> Result<Self, KeyError>`: Generates an Ed25519 signing seed via OS CSPRNG (`getrandom`), computes the public key, generates a separate random 32-byte database encryption key, and writes both into the OS keyring.
    - `pub fn open_existing(...) -> Result<Self, KeyError>`: Opens an enrolled identity, verifying that both keys exist and that the public key matches the saved record.
    - `impl<S: SecretStore> KeyProvider for Custody<S>`: Implements the [[kami-key-provider]] interface so [[kami-storage]] can retrieve the database key without knowing about OS keyrings.
  - **Signature Methods (KAMI-50 & KAMI-51 WP2-A1)**:
    - `pub fn create_peer_binding(&self, record: &EnrollmentRecord, public_key_protobuf: &[u8], peer_id: &PeerId) -> Result<PeerBindingProof, IdentityError>`: Cryptographically signs a binding transcript connecting the device key to a libp2p `PeerId`.
    - `pub fn create_olm_binding(&self, record: &EnrollmentRecord, peer_binding: &PeerBindingProof, data: &OlmBindingData) -> Result<OlmBindingProof, IdentityError>`: Signs a fixed transcript binding the device to selected Olm account and prekey material. Private keys are never exported.

---

### 4. Module `fingerprint` ([crates/kami-identity/src/fingerprint.rs](file:///home/phu/dev/kami/crates/kami-identity/src/fingerprint.rs))

Implements human-verifiable security fingerprints (KAMI-50).

- **Struct**: `DeviceFingerprint`
  - **What it represents**: A SHA-256 hash formatted for human visual verification:
    $$\text{SHA-256}(b"\text{kami-device-fingerprint-v1}\backslash 0" \parallel \text{public\_key\_bytes})$$
  - **Display Format**: Rendered as eight lowercase groups of eight hexadecimal characters separated by spaces (e.g., `126dea50 6043ab05 ...`).
  - **Method**: `pub fn parse_display(s: &str) -> Result<Self, IdentityError>`: Parses and validates the full formatted fingerprint. Abbreviations are strictly rejected.

---

### 5. Module `binding` ([crates/kami-identity/src/binding.rs](file:///home/phu/dev/kami/crates/kami-identity/src/binding.rs))

Cryptographic proof binding an Ed25519 device key to a libp2p network `PeerId` (KAMI-50).

- **Struct**: `PeerBindingProof`
  - **What it represents**: A 64-byte Ed25519 signature over `b"kami-peer-binding-v1\0"` and the canonical libp2p transport key and `PeerId`.
- **Function**: `pub fn verify_peer_binding(intended_device: [u8; 32], proof: &PeerBindingProof, authenticated_peer: Option<&PeerId>) -> Result<(), IdentityError>`
  - **What it does**: Checks that the signature is valid, non-malleable, and matches the actual authenticated transport peer.

---

### 6. Module `olm_binding` ([crates/kami-identity/src/olm_binding.rs](file:///home/phu/dev/kami/crates/kami-identity/src/olm_binding.rs))

Binds identity custody to [[kami-e2ee]] Double Ratchet keys (KAMI-51 WP2-A1).

- **Enum**: `OlmPrekeyKind`
  - **Variants**: `OneTime = 1`, `Fallback = 2`.
- **Struct**: `OlmBindingData`
  - **What it represents**: Public bundle metadata: `publisher_peer`, `olm_ed25519`, `olm_curve25519`, `prekey_kind`, `prekey_reference`, `prekey_curve25519`, `binding_generation`, `bundle_generation`, `not_before_ms`, `expires_at_ms`.
  - **Validity Window**: Timestamps are bounded to a maximum validity interval of 7 days (`MAX_OLM_BINDING_VALIDITY_MS`).
- **Struct**: `OlmBindingProof`
  - **What it represents**: The signed wire representation (capped at `MAX_OLM_BINDING_BYTES`).
- **Function**: `pub fn verify_olm_binding(contact: Option<&ContactTrust>, intended_device: [u8; 32], proof: &OlmBindingProof, expected: &OlmBindingData, authenticated_peer: Option<&PeerId>, now_ms: u64) -> Result<(), IdentityError>`
  - **What it does**: Checks contact trust state, validates the signature over the fixed transcript `b"kami-olm-binding-v1\0"`, and verifies time validity.

---

### 7. Module `descriptor` ([crates/kami-identity/src/descriptor.rs](file:///home/phu/dev/kami/crates/kami-identity/src/descriptor.rs))

- **Struct**: `Invitation`
  - **What it represents**: An exportable contact card (`b"KINV"` format, max 512 bytes) containing a device key, public descriptors, and self-signatures.
  - **Security Rule**: Imported invitations are always treated as *untrusted data* until explicitly verified by the user.

---

### 8. Module `trust` ([crates/kami-identity/src/trust.rs](file:///home/phu/dev/kami/crates/kami-identity/src/trust.rs))

Durable contact verification and policy engine (KAMI-50).

- **Enum**: `ContactTrustState`
  - **Variants**: `Unverified`, `Verified`, `Blocked`, `ChangedIdentity`.
- **Struct**: `ContactTrust`
  - **What it represents**: The verified trust state for one peer, tracking positive monotonic generations.
- **Function**: `pub fn send_decision(contact: Option<&ContactTrust>, intended_device: [u8; 32], binding: Option<&PeerBindingProof>, authenticated_peer: Option<&PeerId>) -> SendDecision`
  - **What it does**: Decides whether a message may be sent. If the recipient's identity key has changed, returns `SendDecision::Hold`, preventing accidental disclosure.

---

### 9. Module `native` ([crates/kami-identity/src/native/](file:///home/phu/dev/kami/crates/kami-identity/src/native/))

Platform-specific secure credential store adapters:
- **Linux** (`linux.rs`): Connects to Freedesktop Secret Service over D-Bus via `zbus` using an encrypted Diffie-Hellman session.
- **macOS** (`macos.rs`): Stores secrets in the Apple Login Keychain via `security-framework`.
- **Windows** (`windows.rs`): Stores secrets in Windows Credential Manager with `Local` persistence.

---

### 10. Runtime Supervisor Integration (KAMI-26)

In [[kami-runtime]], identity operations, OS keyring access, and `KeyProvider` unlock calls are bound to the dedicated single-thread blocking worker. Key material is never passed across Tokio `.await` boundaries, and contact trust checks are synchronously enforced prior to outbound E2EE preparation or inbound frame decryption.

---

### 11. Transport Credential & Trust Boundary Integration (KAMI-27)

In [[kami-transport]], `kami-identity` provides the authoritative trust and credential anchor:
- **Sealed Transport Credentials**: `SealedTransportCredentials` wraps the device's libp2p identity keypair and Peer ID. Raw key seeds are never exposed to the transport driver.
- **Mutual Handshake Enforcement**: Direct connections dial with expected `PeerId` binding, ensuring the remote host proves ownership of the private key matching its advertised identity.
- **Mandatory Trust Verification**: Upon connection establishment, `kami-transport` verifies the remote device against fresh `ContactTrust` records via `send_decision(...) == SendDecision::Allow`. Unverified, blocked, or altered identity fingerprints reject swarm admission immediately.

---

---

### 12. WP-B Native Per-Selection Signing ([crates/kami-identity/src/custody.rs](file:///home/phu/dev/kami/crates/kami-identity/src/custody.rs))

Introduced in **KAMI-51 WP-B** for batch signing of prekey selections:
- **Methods**:
  - `NativeCustody::create_olm_binding(&self, enrollment: &VerifiedEnrollment, fresh_row: &[u8], peer_binding: &PeerBindingProof, data: OlmBindingData) -> Result<OlmBindingProof, IdentityError>`
  - `NativeCustody::create_olm_bindings(&self, enrollment: &VerifiedEnrollment, fresh_row: &[u8], peer_binding: &PeerBindingProof, data: &[OlmBindingData]) -> Result<Vec<OlmBindingProof>, IdentityError>`
- **Security Invariants**:
  - Requires existing `VerifiedEnrollment`, exact fresh SQLCipher enrollment row (`fresh_row == enrollment.row`), and local peer binding verification.
  - Bounded batch accepts between 1 and 52 selections (50 OTKs plus current/previous fallback key).
  - Checks native enrollment and anchor before and after signing.
  - Verifies the protected seed against the enrolled device and Peer ID.
  - Produces independent per-selection `OlmBindingProof` signatures, **not** an aggregate batch signature.
  - Network responses carry exactly one signed selection.
  - Purpose-limited: signs only fixed prekey binding transcripts; never provides an arbitrary signing API or exports protected identity seeds.
  - `OlmBindingData::new_challenge() -> Result<[u8; 32], IdentityError>`: Generates unpredictable 32-byte exchange challenges from the OS CSPRNG (`getrandom`).

---

## Completion and Verification

- **Milestone 1 Completion Estimate**: **Not estimable**
  - *Milestone Status*: **Functionally Implemented** (KAMI-23 identity custody, KAMI-50 contact trust/fingerprints, KAMI-51 WP2-A1 signed Olm bindings, and KAMI-51 WP-B per-selection signing are fully implemented and verified on Linux; Windows/macOS native runtime qualification, cross-process lease integration, and live transport provenance remain release gates).
- **Whole-Project Completion Estimate**: **Not estimable**
- **Status Breakdown**:
  - **Implemented Behavior**: 3-stage enrollment lifecycle (`Fresh` -> `Pending` -> `Enrolled`), dual-namespace slot management, OS CSPRNG key generation, fingerprint calculation, Peer-ID binding, Olm prekey binding, per-selection batch signing (1..=52), and contact trust state transitions.
  - **Tested Behavior**: Linux is fully verified via automated tests, absent-slot probes, disposable GNOME Keyring daemon integration tests, KAMI-50/WP2-A1 binding test suites, and WP-B per-selection signing (`t_d364301b` run 204, QA `t_0dcc13ac` run 205 PASS, Evaluator `t_250e6416` run 206 PASS-WITH-CONDITIONS). macOS and Windows targets pass cross-compilation and strict Clippy checks.
  - **Missing Behavior**: Native host execution tests on physical Windows and macOS machines.
  - **Known Limitations**: Detached signatures prove binding; they do not establish live transport network connections (handled by transport layers).

### Exact Verification Commands

```sh
# Run all unit tests in kami-identity
cargo test --locked -p kami-identity

# Run KAMI-51 WP2-A1 Olm binding tests
cargo test --locked -p kami-identity --test olm_binding

# Cross-compile check for Windows and macOS targets
cargo check --locked -p kami-identity --all-targets --target x86_64-pc-windows-msvc
cargo check --locked -p kami-identity --all-targets --target x86_64-apple-darwin

# Run Linux native headless D-Bus probe (requires gnome-keyring-daemon and dbus-run-session)
cargo test --locked -p kami-identity --test linux_collection -- --ignored --nocapture
```
