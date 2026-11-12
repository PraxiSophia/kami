mod common;
#[test]
fn lifecycle_migration_and_global_otk_exclusivity() {
    let dir = common::directory();
    let store = common::create(&dir);
    let db = common::raw(&dir);
    let version: i64 = db
        .pragma_query_value(None, "user_version", |r| r.get(0))
        .unwrap();
    assert_eq!(version, 7);
    let count: i64 = db
        .query_row("SELECT count(*) FROM prekey_publication", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 0);
    drop(db);
    store.shutdown().unwrap();
    let reopened = common::open(&dir);
    reopened.shutdown().unwrap();
}
