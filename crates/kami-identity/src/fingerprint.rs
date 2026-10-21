use crate::IdentityError;
use ed25519_dalek::VerifyingKey;
use sha2::{Digest, Sha256};
use std::fmt;

pub(crate) fn checked_key(bytes: &[u8; 32]) -> Result<VerifyingKey, IdentityError> {
    let key = VerifyingKey::from_bytes(bytes).map_err(|_| IdentityError::InvalidKey)?;
    if key.is_weak() || key.to_edwards().compress().to_bytes() != *bytes {
        return Err(IdentityError::InvalidKey);
    }
    Ok(key)
}
/// Full version-1, domain-separated SHA-256 fingerprint; no abbreviated comparison.
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct DeviceFingerprint([u8; 32]);
impl DeviceFingerprint {
    /// Parse exactly eight lowercase eight-hex-digit groups, all 256 bits.
    /// This is a comparison value, not a public-key validity or trust assertion.
    pub fn parse_display(display: &str) -> Result<Self, IdentityError> {
        if display.len() != 71 || !display.is_ascii() {
            return Err(IdentityError::Malformed);
        }
        let bytes = display.as_bytes();
        let mut digest = [0u8; 32];
        for (i, output) in digest.iter_mut().enumerate() {
            let offset = i * 2 + i / 4;
            if i % 4 == 0 && i > 0 && bytes[offset - 1] != b' ' {
                return Err(IdentityError::Malformed);
            }
            let nibble = |byte| match byte {
                b'0'..=b'9' => Ok(byte - b'0'),
                b'a'..=b'f' => Ok(byte - b'a' + 10),
                _ => Err(IdentityError::Malformed),
            };
            *output = nibble(bytes[offset])? * 16 + nibble(bytes[offset + 1])?;
        }
        Ok(Self(digest))
    }
    pub fn from_public_key(key: [u8; 32]) -> Result<Self, IdentityError> {
        checked_key(&key)?;
        let mut digest = Sha256::new();
        digest.update(b"kami-device-fingerprint-v1\0");
        digest.update(key);
        Ok(Self(digest.finalize().into()))
    }
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}
impl fmt::Display for DeviceFingerprint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, byte) in self.0.iter().enumerate() {
            if i != 0 && i % 4 == 0 {
                f.write_str(" ")?;
            }
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}
impl fmt::Debug for DeviceFingerprint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("DeviceFingerprint([REDACTED])")
    }
}
