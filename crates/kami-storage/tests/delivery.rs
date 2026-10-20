mod common;
use common::*;
use kami_storage::{Acceptance, StorageError};

#[test]
fn receipt_binding_cancel_and_delete_are_distinct_durable_operations() {
    let dir = directory();
    let s = create(&dir);
    let f = facts(1, 1);
    s.enqueue(message(f)).unwrap();
    s.delete_history(f.sender(), f.message_id()).unwrap();
    assert_eq!(s.pending(10).unwrap().len(), 1);
    assert_eq!(
        s.apply_authenticated_receipt(f, f.sender()),
        Err(StorageError::Conflict)
    );
    assert_eq!(s.pending(10).unwrap().len(), 1);
    s.apply_authenticated_receipt(f, f.recipient()).unwrap();
    assert!(s.pending(10).unwrap().is_empty());
    let f2 = facts(2, 2);
    s.enqueue(message(f2)).unwrap();
    s.cancel(f2.sender(), f2.message_id()).unwrap();
    assert!(s.pending(10).unwrap().is_empty());
    assert_eq!(s.history(f2.conversation_id(), 10).unwrap().len(), 1);
    s.delete_history(f2.sender(), f2.message_id()).unwrap();
    s.shutdown().unwrap();
    let db = raw(&dir);
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM message WHERE payload IS NOT NULL",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
}

#[test]
fn ordering_hidden_predecessor_does_not_mask_visible_gap() {
    let dir = directory();
    let s = create(&dir);
    for (sequence, received_at) in [(1, 100), (2, 100), (5, 100), (4, 190)] {
        s.accept(message(facts(sequence, sequence as u64)), received_at)
            .unwrap();
    }
    let snapshot = |s: &kami_storage::Storage, now| {
        s.ordered_history(
            facts(1, 1).conversation_id(),
            facts(1, 1).sender(),
            now,
            50,
            10,
        )
        .unwrap()
        .into_iter()
        .map(|m| (m.message.facts().sequence().value(), m.gap_before))
        .collect::<Vec<_>>()
    };
    // Young sequence 4 is retained but hidden; it cannot close the visible gap.
    assert_eq!(snapshot(&s, 200), [(1, false), (2, false), (5, true)]);
    s.shutdown().unwrap();
    let s = open(&dir);
    // Visibility and its gap survive restart, even if the clock rolls back.
    assert_eq!(snapshot(&s, 199), [(1, false), (2, false), (5, true)]);
    // Once 4 ages out, the gap moves to 4; it must no longer appear before 5.
    assert_eq!(
        snapshot(&s, 240),
        [(1, false), (2, false), (4, true), (5, false)]
    );
    s.accept(message(facts(3, 3)), 241).unwrap();
    // Contiguous late arrival releases immediately and reconciles without duplicates.
    assert_eq!(
        snapshot(&s, 241),
        [(1, false), (2, false), (3, false), (4, false), (5, false)]
    );
    s.shutdown().unwrap();
}

#[test]
fn ordering_waits_then_marks_gap_and_reconciles() {
    let dir = directory();
    let s = create(&dir);
    assert_eq!(
        s.accept(message(facts(1, 1)), 100).unwrap(),
        Acceptance::New
    );
    s.accept(message(facts(3, 3)), 100).unwrap();
    let visible = s
        .ordered_history(
            facts(1, 1).conversation_id(),
            facts(1, 1).sender(),
            110,
            50,
            10,
        )
        .unwrap();
    assert_eq!(visible.len(), 1);
    let visible = s
        .ordered_history(
            facts(1, 1).conversation_id(),
            facts(1, 1).sender(),
            151,
            50,
            10,
        )
        .unwrap();
    assert_eq!(visible.len(), 2);
    assert!(visible[1].gap_before);
    s.accept(message(facts(2, 2)), 152).unwrap();
    let visible = s
        .ordered_history(
            facts(1, 1).conversation_id(),
            facts(1, 1).sender(),
            153,
            50,
            10,
        )
        .unwrap();
    assert_eq!(visible.len(), 3);
    assert!(visible.iter().all(|m| !m.gap_before));
}
