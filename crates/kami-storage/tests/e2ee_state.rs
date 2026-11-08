mod common;

use kami_storage::{ContactTrustRow, StorageError, TrustState};

#[test]
fn encrypted_v2_upgrade_and_reopen_preserve_contact_and_outbox() {
    let dir = common::directory();
    let storage = common::create(&dir);
    let contact = ContactTrustRow::new([2; 32], TrustState::Pending, 1, None, vec![3]).unwrap();
    storage
        .compare_and_swap_contact(None, contact.clone())
        .unwrap();
    storage
        .enqueue(common::message(common::facts(1, 1)))
        .unwrap();
    storage.shutdown().unwrap();
    let db = common::raw(&dir);
    db.execute_batch(
        "DROP TABLE enrollment_complete; DROP TABLE delivery_artifact; DROP TABLE receiver_gap; DROP TABLE sender_attempt; DROP TABLE sender_prepare; DROP TABLE sender_binding_key; DROP TABLE e2ee_frame; DROP TABLE e2ee_session; DROP TABLE e2ee_account; \
         PRAGMA user_version=2;",
    )
    .unwrap();
    drop(db);
    for _ in 0..2 {
        let storage = common::open(&dir);
        assert_eq!(
            storage.load_contact([2; 32]).unwrap(),
            Some(contact.clone())
        );
        assert_eq!(storage.pending(1).unwrap()[0].facts(), common::facts(1, 1));
        storage.shutdown().unwrap();
        assert_eq!(
            common::raw(&dir)
                .pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))
                .unwrap(),
            6
        );
    }
}

#[test]
fn encrypted_v3_upgrade_failure_preserves_v2_schema_and_data() {
    let dir = common::directory();
    let storage = common::create(&dir);
    storage
        .enqueue(common::message(common::facts(1, 1)))
        .unwrap();
    storage.shutdown().unwrap();
    let db = common::raw(&dir);
    db.execute_batch(
        "DROP TABLE enrollment_complete; DROP TABLE delivery_artifact; DROP TABLE receiver_gap; DROP TABLE sender_attempt; DROP TABLE sender_prepare; DROP TABLE sender_binding_key; DROP TABLE e2ee_frame; DROP TABLE e2ee_session; DROP TABLE e2ee_account; \
         PRAGMA user_version=2; CREATE TABLE e2ee_session(conflict INTEGER);",
    )
    .unwrap();
    drop(db);
    let result = kami_storage::Storage::open(
        kami_storage::ProfileLease::acquire(dir.path()).unwrap(),
        common::profile(),
        std::sync::Arc::new(common::Provider(Ok(42))),
        kami_storage::Config::default(),
    );
    assert!(matches!(result, Err(StorageError::Migration)));
    let db = common::raw(&dir);
    assert_eq!(
        db.pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))
            .unwrap(),
        2
    );
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM sqlite_schema WHERE name='e2ee_account'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
    assert_eq!(
        db.query_row("SELECT count(*) FROM message", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
}
