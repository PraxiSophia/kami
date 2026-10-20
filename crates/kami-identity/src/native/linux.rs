use super::NativeStore;
use crate::{KeySlot, SecretStore};
use kami_key_provider::KeyError;
use secret_service::{blocking::SecretService, EncryptionType, Error};
use std::collections::HashMap;
use zeroize::Zeroizing;

fn connect() -> Result<SecretService<'static>, KeyError> {
    SecretService::connect(EncryptionType::Dh).map_err(map_error)
}
fn attributes(handle: &str) -> HashMap<&str, &str> {
    HashMap::from([
        ("application", "org.kami.identity"),
        ("kami-handle", handle),
    ])
}
impl SecretStore for NativeStore {
    fn read(&self, slot: &KeySlot) -> Result<Zeroizing<Vec<u8>>, KeyError> {
        let service = connect()?;
        // No session-only collection or get_any_collection fallback. Failure to
        // obtain a persistent default collection is unavailable, not absence.
        let collection = service.get_default_collection().map_err(map_error)?;
        collection.ensure_unlocked().map_err(map_error)?;
        let handle = slot.to_persistent_string();
        // Collection-scoped search is essential: service-wide search also finds
        // ephemeral session credentials and keys in unrelated collections.
        let matches = collection
            .search_items(attributes(&handle))
            .map_err(map_error)?;
        // Unlike service-wide search, collection search does not partition by
        // lock state. Fail closed without ever requesting an unlock prompt.
        for item in &matches {
            item.ensure_unlocked().map_err(map_error)?;
        }
        match matches.as_slice() {
            [] => Err(KeyError::Missing),
            [item] => Ok(Zeroizing::new(item.get_secret().map_err(map_error)?)),
            _ => Err(KeyError::Corrupt),
        }
    }
    fn create(&self, slot: &KeySlot, secret: &[u8]) -> Result<(), KeyError> {
        if secret.len() != 32 {
            return Err(KeyError::Corrupt);
        }
        match self.read(slot) {
            Err(KeyError::Missing) => {}
            Ok(_) => return Err(KeyError::Corrupt),
            Err(error) => return Err(error),
        }
        let service = connect()?;
        let collection = service.get_default_collection().map_err(map_error)?;
        collection.ensure_unlocked().map_err(map_error)?;
        let handle = slot.to_persistent_string();
        // replace=false never updates a racing existing credential. A duplicate
        // is detected by readback as Corrupt; caller must retain the profile lease.
        collection
            .create_item(
                "Kami protected key",
                attributes(&handle),
                secret,
                false,
                "application/octet-stream",
            )
            .map_err(map_error)?;
        Ok(())
    }
}
fn map_error(error: Error) -> KeyError {
    match error {
        Error::Locked => KeyError::Locked,
        Error::Prompt => KeyError::PermissionDenied,
        Error::ZbusFdo(zbus::fdo::Error::AccessDenied(_) | zbus::fdo::Error::AuthFailed(_)) => {
            KeyError::PermissionDenied
        }
        Error::Zbus(zbus::Error::MethodError(name, _, _)) => match name.as_str() {
            "org.freedesktop.DBus.Error.AccessDenied" | "org.freedesktop.DBus.Error.AuthFailed" => {
                KeyError::PermissionDenied
            }
            "org.freedesktop.Secret.Error.IsLocked" => KeyError::Locked,
            _ => KeyError::Unavailable,
        },
        Error::Crypto(_) | Error::Zvariant(_) => KeyError::Corrupt,
        // NoResult can mean no collection, NOT confirmed credential absence.
        _ => KeyError::Unavailable,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_errors_never_turn_query_failure_into_missing() {
        assert_eq!(map_error(Error::Locked), KeyError::Locked);
        assert_eq!(
            map_error(Error::ZbusFdo(zbus::fdo::Error::AccessDenied(
                "private credential detail".into()
            ))),
            KeyError::PermissionDenied
        );
        assert_eq!(
            map_error(Error::Zbus(zbus::Error::MethodError(
                "org.freedesktop.DBus.Error.AccessDenied"
                    .try_into()
                    .unwrap(),
                Some("private credential detail".into()),
                zbus::Message::method_call("/", "Test")
                    .unwrap()
                    .build(&())
                    .unwrap()
            ))),
            KeyError::PermissionDenied
        );
        assert_eq!(map_error(Error::Prompt), KeyError::PermissionDenied);
        assert_eq!(map_error(Error::NoResult), KeyError::Unavailable);
        assert_eq!(map_error(Error::Unavailable), KeyError::Unavailable);
        assert_eq!(
            map_error(Error::Crypto("sensitive native detail")),
            KeyError::Corrupt
        );
    }
}
