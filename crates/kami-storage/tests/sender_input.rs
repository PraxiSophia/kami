mod common;
use kami_core::{ConversationId, DeviceId, MessageId};
use kami_storage::{SendRequest, StorageError};

#[test]
fn preparation_quota_retains_conflict_tombstones_and_releases_uncommitted_lanes() {
    let Some(_native) = common::native::isolated() else {
        return;
    };
    let dir = common::directory();
    let storage = common::fresh_create_with_config(
        &dir,
        kami_storage::Config {
            outbox_messages: 1,
            ..Default::default()
        },
    );
    let make = |id| {
        SendRequest::new(
            MessageId::from_u128(id),
            ConversationId::from_u128(id),
            DeviceId::from_bytes([1; 32]),
            DeviceId::from_bytes([2; 32]),
            1,
            "text".into(),
        )
        .unwrap()
    };
    let first = make(1);
    let old = storage.prepare_send(first.clone(), 0).unwrap();
    assert_eq!(storage.prepare_send(make(2), 0), Err(StorageError::Full));
    storage.cleanup_prepared(60_000).unwrap();
    assert_eq!(
        storage.prepare_send(make(2), 60_000),
        Err(StorageError::Full)
    );
    let renewed = storage.prepare_send(first, 60_000).unwrap();
    assert_ne!(old, renewed);
    assert_eq!(
        common::raw(&dir)
            .query_row("SELECT count(*) FROM sender_sequence", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
    storage.shutdown().unwrap();
}

#[test]
fn modified_request_fields_are_revalidated_at_storage_boundary() {
    let Some(_native) = common::native::isolated() else {
        return;
    };
    let dir = common::directory();
    let storage = common::fresh_create(&dir);
    let request = || {
        SendRequest::new(
            MessageId::from_u128(1),
            ConversationId::from_u128(1),
            DeviceId::from_bytes([1; 32]),
            DeviceId::from_bytes([2; 32]),
            1,
            "text".into(),
        )
        .unwrap()
    };
    let mut invalid = request();
    invalid.recipient = invalid.sender;
    assert_eq!(
        storage.prepare_send(invalid, 1),
        Err(StorageError::InvalidInput)
    );
    let mut invalid = request();
    invalid.created_at_ms = u64::MAX;
    assert_eq!(
        storage.prepare_send(invalid, 1),
        Err(StorageError::InvalidInput)
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
