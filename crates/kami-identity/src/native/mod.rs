//! Explicit target-selected native store; no mutable process-global builder.
use std::fmt;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;

/// Uses only the platform's protected, persistent credential facility.
/// No fallback, automatic unlock, deletion, or plaintext file storage.
#[derive(Default)]
pub struct NativeStore;
impl NativeStore {
    pub fn new() -> Self {
        Self
    }
}
impl fmt::Debug for NativeStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("NativeStore([REDACTED])")
    }
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
impl crate::SecretStore for NativeStore {
    fn read(
        &self,
        _: &crate::KeySlot,
    ) -> Result<zeroize::Zeroizing<Vec<u8>>, kami_key_provider::KeyError> {
        Err(kami_key_provider::KeyError::Unavailable)
    }
    fn create(&self, _: &crate::KeySlot, _: &[u8]) -> Result<(), kami_key_provider::KeyError> {
        Err(kami_key_provider::KeyError::Unavailable)
    }
}
