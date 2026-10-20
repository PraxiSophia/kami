use std::fmt;
use zeroize::Zeroizing;

/// Owned 256-bit database key. Not Clone or Copy; zeroized on drop.
///
/// ```compile_fail
/// use kami_key_provider::DatabaseKey;
/// use zeroize::Zeroizing;
/// let key = DatabaseKey::from_bytes(Zeroizing::new([0; 32]));
/// let duplicate = key.clone();
/// ```
///
/// ```compile_fail
/// use kami_key_provider::DatabaseKey;
/// fn requires_copy<T: Copy>() {}
/// requires_copy::<DatabaseKey>();
/// ```
pub struct DatabaseKey(Zeroizing<[u8; 32]>);

impl DatabaseKey {
    /// Takes ownership of an exact-length, already zeroizing key.
    pub fn from_bytes(bytes: Zeroizing<[u8; 32]>) -> Self {
        Self(bytes)
    }

    /// Borrow only at the encrypted-storage boundary. Never log the borrow.
    pub fn with_bytes<T>(&self, use_key: impl FnOnce(&[u8; 32]) -> T) -> T {
        use_key(&self.0)
    }
}

impl fmt::Debug for DatabaseKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("DatabaseKey([REDACTED])")
    }
}

impl fmt::Display for DatabaseKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[REDACTED]")
    }
}
