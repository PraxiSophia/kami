mod common;
use kami_core::{ConversationId, DeviceId, MessageId};
use kami_storage::{Preparation, SendRequest, StorageError};
fn request(id: u128, text: &str) -> SendRequest {
    SendRequest::new(
        MessageId::from_u128(id),
        ConversationId::from_u128(2),
        DeviceId::from_bytes([1; 32]),
        DeviceId::from_bytes([2; 32]),
        1000,
        text.to_owned(),
    )
    .unwrap()
}
#[test]
fn reservation_is_single_lane_durable_and_keyed() {
    let Some(_native) = common::native::isolated() else {
        return;
    };
    let dir = common::directory();
    let s = common::fresh_create(&dir);
    let Preparation::Prepared(first) = s.prepare_send(request(10, "private text"), 1000).unwrap()
    else {
        panic!()
    };
    assert_eq!(first.sequence().value(), 1);
    assert!(matches!(
        s.prepare_send(request(11, "later"), 1000),
        Err(StorageError::Busy)
    ));
    assert!(matches!(
        s.prepare_send(request(10, "changed"), 1000),
        Err(StorageError::Conflict)
    ));
    let Preparation::Prepared(recovered) =
        s.prepare_send(request(10, "private text"), 1001).unwrap()
    else {
        panic!()
    };
    assert_eq!(first, recovered);
    let db = common::raw(&dir);
    let mac: Vec<u8> = db
        .query_row("SELECT request_mac FROM sender_prepare", [], |r| r.get(0))
        .unwrap();
    assert_eq!(mac.len(), 32);
    let other_dir = common::directory();
    let other = common::fresh_create(&other_dir);
    other
        .prepare_send(request(10, "private text"), 1000)
        .unwrap();
    let other_mac: Vec<u8> = common::raw(&other_dir)
        .query_row("SELECT request_mac FROM sender_prepare", [], |r| r.get(0))
        .unwrap();
    assert_ne!(
        mac, other_mac,
        "request binding is keyed per encrypted database"
    );
    other.shutdown().unwrap();
    for changed in 0..5 {
        let mut conflict = request(10, "private text");
        match changed {
            0 => conflict.conversation = ConversationId::from_u128(3),
            1 => conflict.recipient = DeviceId::from_bytes([3; 32]),
            2 => conflict.created_at_ms += 1,
            3 => conflict.message = MessageId::from_u128(12),
            _ => conflict = request(10, "private texu"),
        }
        let expected = if changed == 3 {
            StorageError::Busy
        } else {
            StorageError::Conflict
        };
        assert_eq!(s.prepare_send(conflict, 1001), Err(expected));
    }
    assert!(!format!("{:?}", request(10, "private text")).contains("private text"));
    s.shutdown().unwrap();
    let s = common::open(&dir);
    assert_eq!(
        s.prepare_send(request(10, "private text"), 1002),
        Err(StorageError::EnrollmentRequired)
    );
    assert_eq!(
        common::raw(&dir)
            .query_row("SELECT state FROM sender_prepare", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
    s.shutdown().unwrap();
}

#[test]
fn expired_preparation_can_resume_same_request_but_rotates_fence() {
    let Some(_native) = common::native::isolated() else {
        return;
    };
    let dir = common::directory();
    let s = common::fresh_create(&dir);
    let old = s.prepare_send(request(10, "same"), 1000).unwrap();
    s.cleanup_prepared(61_000).unwrap();
    let renewed = s.prepare_send(request(10, "same"), 61_000).unwrap();
    assert_ne!(old, renewed);
    let Preparation::Prepared(p) = renewed else {
        panic!()
    };
    assert_eq!(p.sequence().value(), 1);
    s.shutdown().unwrap();
}
#[test]
fn cleanup_releases_only_uncommitted_sequence_and_blocks_legacy_overtake() {
    let Some(_native) = common::native::isolated() else {
        return;
    };
    let dir = common::directory();
    let s = common::fresh_create(&dir);
    let old = s.prepare_send(request(10, "old"), 1000).unwrap();
    assert_eq!(
        s.enqueue(common::message(common::facts(20, 1))),
        Err(StorageError::Busy)
    );
    assert_eq!(s.cleanup_prepared(60_999).unwrap(), 0);
    assert_eq!(s.cleanup_prepared(61_000).unwrap(), 1);
    let Preparation::Prepared(next) = s.prepare_send(request(11, "next"), 61_000).unwrap() else {
        panic!()
    };
    assert_eq!(next.sequence().value(), 1);
    assert_ne!(old, Preparation::Prepared(next));
    assert!(matches!(
        s.prepare_send(request(10, "changed"), 61_000),
        Err(StorageError::Conflict)
    ));
    s.shutdown().unwrap();
}
#[test]
fn sender_schema_is_durable_across_reopen() {
    let Some(_native) = common::native::isolated() else {
        return;
    };
    let dir = common::directory();
    let s = common::fresh_create(&dir);
    s.shutdown().unwrap();
    let s = common::open(&dir);
    let db = common::raw(&dir);
    assert_eq!(
        db.pragma_query_value::<i64, _>(None, "user_version", |r| r.get(0))
            .unwrap(),
        7
    );
    for table in ["sender_prepare", "sender_attempt", "sender_binding_key"] {
        assert_eq!(
            db.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            if table == "sender_binding_key" { 1 } else { 0 }
        );
    }
    s.shutdown().unwrap();
}
