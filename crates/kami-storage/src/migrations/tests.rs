use super::*;
#[test]
fn additive_v2_failure_rolls_back_without_changing_v1_data_or_version() {
    let mut db = Connection::open_in_memory().unwrap();
    db.execute_batch(include_str!("../../migrations/001_initial.sql"))
        .unwrap();
    db.execute_batch("PRAGMA user_version=1; CREATE TABLE contact_trust(conflict INTEGER); INSERT INTO profile VALUES(1,'fixture');").unwrap();
    assert_eq!(migrate(&mut db, false), Err(StorageError::Migration));
    assert_eq!(
        db.pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        db.query_row("SELECT namespace FROM profile", [], |r| r
            .get::<_, String>(0))
            .unwrap(),
        "fixture"
    );
}

#[test]
fn failed_migration_rolls_back_all_schema_and_version_changes() {
    let mut db = Connection::open_in_memory().unwrap();
    db.execute_batch("CREATE TABLE history(already_here INTEGER)")
        .unwrap();
    assert_eq!(migrate(&mut db, true), Err(StorageError::Migration));
    assert_eq!(
        db.pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM sqlite_schema WHERE name='profile'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
}
#[test]
fn initial_migration_is_idempotent_and_unknown_version_refused() {
    let mut db = Connection::open_in_memory().unwrap();
    migrate(&mut db, true).unwrap();
    migrate(&mut db, false).unwrap();
    db.pragma_update(None, "user_version", VERSION + 1).unwrap();
    assert_eq!(migrate(&mut db, false), Err(StorageError::NewerSchema));
}
