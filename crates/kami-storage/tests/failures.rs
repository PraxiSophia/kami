mod common;
use common::*;
use kami_key_provider::{test_provider::TestKeyProvider, KeyError};
use kami_storage::{Config, ProfileLease, Storage, StorageError};
use std::sync::Arc;

#[test]
fn injected_key_failures_never_create_files_or_replace_existing_database() {
    let provider = Arc::new(TestKeyProvider::seeded(profile(), [42; 32]));
    for error in [
        KeyError::Locked,
        KeyError::Unavailable,
        KeyError::Missing,
        KeyError::Corrupt,
        KeyError::PermissionDenied,
        KeyError::Other,
    ] {
        let dir = directory();
        provider.fail_next(&profile(), error).unwrap();
        assert!(
            matches!(Storage::create(ProfileLease::acquire(dir.path()).unwrap(),profile(),provider.clone(),Config::default()),Err(StorageError::Key(e)) if e==error)
        );
        assert!(!dir.path().join("messages.db").exists());
        assert!(!dir.path().join("enrollment").exists());
        let s = create(&dir);
        s.enqueue(message(facts(1, 1))).unwrap();
        s.shutdown().unwrap();
        let before = std::fs::read(dir.path().join("messages.db")).unwrap();
        provider.fail_next(&profile(), error).unwrap();
        assert!(
            matches!(Storage::open(ProfileLease::acquire(dir.path()).unwrap(),profile(),provider.clone(),Config::default()),Err(StorageError::Key(e)) if e==error)
        );
        assert_eq!(
            before,
            std::fs::read(dir.path().join("messages.db")).unwrap()
        );
        assert_eq!(open(&dir).pending(1).unwrap().len(), 1);
    }
}
#[test]
fn wrong_key_corrupt_plaintext_and_missing_database_fail_closed() {
    let dir = directory();
    create(&dir).shutdown().unwrap();
    assert!(matches!(
        Storage::open(
            ProfileLease::acquire(dir.path()).unwrap(),
            profile(),
            Arc::new(Provider(Ok(41))),
            Config::default()
        ),
        Err(StorageError::WrongKeyOrCorrupt)
    ));
    std::fs::write(
        dir.path().join("messages.db"),
        b"SQLite format 3\0not a valid keyed database",
    )
    .unwrap();
    assert!(matches!(
        Storage::open(
            ProfileLease::acquire(dir.path()).unwrap(),
            profile(),
            Arc::new(Provider(Ok(42))),
            Config::default()
        ),
        Err(StorageError::WrongKeyOrCorrupt)
    ));
    std::fs::remove_file(dir.path().join("messages.db")).unwrap();
    assert!(matches!(
        Storage::open(
            ProfileLease::acquire(dir.path()).unwrap(),
            profile(),
            Arc::new(Provider(Ok(42))),
            Config::default()
        ),
        Err(StorageError::MissingDatabase)
    ));
    assert!(matches!(
        Storage::create(
            ProfileLease::acquire(dir.path()).unwrap(),
            profile(),
            Arc::new(Provider(Ok(42))),
            Config::default()
        ),
        Err(StorageError::AlreadyExists)
    ));
}
#[test]
fn quotas_and_sqlite_full_rollback_do_not_grant_send_or_ack() {
    let dir = directory();
    let s = Storage::create(
        ProfileLease::acquire(dir.path()).unwrap(),
        profile(),
        Arc::new(Provider(Ok(42))),
        Config {
            outbox_bytes: 2,
            ..Config::default()
        },
    )
    .unwrap();
    assert_eq!(s.enqueue(message(facts(1, 1))), Err(StorageError::Full));
    assert!(s.pending(10).unwrap().is_empty());
    s.shutdown().unwrap();
    let s = Storage::open(
        ProfileLease::acquire(dir.path()).unwrap(),
        profile(),
        Arc::new(Provider(Ok(42))),
        Config {
            max_database_pages: 16,
            ..Config::default()
        },
    )
    .unwrap();
    assert_eq!(
        s.enqueue(kami_storage::StoredMessage::new(facts(1, 1), vec![7; 65_536]).unwrap()),
        Err(StorageError::Full)
    );
    assert!(s.pending(10).unwrap().is_empty());
    assert_eq!(
        s.accept(
            kami_storage::StoredMessage::new(facts(1, 1), vec![7; 65_536]).unwrap(),
            1
        ),
        Err(StorageError::Full)
    );
    assert!(s
        .receipt(facts(1, 1).sender(), facts(1, 1).message_id())
        .unwrap()
        .is_none());
}
#[test]
fn sqlite_lock_is_bounded_and_never_loses_accepted_records() {
    let dir = directory();
    let s = create(&dir);
    let other = raw(&dir);
    other.execute_batch("BEGIN IMMEDIATE").unwrap();
    assert_eq!(s.enqueue(message(facts(1, 1))), Err(StorageError::Busy));
    other.execute_batch("ROLLBACK").unwrap();
    s.enqueue(message(facts(1, 1))).unwrap();
    assert_eq!(s.pending(1).unwrap().len(), 1);
}
#[test]
fn outbox_count_quota_rejects_new_messages_without_consuming_sequence() {
    let dir = directory();
    let s = Storage::create(
        ProfileLease::acquire(dir.path()).unwrap(),
        profile(),
        Arc::new(Provider(Ok(42))),
        Config {
            outbox_messages: 1,
            ..Config::default()
        },
    )
    .unwrap();
    s.enqueue(message(facts(1, 1))).unwrap();
    assert_eq!(s.enqueue(message(facts(2, 2))), Err(StorageError::Full));
    s.cancel(facts(1, 1).sender(), facts(1, 1).message_id())
        .unwrap();
    s.enqueue(message(facts(2, 2))).unwrap();
    assert_eq!(s.pending(10).unwrap()[0].facts(), facts(2, 2));
}

#[test]
fn refuses_newer_schema_without_rewriting_database() {
    let dir = directory();
    create(&dir).shutdown().unwrap();
    let db = raw(&dir);
    db.pragma_update(None, "user_version", 99).unwrap();
    drop(db);
    assert!(matches!(
        Storage::open(
            ProfileLease::acquire(dir.path()).unwrap(),
            profile(),
            Arc::new(Provider(Ok(42))),
            Config::default()
        ),
        Err(StorageError::NewerSchema)
    ));
    assert_eq!(
        raw(&dir)
            .pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))
            .unwrap(),
        99
    );
}
