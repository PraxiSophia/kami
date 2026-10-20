use super::NativeStore;
use crate::{KeySlot, SecretStore};
use kami_key_provider::KeyError;
use keyring_core::{api::CredentialStoreApi, Error};
use security_framework::{
    base::Error as NativeError,
    os::macos::keychain::{SecKeychain, SecPreferencesDomain},
};
use std::sync::Mutex;
use zeroize::{Zeroize, Zeroizing};

// Keychain's UI interaction setting is process-global. Serialize this adapter's
// operations; service composition must not concurrently change that setting.
static ACCESS: Mutex<()> = Mutex::new(());
fn service() -> &'static str {
    "org.kami.identity.v1"
}
impl SecretStore for NativeStore {
    fn read(&self, slot: &KeySlot) -> Result<Zeroizing<Vec<u8>>, KeyError> {
        let _access = ACCESS.lock().map_err(|_| KeyError::Other)?;
        let _no_ui = SecKeychain::disable_user_interaction().map_err(map_native)?;
        let store = apple_native_keyring_store::keychain::Store::new().map_err(map_error)?;
        let entry = store
            .build(service(), &slot.to_persistent_string(), None)
            .map_err(map_error)?;
        Ok(Zeroizing::new(entry.get_secret().map_err(map_error)?))
    }
    fn create(&self, slot: &KeySlot, secret: &[u8]) -> Result<(), KeyError> {
        if secret.len() != 32 {
            return Err(KeyError::Corrupt);
        }
        let _access = ACCESS.lock().map_err(|_| KeyError::Other)?;
        let _no_ui = SecKeychain::disable_user_interaction().map_err(map_native)?;
        let keychain =
            SecKeychain::default_for_domain(SecPreferencesDomain::User).map_err(map_native)?;
        // SecKeychainAddGenericPassword fails with duplicate-item rather than
        // updating an existing identity (unlike keyring's set_secret upsert).
        keychain
            .add_generic_password(service(), &slot.to_persistent_string(), secret)
            .map_err(map_native)
    }
}
fn map_native(error: NativeError) -> KeyError {
    match error.code() {
        -25300 => KeyError::Missing, // errSecItemNotFound
        -25308 => KeyError::Locked,  // errSecInteractionNotAllowed (no UI)
        -25293 | -128 | -61 | -25244 | -25292 => KeyError::PermissionDenied,
        -25291 | -25294 | -25295 => KeyError::Unavailable,
        -25299 | -26275 => KeyError::Corrupt, // duplicate item / decode failure
        _ => KeyError::Other,
    }
}
fn map_error(error: Error) -> KeyError {
    match error {
        Error::NoEntry => KeyError::Missing,
        Error::NoStorageAccess(native) | Error::PlatformFailure(native) => native
            .downcast_ref::<NativeError>()
            .map(|e| map_native(NativeError::from_code(e.code())))
            .unwrap_or(KeyError::Other),
        Error::BadEncoding(mut bytes) | Error::BadDataFormat(mut bytes, _) => {
            bytes.zeroize();
            KeyError::Corrupt
        }
        Error::Ambiguous(_)
        | Error::BadStoreFormat(_)
        | Error::Invalid(_, _)
        | Error::TooLong(_, _) => KeyError::Corrupt,
        Error::NoDefaultStore | Error::NotSupportedByStore(_) => KeyError::Unavailable,
        _ => KeyError::Other,
    }
}
