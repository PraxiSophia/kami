use crate::{
    errors::{Error, ProviderError},
    profile::{Profile, MAX_PLAINTEXT_BYTES},
    provider::{DecryptResult, E2eeProvider, EncryptResult},
    state::{StateEnvelope, StateVersion, MAX_STATE_BYTES},
    types::ApplicationContext,
};
use vodozemac::olm::{OlmMessage, Session, SessionPickle};

/// Provider JSON input ceiling, not a full network-envelope size guarantee.
pub const MAX_CIPHERTEXT_BYTES: usize = 65_536;

/// Internal foundation around vodozemac 0.11.1, not an enabled messaging provider.
///
/// Olm has no native AAD API here. Public encrypt/decrypt operations fail with
/// `AadUnsupported` until a separately reviewed authenticated context binding
/// exists. There is no opt-out flag or raw-session escape hatch.
///
/// ```compile_fail
/// use kami_e2ee::VodozemacProvider;
/// fn bypass(provider: VodozemacProvider) {
///     let _raw = provider.into_session();
/// }
/// ```
pub struct VodozemacProvider {
    session: Session,
    generation: u64,
    max_plaintext_bytes: usize,
}

impl VodozemacProvider {
    /// Anchor the supplied local session at generation zero. This is not a
    /// durable-state import, authenticated enrollment, or commit API.
    pub fn new(session: Session) -> Self {
        Self {
            session,
            generation: 0,
            max_plaintext_bytes: MAX_PLAINTEXT_BYTES,
        }
    }

    pub fn with_profile(session: Session, profile: &Profile) -> Result<Self, Error> {
        profile.validate()?;
        Ok(Self::new(session))
    }

    fn encode_session(session: &Session) -> Result<Vec<u8>, Error> {
        serde_json::to_vec(&session.pickle())
            .map_err(|_| Error::Provider(ProviderError::Serialization))
    }

    fn validate_state(&self, state: &StateEnvelope) -> Result<(), Error> {
        if state.version != StateVersion::V1 {
            return Err(Error::StateVersion {
                found: state.version.0,
                expected: StateVersion::V1.0,
            });
        }
        if state.generation != self.generation {
            return Err(Error::StateRollback {
                found: state.generation,
                expected: self.generation,
            });
        }
        if state.provider.len() > MAX_STATE_BYTES {
            return Err(Error::StateTooLarge);
        }
        if state.provider.is_empty() {
            // The empty sentinel is supported only for initial bootstrapping.
            if self.generation != 0 {
                return Err(Error::Provider(ProviderError::State));
            }
        } else if state.provider != Self::encode_session(&self.session)? {
            // A matching counter alone does not identify a committed branch.
            return Err(Error::Provider(ProviderError::State));
        }
        Ok(())
    }

    fn session_for(&self, state: &StateEnvelope) -> Result<Session, Error> {
        // Directly constructed public envelopes are untrusted too. Validate the
        // version, exact generation, size and branch before provider parsing.
        self.validate_state(state)?;
        let initial;
        let bytes = if state.provider.is_empty() {
            initial = Self::encode_session(&self.session)?;
            initial.as_slice()
        } else {
            state.provider.as_slice()
        };
        serde_json::from_slice::<SessionPickle>(bytes)
            .map(Session::from_pickle)
            .map_err(|_| Error::Provider(ProviderError::State))
    }

    fn next_state(state: &StateEnvelope, session: &Session) -> Result<StateEnvelope, Error> {
        let generation = state
            .generation
            .checked_add(1)
            .ok_or(Error::StateEncoding)?;
        StateEnvelope::new(generation, Self::encode_session(session)?)
    }

    // Private unbound staging primitives retained for fixed foundation tests.
    // Public callers cannot reach these until authenticated context is supported.
    fn encrypt_unbound(
        &self,
        state: &StateEnvelope,
        plaintext: &[u8],
        encoded_context: Vec<u8>,
    ) -> Result<EncryptResult, Error> {
        if plaintext.len() > self.max_plaintext_bytes {
            return Err(Error::Provider(ProviderError::PlaintextTooLarge));
        }
        let mut session = self.session_for(state)?;
        let message = session
            .encrypt(plaintext)
            .map_err(|_| Error::Provider(ProviderError::Session))?;
        let ciphertext = serde_json::to_vec(&message)
            .map_err(|_| Error::Provider(ProviderError::Serialization))?;
        Ok(EncryptResult {
            ciphertext,
            next_state: Self::next_state(state, &session)?,
            context: encoded_context,
        })
    }

    fn decrypt_unbound(
        &self,
        state: &StateEnvelope,
        ciphertext: &[u8],
        encoded_context: Vec<u8>,
    ) -> Result<DecryptResult, Error> {
        Self::validate_ciphertext_size(ciphertext)?;
        let mut session = self.session_for(state)?;
        let message: OlmMessage = serde_json::from_slice(ciphertext)
            .map_err(|_| Error::Provider(ProviderError::Ciphertext))?;
        let plaintext = session
            .decrypt(&message)
            .map_err(|_| Error::Provider(ProviderError::Session))?;
        if plaintext.len() > self.max_plaintext_bytes {
            return Err(Error::Provider(ProviderError::PlaintextTooLarge));
        }
        Ok(DecryptResult {
            plaintext,
            next_state: Self::next_state(state, &session)?,
            context: encoded_context,
        })
    }

    fn validate_ciphertext_size(ciphertext: &[u8]) -> Result<(), Error> {
        if ciphertext.len() > MAX_CIPHERTEXT_BYTES {
            return Err(Error::Provider(ProviderError::Ciphertext));
        }
        Ok(())
    }
}

impl E2eeProvider for VodozemacProvider {
    fn encrypt(
        &self,
        state: &StateEnvelope,
        plaintext: &[u8],
        context: &ApplicationContext,
    ) -> Result<EncryptResult, Error> {
        let encoded_context = context.encode()?;
        self.validate_state(state)?;
        if !self.supports_aad() {
            return Err(Error::AadUnsupported);
        }
        self.encrypt_unbound(state, plaintext, encoded_context)
    }

    fn decrypt(
        &self,
        state: &StateEnvelope,
        ciphertext: &[u8],
        context: &ApplicationContext,
    ) -> Result<DecryptResult, Error> {
        Self::validate_ciphertext_size(ciphertext)?;
        let encoded_context = context.encode()?;
        self.validate_state(state)?;
        if !self.supports_aad() {
            return Err(Error::AadUnsupported);
        }
        self.decrypt_unbound(state, ciphertext, encoded_context)
    }
}

#[cfg(test)]
#[path = "vodozemac_tests.rs"]
mod tests;
