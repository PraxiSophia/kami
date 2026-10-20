//! Separate dependency graph deliberately links ordinary SQLite, NOT SQLCipher.
//! Imports the actual production open path, not a reimplementation of its checks.
#[path = "../../../src/error.rs"]
mod error;
#[path = "../../../src/lease.rs"]
mod lease;
#[rustfmt::skip]
#[path = "../../../src/migrations.rs"]
mod migrations;
#[path = "../../../src/open.rs"]
mod open;
pub use error::StorageError;
use kami_key_provider::{DatabaseKey, KeyError, KeyProvider, ProfileKeyId};
pub use lease::ProfileLease;
struct Fixture;
impl KeyProvider for Fixture {
    fn load_database_key(&self, _: &ProfileKeyId) -> Result<DatabaseKey, KeyError> {
        Ok(DatabaseKey::from_bytes(zeroize::Zeroizing::new([42; 32])))
    }
}
fn main() {
    let exe = std::env::current_exe().unwrap();
    let dir = tempfile::tempdir_in(exe.parent().unwrap()).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    let lease = ProfileLease::acquire(dir.path()).unwrap();
    let result = open::connect(
        &lease,
        &ProfileKeyId::new("0123456789abcdef0123456789abcdef").unwrap(),
        &Fixture,
        true,
        16_384,
    );
    assert!(matches!(result, Err(StorageError::CipherUnavailable)));
    assert_eq!(
        std::fs::metadata(dir.path().join("messages.db"))
            .unwrap()
            .len(),
        0
    );
    println!("PASS: production open rejects a cipher-disabled binary before schema/data writes");
}
