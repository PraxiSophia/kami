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

/// Provider transitions are pure from the caller's perspective: persistence
/// commits `next_state` only after the enclosing operation succeeds. Providers
/// must reject application operations when they cannot authenticate context;
/// `supports_aad` is a capability query, not permission to ignore that binding.
pub trait E2eeProvider {
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
    fn supports_aad(&self) -> bool {
        false
    }
}
