mod common;
use kami_core::{ConversationId, DeviceId, MessageId};
use kami_storage::{SendRequest, StorageError};

#[test]
fn generic_creation_cannot_reserve_a_new_cryptographic_transition() {
    let dir = common::directory();
    let storage = common::create(&dir);
    let request = SendRequest::new(
        MessageId::from_u128(1),
        ConversationId::from_u128(1),
        DeviceId::from_bytes([1; 32]),
        DeviceId::from_bytes([2; 32]),
        1,
        "text".into(),
    )
    .unwrap();
    assert_eq!(
        storage.prepare_send(request, 1),
        Err(StorageError::EnrollmentRequired)
    );
    assert_eq!(
        common::raw(&dir)
            .query_row("SELECT count(*) FROM sender_prepare", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
    storage.shutdown().unwrap();
}

#[test]
fn native_handoff_is_one_use_and_reopen_never_restores_authority() {
    let Some(_native) = common::native::isolated() else {
        return;
    };
    let dir = common::directory();
    let storage = common::fresh_create(&dir);
    assert_eq!(
        storage.recovery_status().unwrap(),
        kami_storage::RecoveryStatus::FreshEnrollment
    );
    storage.prepare_send(request(1), 1).unwrap();
    storage.shutdown().unwrap();
    let storage = common::open(&dir);
    assert_eq!(
        storage.recovery_status().unwrap(),
        kami_storage::RecoveryStatus::Disabled
    );
    assert_eq!(
        storage.prepare_send(request(1), 1),
        Err(StorageError::EnrollmentRequired)
    );
    assert_eq!(
        storage.prepare_send(request(2), 1),
        Err(StorageError::EnrollmentRequired)
    );
    let preconditions = storage.recovery_status().unwrap().preconditions();
    assert!(preconditions.independently_fresh_enrollment);
    assert!(preconditions.fresh_account_and_prekeys);
    assert!(preconditions.new_session_identity);
    assert!(preconditions.independently_verified_contact);
}
fn request(id: u128) -> SendRequest {
    SendRequest::new(
        MessageId::from_u128(id),
        ConversationId::from_u128(1),
        DeviceId::from_bytes([1; 32]),
        DeviceId::from_bytes([2; 32]),
        1,
        "text".into(),
    )
    .unwrap()
}

#[test]
fn fresh_proof_for_another_profile_cannot_enable_this_worker() {
    let Some(_native) = common::native::isolated() else {
        return;
    };
    let dir = common::directory();
    let other = common::directory();
    let profile = common::native::new_profile(other.path());
    let native = kami_identity::NativeCustody::open();
    let proof = kami_identity::begin_fresh_native(&native, &profile).unwrap();
    let storage = common::create(&dir);
    assert!(storage.enroll_fresh(proof).is_err());
    assert_eq!(
        storage.recovery_status().unwrap(),
        kami_storage::RecoveryStatus::Quarantined
    );
    assert_eq!(
        storage.prepare_send(request(1), 1),
        Err(StorageError::EnrollmentRequired)
    );
}

#[test]
fn ambiguous_storage_failure_quarantines_the_worker() {
    let Some(_native) = common::native::isolated() else {
        return;
    };
    let dir = common::directory();
    let storage = common::fresh_create(&dir);
    common::raw(&dir)
        .execute_batch("ALTER TABLE sender_prepare RENAME TO missing_prepare")
        .unwrap();
    assert_eq!(storage.prepare_send(request(1), 1), Err(StorageError::Io));
    assert_eq!(
        storage.recovery_status().unwrap(),
        kami_storage::RecoveryStatus::Quarantined
    );
    common::raw(&dir)
        .execute_batch("ALTER TABLE missing_prepare RENAME TO sender_prepare")
        .unwrap();
    assert_eq!(
        storage.prepare_send(request(1), 1),
        Err(StorageError::EnrollmentRequired)
    );
}

#[test]
fn native_fresh_proof_cannot_authorize_a_database_with_a_different_key() {
    let Some(_native) = common::native::isolated() else {
        return;
    };
    let dir = common::directory();
    let profile = common::native::new_profile(dir.path());
    let native = kami_identity::NativeCustody::open();
    let fresh = kami_identity::begin_fresh_native(&native, &profile).unwrap();
    let storage = kami_storage::Storage::create(
        kami_storage::ProfileLease::acquire(dir.path()).unwrap(),
        profile,
        std::sync::Arc::new(common::Provider(Ok(42))),
        Default::default(),
    )
    .unwrap();
    assert!(storage.enroll_fresh(fresh).is_err());
    assert_eq!(
        storage.recovery_status().unwrap(),
        kami_storage::RecoveryStatus::Quarantined
    );
    assert_eq!(
        storage.prepare_send(request(1), 1),
        Err(StorageError::EnrollmentRequired)
    );
}

#[test]
fn completion_bytes_and_a_static_native_anchor_do_not_enable_a_reopened_worker() {
    let Some(_native) = common::native::isolated() else {
        return;
    };
    let dir = common::directory();
    let profile = common::native::new_profile(dir.path());
    let native = kami_identity::NativeCustody::open();
    let fresh = kami_identity::begin_fresh_native(&native, &profile).unwrap();
    let storage = kami_storage::Storage::create(
        kami_storage::ProfileLease::acquire(dir.path()).unwrap(),
        profile.clone(),
        std::sync::Arc::new(kami_identity::NativeCustody::open()),
        Default::default(),
    )
    .unwrap();
    let row = storage
        .commit_fresh_enrollment(fresh.canonical_row())
        .unwrap();
    assert_eq!(
        storage.prepare_send(request(1), 1),
        Err(StorageError::EnrollmentRequired)
    );
    kami_identity::complete_fresh_enrollment(&native, fresh, &row).unwrap();
    let _static_proof =
        kami_identity::verify_enrollment_for_transport(&native, &profile, &row).unwrap();
    assert_eq!(
        storage.prepare_send(request(1), 1),
        Err(StorageError::EnrollmentRequired)
    );
    storage.shutdown().unwrap();
    let storage = common::open(&dir);
    assert_eq!(
        storage.recovery_status().unwrap(),
        kami_storage::RecoveryStatus::Disabled
    );
    assert_eq!(
        storage.prepare_send(request(1), 1),
        Err(StorageError::EnrollmentRequired)
    );
}
