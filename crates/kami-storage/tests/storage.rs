mod common;
use common::*;
use kami_core::{ContentBinding, MessageFacts};
use kami_storage::{Acceptance, Config, ProfileLease, Storage, StorageError, StoredMessage};
use std::sync::Arc;

#[test]
fn durable_outbox_allocates_sequence_and_replays_identical_material() {
    let dir = directory();
    let store = create(&dir);
    let f = facts(1, 1);
    assert_eq!(
        store
            .next_sender_sequence(f.conversation_id(), f.sender())
            .unwrap()
            .value(),
        1
    );
    store
        .enqueue(StoredMessage::new(f, b"protected retransmission".to_vec()).unwrap())
        .unwrap();
    assert_eq!(store.pending(10).unwrap()[0].facts(), f);
    assert_eq!(
        store.enqueue(StoredMessage::new(facts(2, 3), vec![1]).unwrap()),
        Err(StorageError::Conflict)
    );
    store.shutdown().unwrap();
    let store = open(&dir);
    assert_eq!(
        store
            .next_sender_sequence(f.conversation_id(), f.sender())
            .unwrap()
            .value(),
        2
    );
    assert_eq!(
        store.pending(10).unwrap()[0].payload(),
        b"protected retransmission"
    );
}
#[test]
fn accept_commits_dedup_and_receipt_before_ack_and_history_delete_retains_evidence() {
    let dir = directory();
    let store = create(&dir);
    let f = facts(7, 1);
    assert_eq!(store.accept(message(f), 100).unwrap(), Acceptance::New);
    assert_eq!(store.receipt(f.sender(), f.message_id()).unwrap(), Some(f));
    store.delete_history(f.sender(), f.message_id()).unwrap();
    store.shutdown().unwrap();
    let store = open(&dir);
    assert_eq!(
        store.accept(message(f), 101).unwrap(),
        Acceptance::Duplicate
    );
    assert!(store.history(f.conversation_id(), 10).unwrap().is_empty());
    let conflicting = MessageFacts::new(
        f.message_id(),
        f.conversation_id(),
        f.sender(),
        f.recipient(),
        f.sequence(),
        ContentBinding::from_bytes([4; 32]),
    );
    assert_eq!(
        store.accept(message(conflicting), 102),
        Err(StorageError::Conflict)
    );
}
#[test]
fn encrypted_create_reopen_and_exclusive_lease() {
    let dir = directory();
    let storage = Storage::create(
        ProfileLease::acquire(dir.path()).unwrap(),
        profile(),
        Arc::new(Provider(Ok(42))),
        Config::default(),
    )
    .unwrap();
    assert!(ProfileLease::acquire(dir.path()).is_err());
    assert!(storage.cipher_version().unwrap().starts_with("4."));
    println!("SQLCipher runtime: {}", storage.cipher_version().unwrap());
    storage.shutdown().unwrap();
    let bytes = std::fs::read(dir.path().join("messages.db")).unwrap();
    assert_ne!(&bytes[..16], b"SQLite format 3\0");
    open(&dir).shutdown().unwrap();
}
