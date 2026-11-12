mod common;
use kami_storage::{Config, ProfileLease, Storage, StorageError};
#[test]
fn encrypted_v4_upgrade_preserves_canonical_rows_and_key() {
    let dir = common::directory();
    let s = common::create(&dir);
    s.accept(common::message(common::facts(1, 1)), 1000)
        .unwrap();
    s.shutdown().unwrap();
    let db = common::raw(&dir);
    let key: Vec<u8> = db
        .query_row("SELECT key FROM sender_binding_key", [], |r| r.get(0))
        .unwrap();
    db.execute_batch(
        "DROP TABLE prekey_session; DROP TABLE prekey_received; DROP TABLE prekey_query; DROP TABLE prekey_authority; DROP TABLE prekey_publication; DROP TABLE prekey_inventory; DROP TABLE prekey_clock; DROP TABLE enrollment_complete; DROP TABLE delivery_artifact; DROP TABLE receiver_gap; PRAGMA user_version=4;",
    )
    .unwrap();
    drop(db);
    let s = common::open(&dir);
    assert_eq!(
        s.history(common::facts(1, 1).conversation_id(), 10)
            .unwrap()
            .len(),
        1
    );
    s.shutdown().unwrap();
    let db = common::raw(&dir);
    assert_eq!(
        db.pragma_query_value::<i64, _>(None, "user_version", |r| r.get(0))
            .unwrap(),
        7
    );
    assert_eq!(
        db.query_row("SELECT key FROM sender_binding_key", [], |r| r
            .get::<_, Vec<u8>>(0))
            .unwrap(),
        key
    );
    assert_eq!(
        db.query_row("SELECT count(*) FROM delivery_artifact", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
}
#[test]
fn receiver_migration_failure_rolls_back_new_artifact_table_and_version() {
    let dir = common::directory();
    common::create(&dir).shutdown().unwrap();
    let db = common::raw(&dir);
    db.execute_batch("DROP TABLE prekey_session; DROP TABLE prekey_received; DROP TABLE prekey_query; DROP TABLE prekey_authority; DROP TABLE prekey_publication; DROP TABLE prekey_inventory; DROP TABLE prekey_clock; DROP TABLE enrollment_complete; DROP TABLE delivery_artifact; DROP TABLE receiver_gap; CREATE TABLE receiver_gap(conflict INTEGER); PRAGMA user_version=4;").unwrap();
    drop(db);
    let result = Storage::open(
        ProfileLease::acquire(dir.path()).unwrap(),
        common::profile(),
        std::sync::Arc::new(common::Provider(Ok(42))),
        Config::default(),
    );
    assert!(matches!(result, Err(StorageError::Migration)));
    let db = common::raw(&dir);
    assert_eq!(
        db.pragma_query_value::<i64, _>(None, "user_version", |r| r.get(0))
            .unwrap(),
        4
    );
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM sqlite_master WHERE name='delivery_artifact'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
    assert_eq!(
        db.query_row("SELECT count(*) FROM sender_binding_key", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        1
    );
}
