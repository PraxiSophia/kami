mod common;
use common::*;
use kami_storage::{Config, ProfileLease, Storage, StorageError};
use std::{
    io::{Seek, SeekFrom, Write},
    sync::Arc,
};

#[test]
fn cipher_authentication_detects_modified_database_page() {
    let dir = directory();
    let s = create(&dir);
    s.enqueue(message(facts(1, 1))).unwrap();
    s.shutdown().unwrap();
    let path = dir.path().join("messages.db");
    let mut file = std::fs::OpenOptions::new().write(true).open(path).unwrap();
    file.seek(SeekFrom::Start(100)).unwrap();
    file.write_all(&[0; 32]).unwrap();
    file.sync_all().unwrap();
    drop(file);
    assert!(matches!(
        Storage::open(
            ProfileLease::acquire(dir.path()).unwrap(),
            profile(),
            Arc::new(Provider(Ok(42))),
            Config::default()
        ),
        Err(StorageError::WrongKeyOrCorrupt)
    ));
}
#[test]
fn unavailable_key_runs_on_worker_and_no_database_is_created_for_missing_open() {
    struct ThreadProvider;
    impl kami_key_provider::KeyProvider for ThreadProvider {
        fn load_database_key(
            &self,
            _: &kami_key_provider::ProfileKeyId,
        ) -> Result<kami_key_provider::DatabaseKey, kami_key_provider::KeyError> {
            assert_eq!(std::thread::current().name(), Some("kami-storage"));
            Err(kami_key_provider::KeyError::Unavailable)
        }
    }
    let dir = directory();
    assert!(matches!(
        Storage::open(
            ProfileLease::acquire(dir.path()).unwrap(),
            profile(),
            Arc::new(ThreadProvider),
            Config::default()
        ),
        Err(StorageError::Key(kami_key_provider::KeyError::Unavailable))
    ));
    assert!(!dir.path().join("messages.db").exists());
}
#[test]
fn database_and_profile_errors_are_redacted() {
    let msg = message(facts(1, 1));
    assert_eq!(format!("{msg:?}"), "StoredMessage([REDACTED])");
    for error in [
        StorageError::Full,
        StorageError::WrongKeyOrCorrupt,
        StorageError::Key(kami_key_provider::KeyError::Locked),
    ] {
        assert!(!format!("{error} {error:?}").contains("SECRET"));
        assert!(std::error::Error::source(&error).is_none());
    }
}
#[cfg(unix)]
#[test]
fn refuses_insecure_directories_symlinked_profiles_and_sidecars() {
    use std::os::unix::fs::{symlink, PermissionsExt};
    let dir = directory();
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(ProfileLease::acquire(dir.path()).is_err());
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    symlink(dir.path(), dir.path().join("alias")).unwrap();
    assert!(ProfileLease::acquire(&dir.path().join("alias")).is_err());
    create(&dir).shutdown().unwrap();
    symlink(
        dir.path().join("messages.db"),
        dir.path().join("messages.db-wal"),
    )
    .unwrap();
    assert!(matches!(
        Storage::open(
            ProfileLease::acquire(dir.path()).unwrap(),
            profile(),
            Arc::new(Provider(Ok(42))),
            Config::default()
        ),
        Err(StorageError::PermissionDenied)
    ));
}
