use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum Error {
    #[error("profile restore requires an unsupported non-rollbackable anchor")]
    RollbackAnchorUnsupported,
    #[error("authenticated envelope rejected")]
    AuthenticationRejected,
    #[error("unsupported or malformed profile context")]
    InvalidContext,
    #[error("state is too large")]
    StateTooLarge,
    #[error("state version {found} is not accepted by version {expected}")]
    StateVersion { found: u16, expected: u16 },
    #[error("state generation mismatch: {found} != {expected}")]
    StateRollback { found: u64, expected: u64 },
    #[error("state encoding failed")]
    StateEncoding,
    #[error("provider operation failed: {0}")]
    Provider(ProviderError),
    #[error("AAD/context binding is not implemented by the selected provider")]
    AadUnsupported,
}

/// Stable, intentionally non-diagnostic provider failure categories.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ProviderError {
    #[error("invalid provider state")]
    State,
    #[error("provider serialization failed")]
    Serialization,
    #[error("invalid ciphertext")]
    Ciphertext,
    #[error("cryptographic session operation failed")]
    Session,
    #[error("plaintext exceeds the configured provider limit")]
    PlaintextTooLarge,
}
