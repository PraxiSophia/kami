use super::NativeStore;
use crate::{KeySlot, SecretStore};
use kami_key_provider::KeyError;
use keyring_core::{api::CredentialStoreApi, Entry, Error};
use std::{collections::HashMap, sync::Mutex};
use zeroize::{Zeroize, Zeroizing};

// Credential Manager operations on a slot must be sequenced on one worker.
// Cross-process enrollment additionally requires the canonical profile lease.
static ACCESS: Mutex<()> = Mutex::new(());
fn entry(slot: &KeySlot) -> Result<Entry, KeyError> {
    let store = windows_native_keyring_store::Store::new().map_err(map_error)?;
    let handle = slot.to_persistent_string();
    // Explicit Local persistence, never the library's Enterprise default.
    store
        .build(
            "org.kami.identity.v1",
            slot.profile().as_str(),
            Some(&HashMap::from([
                ("target", handle.as_str()),
                ("persistence", "Local"),
            ])),
        )
        .map_err(map_error)
}
impl SecretStore for NativeStore {
    fn read(&self, slot: &KeySlot) -> Result<Zeroizing<Vec<u8>>, KeyError> {
        let _access = ACCESS.lock().map_err(|_| KeyError::Other)?;
        Ok(Zeroizing::new(
            entry(slot)?.get_secret().map_err(map_error)?,
        ))
    }
    fn create(&self, slot: &KeySlot, secret: &[u8]) -> Result<(), KeyError> {
        if secret.len() != 32 {
            return Err(KeyError::Corrupt);
        }
        let _access = ACCESS.lock().map_err(|_| KeyError::Other)?;
        let entry = entry(slot)?;
        match entry.get_secret() {
            Err(Error::NoEntry) => {}
            Ok(bytes) => {
                let _secret = Zeroizing::new(bytes);
                return Err(KeyError::Corrupt);
            }
            Err(error) => return Err(map_error(error)),
        }
        // CredWrite is an upsert. The caller MUST hold its exclusive profile
        // lease across this call; arbitrary noncooperating writers are not safe.
        entry.set_secret(secret).map_err(map_error)
    }
}
fn map_error(error: Error) -> KeyError {
    match error {
        Error::NoEntry => KeyError::Missing,
        // This backend uses NoStorageAccess for ERROR_NO_SUCH_LOGON_SESSION.
        Error::NoStorageAccess(_) | Error::NoDefaultStore | Error::NotSupportedByStore(_) => {
            KeyError::Unavailable
        }
        Error::BadEncoding(mut bytes) | Error::BadDataFormat(mut bytes, _) => {
            bytes.zeroize();
            KeyError::Corrupt
        }
        Error::Ambiguous(_)
        | Error::BadStoreFormat(_)
        | Error::Invalid(_, _)
        | Error::TooLong(_, _) => KeyError::Corrupt,
        // Backend's numeric PlatformError is private. Never parse/localize its
        // message or misclassify a query failure as confirmed absence.
        _ => KeyError::Other,
    }
}
