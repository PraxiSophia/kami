use crate::KeyError;
use std::fmt;

/// Stable random 128-bit identifier encoded as exactly 32 lowercase hex digits.
/// It is a credential namespace, never a filesystem path or display name.
#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct ProfileKeyId(String);
impl ProfileKeyId {
    pub fn new(value: &str) -> Result<Self, KeyError> {
        if value.len() != 32
            || !value
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(KeyError::Corrupt);
        }
        Ok(Self(value.to_owned()))
    }
    /// Nonsecret persistence representation; do not use in diagnostics.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl fmt::Debug for ProfileKeyId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ProfileKeyId([REDACTED])")
    }
}
