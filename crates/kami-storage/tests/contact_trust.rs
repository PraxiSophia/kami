mod common;
use kami_storage::{ContactTrustRow, StorageError, TrustState};

#[test]
fn wrong_profile_cannot_mutate_v1_schema_before_identity_check() {
    let dir = common::directory();
    common::create(&dir).shutdown().unwrap();
    let db = common::raw(&dir);
    db.execute_batch("DROP TABLE contact_trust; PRAGMA user_version=1;")
        .unwrap();
    drop(db);
    let wrong = kami_key_provider::ProfileKeyId::new("1123456789abcdef0123456789abcdef").unwrap();
    let result = kami_storage::Storage::open(
        kami_storage::ProfileLease::acquire(dir.path()).unwrap(),
        wrong,
        std::sync::Arc::new(common::Provider(Ok(42))),
        kami_storage::Config::default(),
    );
    assert!(matches!(result, Err(StorageError::WrongKeyOrCorrupt)));
    assert_eq!(
        common::raw(&dir)
            .pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
}

#[test]
fn encrypted_v1_upgrade_preserves_existing_delivery_and_adds_contacts() {
    let dir = common::directory();
    let storage = common::create(&dir);
    storage
        .enqueue(common::message(common::facts(1, 1)))
        .unwrap();
    storage.shutdown().unwrap();
    // Reconstruct the exact pre-KAMI-50 schema while retaining encrypted v1 data.
    let db = common::raw(&dir);
    db.execute_batch("DROP TABLE contact_trust; PRAGMA user_version=1;")
        .unwrap();
    drop(db);
    let storage = common::open(&dir);
    assert_eq!(storage.pending(1).unwrap()[0].facts(), common::facts(1, 1));
    assert_eq!(storage.load_contact([1; 32]).unwrap(), None);
    let pending = ContactTrustRow::new([1; 32], TrustState::Pending, 1, None, vec![2; 40]).unwrap();
    storage
        .compare_and_swap_contact(None, pending.clone())
        .unwrap();
    storage.shutdown().unwrap();
    assert_eq!(
        common::raw(&dir)
            .pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))
            .unwrap(),
        2
    );
    assert_eq!(
        common::open(&dir).load_contact([1; 32]).unwrap(),
        Some(pending)
    );
}

#[test]
fn row_bounds_generations_and_redaction_are_checked() {
    for candidate in [vec![], vec![0; 513]] {
        assert_eq!(
            ContactTrustRow::new([1; 32], TrustState::Pending, 1, None, candidate),
            Err(StorageError::InvalidInput)
        );
    }
    for generation in [0, u64::MAX] {
        assert!(
            ContactTrustRow::new([1; 32], TrustState::Pending, generation, None, vec![1]).is_err()
        );
    }
    assert!(ContactTrustRow::new([1; 32], TrustState::Verified, 1, None, vec![1]).is_err());
    assert!(
        ContactTrustRow::new([1; 32], TrustState::Verified, 1, Some(vec![2]), vec![1]).is_err()
    );
    assert!(ContactTrustRow::new([1; 32], TrustState::Changed, 1, None, vec![1]).is_err());
    let dir = common::directory();
    let storage = common::create(&dir);
    let row = ContactTrustRow::new([1; 32], TrustState::Pending, 2, None, vec![1]).unwrap();
    assert_eq!(
        storage.compare_and_swap_contact(None, row.clone()),
        Err(StorageError::InvalidInput)
    );
    assert_eq!(format!("{row:?}"), "ContactTrustRow([REDACTED])");
}

#[test]
fn concurrent_cas_cannot_lose_a_block_or_change() {
    let dir = common::directory();
    let storage = common::create(&dir);
    storage
        .compare_and_swap_contact(
            None,
            ContactTrustRow::new([1; 32], TrustState::Pending, 1, None, vec![1]).unwrap(),
        )
        .unwrap();
    let block = ContactTrustRow::new([1; 32], TrustState::Blocked, 2, None, vec![1]).unwrap();
    let verify =
        ContactTrustRow::new([1; 32], TrustState::Verified, 2, Some(vec![1]), vec![1]).unwrap();
    let barrier = std::sync::Barrier::new(2);
    std::thread::scope(|scope| {
        let a = scope.spawn(|| {
            barrier.wait();
            storage.compare_and_swap_contact(Some(1), block)
        });
        let b = scope.spawn(|| {
            barrier.wait();
            storage.compare_and_swap_contact(Some(1), verify)
        });
        let (a, b) = (a.join().unwrap(), b.join().unwrap());
        assert!(
            (a == Ok(()) && b == Err(StorageError::Conflict))
                || (b == Ok(()) && a == Err(StorageError::Conflict))
        );
    });
    assert_eq!(
        storage.load_contact([1; 32]).unwrap().unwrap().generation(),
        2
    );
}

#[test]
fn malformed_durable_row_is_rejected_before_unbounded_copy() {
    let dir = common::directory();
    let storage = common::create(&dir);
    storage
        .compare_and_swap_contact(
            None,
            ContactTrustRow::new([1; 32], TrustState::Pending, 1, None, vec![1]).unwrap(),
        )
        .unwrap();
    let db = common::raw(&dir);
    db.execute_batch(
        "PRAGMA ignore_check_constraints=ON; UPDATE contact_trust SET candidate=zeroblob(100000);",
    )
    .unwrap();
    assert_eq!(
        storage.load_contact([1; 32]),
        Err(StorageError::WrongKeyOrCorrupt)
    );
}

#[test]
fn contact_cas_preserves_pin_candidate_and_sticky_block_across_restart() {
    let dir = common::directory();
    let storage = common::create(&dir);
    assert_eq!(storage.load_contact([1; 32]).unwrap(), None);
    let pending = ContactTrustRow::new([1; 32], TrustState::Pending, 1, None, vec![2; 40]).unwrap();
    storage
        .compare_and_swap_contact(None, pending.clone())
        .unwrap();
    assert_eq!(
        storage.compare_and_swap_contact(None, pending),
        Err(StorageError::Conflict)
    );
    let verified = ContactTrustRow::new(
        [1; 32],
        TrustState::Verified,
        2,
        Some(vec![2; 40]),
        vec![2; 40],
    )
    .unwrap();
    storage.compare_and_swap_contact(Some(1), verified).unwrap();
    let blocked = ContactTrustRow::new(
        [1; 32],
        TrustState::Blocked,
        3,
        Some(vec![2; 40]),
        vec![3; 40],
    )
    .unwrap();
    storage
        .compare_and_swap_contact(Some(2), blocked.clone())
        .unwrap();
    let stale = ContactTrustRow::new(
        [1; 32],
        TrustState::Verified,
        3,
        Some(vec![2; 40]),
        vec![2; 40],
    )
    .unwrap();
    assert_eq!(
        storage.compare_and_swap_contact(Some(2), stale),
        Err(StorageError::Conflict)
    );
    storage.shutdown().unwrap();
    let reopened = common::open(&dir);
    assert_eq!(reopened.load_contact([1; 32]).unwrap(), Some(blocked));
}
