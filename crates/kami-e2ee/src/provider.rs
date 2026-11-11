use crate::{errors::Error, state::StateEnvelope, types::ApplicationContext};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncryptResult {
    pub ciphertext: Vec<u8>,
    pub next_state: StateEnvelope,
    pub context: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecryptResult {
    pub plaintext: Vec<u8>,
    pub next_state: StateEnvelope,
    pub context: Vec<u8>,
}

/// Native provider-AAD transitions, pure from the caller's perspective.
/// Persistence commits `next_state` only after the enclosing operation succeeds.
/// `context.encode()` must be authenticated as associated data, not merely
/// returned alongside ciphertext. Unsupported providers reject the operation;
/// `supports_aad` is never permission to use an unbound fallback.
///
/// This contract is distinct from Kami's canonical inner-envelope composition,
/// which authenticates context *inside* Olm plaintext and checks correspondence
/// after decryption. That composition remains quarantined in `BoundSession`;
/// it neither implements native AAD nor establishes live runtime provenance.
/// No low-level unbound provider operation is part of this public contract.
pub trait NativeAadProvider {
    fn encrypt(
        &self,
        state: &StateEnvelope,
        plaintext: &[u8],
        context: &ApplicationContext,
    ) -> Result<EncryptResult, Error>;
    fn decrypt(
        &self,
        state: &StateEnvelope,
        ciphertext: &[u8],
        context: &ApplicationContext,
    ) -> Result<DecryptResult, Error>;
    /// Whether the provider can authenticate native associated data. An
    /// authenticated inner plaintext format does not make this capability true.
    fn supports_aad(&self) -> bool {
        false
    }
}

/// Compatibility name for the native-AAD contract, not an application-bound
/// session capability or an indication that production encryption is enabled.
pub use NativeAadProvider as E2eeProvider;
