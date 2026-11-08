mod common;
use common::*;
use kami_storage::StorageError;

#[test]
fn complete_row_is_insert_once_fresh_only_and_survives_reopen() {
    let dir = directory();
    let store = create(&dir);
    assert_eq!(store.enrollment_snapshot().unwrap(), None);
    let mut row = vec![1, 1];
    row.extend_from_slice(profile().as_str().as_bytes());
    // Valid Ed25519 public key; evidence only, never enough to mint native proof.
    row.extend_from_slice(&[
        0x58, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66,
        0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66,
        0x66, 0x66,
    ]);
    assert_eq!(
        store.commit_fresh_enrollment(&row[..65]),
        Err(StorageError::InvalidInput)
    );
    assert_eq!(store.commit_fresh_enrollment(&row).unwrap(), row);
    assert!(store.commit_fresh_enrollment(&row).is_err());
    store.shutdown().unwrap();
    let store = open(&dir);
    assert_eq!(store.enrollment_snapshot().unwrap(), Some(row.clone()));
    assert!(store.commit_fresh_enrollment(&row).is_err());
    store.shutdown().unwrap();
    let db = raw(&dir);
    assert!(db.execute("DELETE FROM enrollment_complete", []).is_err());
    assert!(db
        .execute("UPDATE enrollment_complete SET canonical=x'00'", [])
        .is_err());
}
#[test]
fn marker_only_profile_is_never_backfilled_or_adopted() {
    let dir = directory();
    create(&dir).shutdown().unwrap();
    let store = open(&dir);
    assert_eq!(store.enrollment_snapshot().unwrap(), None);
    assert!(store.commit_fresh_enrollment(&[0; 66]).is_err());
}
