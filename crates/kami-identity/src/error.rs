use std::fmt;

/// Stable diagnostics, never containing keys, invitations, hints or backend errors.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IdentityError {
    Malformed,
    UnsupportedVersion,
    Oversized,
    InvalidKey,
    ForgedBinding,
    WrongPeer,
    Unauthenticated,
    Unknown,
    VerificationRequired,
    Changed,
    Blocked,
    Conflict,
    Persistence,
    Custody(kami_key_provider::KeyError),
}
impl fmt::Display for IdentityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "identity: {self:?}")
    }
}
impl std::error::Error for IdentityError {}
