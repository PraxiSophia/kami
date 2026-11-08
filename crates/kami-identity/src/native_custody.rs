use crate::{native::NativeStore, Custody};
use kami_key_provider::{DatabaseKey, KeyError, KeyProvider, ProfileKeyId};

/// Concrete native-only custody. No injectable store or identity-seed access.
/// ```compile_fail
/// fn fake<S: kami_identity::SecretStore>(store: S) {
///     let _ = kami_identity::NativeCustody::new(store);
/// }
/// ```
/// ```compile_fail
/// fn inject<S: kami_identity::SecretStore>(store: S, profile: &kami_key_provider::ProfileKeyId) {
///     let custody = kami_identity::Custody::new(store);
///     let _ = kami_identity::verify_enrollment_for_transport(&custody, profile, &[]);
/// }
/// ```
pub struct NativeCustody {
    pub(crate) custody: Custody<NativeStore>,
}
impl NativeCustody {
    pub fn open() -> Self {
        Self {
            custody: Custody::new(NativeStore::new()),
        }
    }
}
impl KeyProvider for NativeCustody {
    fn load_database_key(&self, profile: &ProfileKeyId) -> Result<DatabaseKey, KeyError> {
        self.custody.load_database_key(profile)
    }
}
impl std::fmt::Debug for NativeCustody {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("NativeCustody([REDACTED])")
    }
}
