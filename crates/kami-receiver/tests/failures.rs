use super::*;
use kami_core::SenderSequence;
use kami_storage::{Acceptance, Storage, StorageError};
struct Pair {
    _sa: Storage,
    sb: Storage,
    _ad: tempfile::TempDir,
    bd: tempfile::TempDir,
    sender: QuarantinedSession,
    bob: Account,
    a: Endpoint,
    b: Endpoint,
    ta: ContactTrust,
    tb: ContactTrust,
    aid: DeviceId,
    bid: DeviceId,
}
impl Pair {
    fn new() -> Self {
        Self::with_config(Default::default())
    }
    fn with_config(config: kami_storage::Config) -> Self {
        let mut alice = Account::new();
        alice.generate_one_time_keys(1);
        let mut bob = Account::new();
        bob.generate_one_time_keys(1);
        let (a, ta, aid) = endpoint(&alice, 7);
        let (b, tb, bid) = endpoint(&bob, 8);
        let ad = common::directory();
        let bd = common::directory();
        let sa = common::fresh_create(&ad);
        let sb = common::fresh_create_with_config(&bd, config);
        seed(&sa, &tb, bid);
        seed(&sb, &ta, aid);
        let sender = QuarantinedSession::outbound(
            alice,
            a.clone(),
            b.clone(),
            ConversationId::from_u128(1),
            &tb,
            &tb.pinned().unwrap().binding().peer_id(),
            1000,
        )
        .unwrap();
        Self {
            _sa: sa,
            sb,
            _ad: ad,
            bd,
            sender,
            bob,
            a,
            b,
            ta,
            tb,
            aid,
            bid,
        }
    }
    fn frame(&self, id: u128, seq: u64, text: &str) -> (Vec<u8>, QuarantinedSession) {
        let (stage, proposed) = self
            .sender
            .prepare_text(
                MessageId::from_u128(id),
                SenderSequence::new(seq),
                1000,
                text,
                &self.tb,
                &self.tb.pinned().unwrap().binding().peer_id(),
                1000,
            )
            .unwrap();
        (stage.into_material().frame().to_vec(), proposed)
    }
    fn stage(&self, frame: &[u8]) -> kami_e2ee::AuthenticatedDelivery {
        self.stage_pair(frame).0
    }
    fn stage_pair(&self, frame: &[u8]) -> (kami_e2ee::AuthenticatedDelivery, QuarantinedSession) {
        let outer = kami_protocol::decode_frame(frame).unwrap();
        let expected = kami_protocol::ExpectedContext {
            message_id: outer.message_id(),
            conversation_id: outer.conversation_id(),
            sender: self.aid,
            recipient: self.bid,
            receipt_target: None,
        };
        QuarantinedSession::inbound_delivery(
            Account::from_pickle(self.bob.pickle()),
            self.b.clone(),
            self.a.clone(),
            ConversationId::from_u128(1),
            frame,
            &expected,
            MessageId::from_u128(88),
            &self.ta,
            &self.ta.pinned().unwrap().binding().peer_id(),
            1000,
        )
        .unwrap()
    }
    fn counts(&self) -> [i64; 6] {
        let db = common::raw(&self.bd);
        [
            "e2ee_account",
            "e2ee_session",
            "message",
            "e2ee_frame",
            "delivery_artifact",
            "receiver_gap",
        ]
        .map(|table| {
            db.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
                .unwrap()
        })
    }
}
#[test]
fn receiver_faults_roll_back_prekey_session_inbox_and_receipt_together() {
    let Some(_native) = common::native::isolated() else {
        return;
    };
    for table in [
        "e2ee_account",
        "e2ee_session",
        "message",
        "history",
        "receipt",
        "e2ee_frame",
        "delivery_artifact",
        "receiver_gap",
    ] {
        let p = Pair::new();
        let (frame, _) = p.frame(1, 3, "private fault payload");
        common::raw(&p.bd).execute_batch(&format!("CREATE TRIGGER injected BEFORE INSERT ON {table} BEGIN SELECT RAISE(ABORT,'injected'); END")).unwrap();
        assert!(p.sb.commit_delivery(p.stage(&frame)).is_err(), "{table}");
        assert_eq!(p.counts(), [0; 6], "{table}");
        common::raw(&p.bd)
            .execute_batch("DROP TRIGGER injected")
            .unwrap();
        assert_eq!(
            p.sb.commit_delivery(p.stage(&frame)).unwrap().acceptance(),
            Acceptance::New
        );
    }
}
#[test]
fn concurrent_duplicates_keep_one_artifact_and_no_losing_ratchet() {
    let Some(_native) = common::native::isolated() else {
        return;
    };
    let p = Pair::new();
    let (frame, _) = p.frame(1, 1, "one message");
    let stages: Vec<_> = (0..6).map(|_| p.stage(&frame)).collect();
    let results = std::thread::scope(|scope| {
        let handles: Vec<_> = stages
            .into_iter()
            .map(|stage| {
                let sb = &p.sb;
                scope.spawn(move || sb.commit_delivery(stage).unwrap())
            })
            .collect();
        handles
            .into_iter()
            .map(|h| h.join().unwrap())
            .collect::<Vec<_>>()
    });
    assert_eq!(
        results
            .iter()
            .filter(|r| r.acceptance() == Acceptance::New)
            .count(),
        1
    );
    assert!(results.iter().all(|r| r.receipt() == results[0].receipt()));
    assert_eq!(p.counts(), [1, 1, 1, 1, 1, 0]);
    assert_eq!(
        common::raw(&p.bd)
            .query_row("SELECT generation FROM e2ee_session", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        2
    );
}
#[test]
fn conflicting_content_and_alternate_ciphertext_never_replace_first_material() {
    let Some(_native) = common::native::isolated() else {
        return;
    };
    let p = Pair::new();
    let (frame, advanced) = p.frame(1, 1, "original");
    let accepted = p.sb.commit_delivery(p.stage(&frame)).unwrap();
    let before = p.counts();
    let (changed, _) = p.frame(1, 1, "different authenticated plaintext");
    assert!(p.sb.commit_delivery(p.stage(&changed)).is_err());
    let (alternate, _) = advanced
        .prepare_text(
            MessageId::from_u128(1),
            SenderSequence::new(1),
            1000,
            "original",
            &p.tb,
            &p.tb.pinned().unwrap().binding().peer_id(),
            1000,
        )
        .unwrap();
    let alternate = alternate.into_material().frame().to_vec();
    let permit = || {
        kami_e2ee::ReplayPermit::new(
            &p.ta,
            &p.ta.pinned().unwrap().binding().peer_id(),
            p.aid,
            p.bid,
            ConversationId::from_u128(1),
        )
        .unwrap()
    };
    assert!(p.sb.recover_delivery(permit(), alternate).is_err());
    let recovered = p.sb.recover_delivery(permit(), frame).unwrap().unwrap();
    assert_eq!(recovered.receipt(), accepted.receipt());
    assert_eq!(p.counts(), before);
}
#[test]
fn trust_race_fences_commit_and_reack_resumes_only_for_original_pin() {
    let Some(_native) = common::native::isolated() else {
        return;
    };
    let p = Pair::new();
    let (frame, _) = p.frame(1, 1, "trusted");
    let stage = p.stage(&frame);
    let row = p.sb.load_contact(*p.aid.as_bytes()).unwrap().unwrap();
    let update = |state, generation| {
        ContactTrustRow::new(
            *p.aid.as_bytes(),
            state,
            generation,
            row.pinned().map(|p| p.to_vec()),
            row.candidate().to_vec(),
        )
        .unwrap()
    };
    p.sb.compare_and_swap_contact(Some(2), update(TrustState::Blocked, 3))
        .unwrap();
    assert_eq!(
        p.sb.commit_delivery(stage).unwrap_err(),
        StorageError::Conflict
    );
    assert_eq!(p.counts(), [0; 6]);
    let q = Pair::new();
    let (frame, _) = q.frame(1, 1, "committed");
    let ack = q.sb.commit_delivery(q.stage(&frame)).unwrap();
    let blocked = q.ta.transition(TrustTransition::Block).unwrap();
    let row = q.sb.load_contact(*q.aid.as_bytes()).unwrap().unwrap();
    q.sb.compare_and_swap_contact(
        Some(2),
        ContactTrustRow::new(
            *q.aid.as_bytes(),
            TrustState::Blocked,
            3,
            row.pinned().map(|p| p.to_vec()),
            row.candidate().to_vec(),
        )
        .unwrap(),
    )
    .unwrap();
    assert!(kami_e2ee::ReplayPermit::new(
        &blocked,
        &q.ta.pinned().unwrap().binding().peer_id(),
        q.aid,
        q.bid,
        ConversationId::from_u128(1)
    )
    .is_err());
    let unblocked = blocked.transition(TrustTransition::Unblock).unwrap();
    q.sb.compare_and_swap_contact(
        Some(3),
        ContactTrustRow::new(
            *q.aid.as_bytes(),
            TrustState::Changed,
            4,
            row.pinned().map(|p| p.to_vec()),
            row.candidate().to_vec(),
        )
        .unwrap(),
    )
    .unwrap();
    let evidence = VerificationEvidence::independent_comparison(
        &unblocked,
        unblocked.candidate().fingerprint(),
        true,
    )
    .unwrap();
    let verified = unblocked
        .transition(TrustTransition::Verify(evidence))
        .unwrap();
    q.sb.compare_and_swap_contact(
        Some(4),
        ContactTrustRow::new(
            *q.aid.as_bytes(),
            TrustState::Verified,
            5,
            Some(verified.pinned().unwrap().to_bytes()),
            verified.candidate().to_bytes(),
        )
        .unwrap(),
    )
    .unwrap();
    let permit = kami_e2ee::ReplayPermit::new(
        &verified,
        &verified.pinned().unwrap().binding().peer_id(),
        q.aid,
        q.bid,
        ConversationId::from_u128(1),
    )
    .unwrap();
    assert_eq!(
        q.sb.recover_delivery(permit, frame)
            .unwrap()
            .unwrap()
            .receipt(),
        ack.receipt()
    );
}
#[test]
fn permanent_gap_reconciles_a_late_predecessor_without_new_presentation_identity() {
    let Some(_native) = common::native::isolated() else {
        return;
    };
    let mut p = Pair::new();
    let (first, sender) = p.frame(1, 1, "first");
    let (stage, receiver) = p.stage_pair(&first);
    let first_ack = p.sb.commit_delivery(stage).unwrap();
    p.sender = sender;
    let (third, sender) = p.frame(3, 3, "third");
    p.sender = sender;
    let expected = |id| kami_protocol::ExpectedContext {
        message_id: MessageId::from_u128(id),
        conversation_id: ConversationId::from_u128(1),
        sender: p.aid,
        recipient: p.bid,
        receipt_target: None,
    };
    let (stage, receiver) = receiver
        .prepare_delivery(
            &third,
            &expected(3),
            MessageId::from_u128(89),
            &p.ta,
            &p.ta.pinned().unwrap().binding().peer_id(),
            1000,
        )
        .unwrap();
    p.sb.commit_delivery(stage).unwrap();
    let history = |now| {
        p.sb.ordered_history(ConversationId::from_u128(1), p.aid, now, 100, 10)
            .unwrap()
    };
    assert_eq!(history(1000).len(), 1);
    let visible = history(1100);
    assert_eq!(visible.len(), 2);
    assert_eq!(
        visible[1].gap_state,
        Some(kami_storage::GapState::Unresolved)
    );
    assert_eq!(
        history(61_000)[1].gap_state,
        Some(kami_storage::GapState::Permanent)
    );
    let (second, _) = p.frame(2, 2, "second");
    let (stage, _) = receiver
        .prepare_delivery(
            &second,
            &expected(2),
            MessageId::from_u128(90),
            &p.ta,
            &p.ta.pinned().unwrap().binding().peer_id(),
            1000,
        )
        .unwrap();
    p.sb.commit_delivery(stage).unwrap();
    let reconciled = history(61_001);
    assert_eq!(reconciled.len(), 3);
    assert_eq!(
        reconciled
            .iter()
            .map(|m| m.message.facts().message_id().as_u128())
            .collect::<Vec<_>>(),
        vec![1, 2, 3]
    );
    assert!(reconciled
        .iter()
        .all(|m| !m.gap_before && m.gap_state.is_none()));
    p.sb.delete_history(p.aid, MessageId::from_u128(1)).unwrap();
    let permit = kami_e2ee::ReplayPermit::new(
        &p.ta,
        &p.ta.pinned().unwrap().binding().peer_id(),
        p.aid,
        p.bid,
        ConversationId::from_u128(1),
    )
    .unwrap();
    assert_eq!(
        p.sb.recover_delivery(permit, first)
            .unwrap()
            .unwrap()
            .receipt(),
        first_ack.receipt()
    );
    assert!(history(61_002).iter().all(|m| !m.gap_before));
}
#[test]
fn process_kill_before_and_after_commit_preserves_ack_boundary() {
    let Some(_native) = common::native::isolated() else {
        return;
    };
    use std::process::{Command, Stdio};
    struct Child(std::process::Child);
    impl Drop for Child {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    for phase in ["before", "after"] {
        let control = common::directory();
        let mut child = Child(
            Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "failures::crash_child", "--nocapture"])
                .env("KAMI_RECEIVER_CRASH_CONTROL", control.path())
                .env("KAMI_RECEIVER_CRASH_PHASE", phase)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap(),
        );
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
        while !control.path().join("ready").exists() {
            assert!(
                child.0.try_wait().unwrap().is_none(),
                "child exited before readiness"
            );
            assert!(
                std::time::Instant::now() < deadline,
                "child readiness timeout"
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        child.0.kill().unwrap();
        child.0.wait().unwrap();
        let profile = std::path::PathBuf::from(
            std::fs::read_to_string(control.path().join("profile")).unwrap(),
        );
        let storage = Storage::open(
            kami_storage::ProfileLease::acquire(&profile).unwrap(),
            common::native::profile(&profile),
            std::sync::Arc::new(kami_identity::NativeCustody::open()),
            Default::default(),
        )
        .unwrap();
        let frame = std::fs::read(control.path().join("frame")).unwrap();
        let outer = kami_protocol::decode_frame(&frame).unwrap();
        let row = storage
            .load_contact(*outer.sender().as_bytes())
            .unwrap()
            .unwrap();
        let trust = ContactTrust::from_persisted(
            kami_identity::ContactTrustState::Verified,
            row.generation(),
            row.pinned(),
            row.candidate(),
        )
        .unwrap();
        let permit = kami_e2ee::ReplayPermit::new(
            &trust,
            &trust.pinned().unwrap().binding().peer_id(),
            outer.sender(),
            outer.recipient(),
            outer.conversation_id(),
        )
        .unwrap();
        let recovered = storage.recover_delivery(permit, frame).unwrap();
        if phase == "before" {
            assert!(recovered.is_none());
        } else {
            assert_eq!(
                recovered.unwrap().receipt(),
                std::fs::read(control.path().join("ack")).unwrap()
            );
        }
        for entry in std::fs::read_dir(&profile).unwrap() {
            let bytes = std::fs::read(entry.unwrap().path()).unwrap();
            assert!(!bytes
                .windows(b"CRASH-PRIVATE-PAYLOAD".len())
                .any(|w| w == b"CRASH-PRIVATE-PAYLOAD"));
        }
        storage.shutdown().unwrap();
        std::fs::remove_dir_all(profile).unwrap();
        std::fs::remove_dir_all(
            std::fs::read_to_string(control.path().join("sender_profile")).unwrap(),
        )
        .unwrap();
    }
}
#[test]
fn crash_child() {
    let Some(control) = std::env::var_os("KAMI_RECEIVER_CRASH_CONTROL") else {
        return;
    };
    let control = std::path::PathBuf::from(control);
    let p = Pair::new();
    let (frame, _) = p.frame(1, 1, "CRASH-PRIVATE-PAYLOAD");
    let stage = p.stage(&frame);
    std::fs::write(control.join("profile"), p.bd.path().to_str().unwrap()).unwrap();
    std::fs::write(
        control.join("sender_profile"),
        p._ad.path().to_str().unwrap(),
    )
    .unwrap();
    std::fs::write(control.join("frame"), &frame).unwrap();
    std::fs::write(control.join("ack"), stage.receipt_frame()).unwrap();
    if std::env::var("KAMI_RECEIVER_CRASH_PHASE").unwrap() == "after" {
        p.sb.commit_delivery(stage).unwrap();
    }
    std::fs::write(control.join("ready"), b"ready").unwrap();
    loop {
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
}
#[test]
fn receiver_byte_and_count_limits_roll_back_without_losing_prior_data() {
    let Some(_native) = common::native::isolated() else {
        return;
    };
    for config in [
        kami_storage::Config {
            outbox_bytes: 0,
            ..Default::default()
        },
        kami_storage::Config {
            outbox_messages: 0,
            ..Default::default()
        },
    ] {
        let p = Pair::with_config(config);
        let (frame, _) = p.frame(1, 1, "quota");
        assert_eq!(
            p.sb.commit_delivery(p.stage(&frame)).unwrap_err(),
            StorageError::Full
        );
        assert_eq!(p.counts(), [0; 6]);
    }
}
#[test]
fn corrupted_canonical_facts_never_authorize_exact_frame_reack() {
    let Some(_native) = common::native::isolated() else {
        return;
    };
    let p = Pair::new();
    let (frame, _) = p.frame(1, 1, "bound facts");
    p.sb.commit_delivery(p.stage(&frame)).unwrap();
    common::raw(&p.bd)
        .execute_batch("UPDATE message SET facts=CAST(substr(facts,1,104)||zeroblob(32) AS BLOB)")
        .unwrap();
    let permit = kami_e2ee::ReplayPermit::new(
        &p.ta,
        &p.ta.pinned().unwrap().binding().peer_id(),
        p.aid,
        p.bid,
        ConversationId::from_u128(1),
    )
    .unwrap();
    assert!(p.sb.recover_delivery(permit, frame).is_err());
}
#[test]
fn receipt_recovery_requires_coherent_durable_generation_and_account_links() {
    let Some(_native) = common::native::isolated() else {
        return;
    };
    for mutation in [
        "UPDATE delivery_artifact SET session_generation=1",
        "PRAGMA foreign_keys=OFF; DELETE FROM e2ee_account",
    ] {
        let p = Pair::new();
        let (frame, _) = p.frame(1, 1, "state links");
        p.sb.commit_delivery(p.stage(&frame)).unwrap();
        common::raw(&p.bd).execute_batch(mutation).unwrap();
        let permit = kami_e2ee::ReplayPermit::new(
            &p.ta,
            &p.ta.pinned().unwrap().binding().peer_id(),
            p.aid,
            p.bid,
            ConversationId::from_u128(1),
        )
        .unwrap();
        assert!(p.sb.recover_delivery(permit, frame).is_err(), "{mutation}");
    }
}
#[test]
fn legacy_inbox_rows_cannot_be_promoted_to_authenticated_delivery() {
    let Some(_native) = common::native::isolated() else {
        return;
    };
    let p = Pair::new();
    let (frame, _) = p.frame(1, 1, "legacy content");
    let outer = kami_protocol::decode_frame(&frame).unwrap();
    let facts = kami_core::MessageFacts::new(
        outer.message_id(),
        outer.conversation_id(),
        p.aid,
        p.bid,
        outer.sequence().unwrap(),
        kami_core::ContentBinding::from_bytes(outer.authentication_context().try_into().unwrap()),
    );
    p.sb.accept(
        kami_storage::StoredMessage::new(facts, b"legacy content".to_vec()).unwrap(),
        1000,
    )
    .unwrap();
    assert!(p.sb.commit_delivery(p.stage(&frame)).is_err());
    let permit = kami_e2ee::ReplayPermit::new(
        &p.ta,
        &p.ta.pinned().unwrap().binding().peer_id(),
        p.aid,
        p.bid,
        ConversationId::from_u128(1),
    )
    .unwrap();
    assert!(p.sb.recover_delivery(permit, frame).is_err());
    assert_eq!(p.counts(), [0, 0, 1, 0, 0, 0]);
}
#[test]
fn per_contact_and_conversation_caps_include_retained_history() {
    let Some(_native) = common::native::isolated() else {
        return;
    };
    for contact_limit in [false, true] {
        let p = Pair::new();
        let n = if contact_limit { 1024 } else { 512 };
        for i in 0..n {
            let conversation = if contact_limit { 2 + i / 400 } else { 1 };
            let sequence = if contact_limit { 1 + i % 400 } else { 1 + i };
            let facts = kami_core::MessageFacts::new(
                MessageId::from_u128(100 + i as u128),
                ConversationId::from_u128(conversation as u128),
                p.aid,
                p.bid,
                SenderSequence::new(sequence),
                kami_core::ContentBinding::from_bytes([7; 32]),
            );
            p.sb.accept(
                kami_storage::StoredMessage::new(facts, b"quota fixture".to_vec()).unwrap(),
                1000,
            )
            .unwrap();
        }
        let (frame, _) = p.frame(9000, if contact_limit { 1 } else { 513 }, "over capacity");
        assert_eq!(
            p.sb.commit_delivery(p.stage(&frame)).unwrap_err(),
            StorageError::Full
        );
        assert_eq!(p.counts(), [0, 0, n as i64, 0, 0, 0]);
    }
}
#[test]
fn concurrent_trust_change_is_atomic_with_delivery_commit() {
    let Some(_native) = common::native::isolated() else {
        return;
    };
    let p = Pair::new();
    let (frame, _) = p.frame(1, 1, "trust race");
    let stage = p.stage(&frame);
    let row = p.sb.load_contact(*p.aid.as_bytes()).unwrap().unwrap();
    let blocked = ContactTrustRow::new(
        *p.aid.as_bytes(),
        TrustState::Blocked,
        3,
        row.pinned().map(|p| p.to_vec()),
        row.candidate().to_vec(),
    )
    .unwrap();
    let barrier = std::sync::Barrier::new(2);
    let result = std::thread::scope(|scope| {
        let a = scope.spawn(|| {
            barrier.wait();
            p.sb.commit_delivery(stage)
        });
        let b = scope.spawn(|| {
            barrier.wait();
            p.sb.compare_and_swap_contact(Some(2), blocked)
        });
        let result = a.join().unwrap();
        b.join().unwrap().unwrap();
        result
    });
    match result {
        Ok(_) => assert_eq!(p.counts(), [1, 1, 1, 1, 1, 0]),
        Err(StorageError::Conflict) => assert_eq!(p.counts(), [0; 6]),
        Err(e) => panic!("{e:?}"),
    }
    let permit = kami_e2ee::ReplayPermit::new(
        &p.ta,
        &p.ta.pinned().unwrap().binding().peer_id(),
        p.aid,
        p.bid,
        ConversationId::from_u128(1),
    )
    .unwrap();
    assert!(p.sb.recover_delivery(permit, frame).is_err());
}
#[test]
fn full_gap_budget_still_admits_a_predecessor_that_resolves_a_gap() {
    let Some(_native) = common::native::isolated() else {
        return;
    };
    let p = Pair::new();
    for i in 0..128 {
        let facts = kami_core::MessageFacts::new(
            MessageId::from_u128(1000 + i as u128),
            ConversationId::from_u128(1),
            p.aid,
            p.bid,
            SenderSequence::new(2 + 2 * i),
            kami_core::ContentBinding::from_bytes([7; 32]),
        );
        p.sb.accept(
            kami_storage::StoredMessage::new(facts, b"gap quota fixture".to_vec()).unwrap(),
            1000,
        )
        .unwrap();
    }
    common::raw(&p.bd)
        .execute_batch("INSERT INTO receiver_gap SELECT sender,id,0 FROM message")
        .unwrap();
    let (extra, _) = p.frame(9000, 258, "another gap");
    assert_eq!(
        p.sb.commit_delivery(p.stage(&extra)).unwrap_err(),
        StorageError::Full
    );
    let (late, _) = p.frame(9001, 1, "missing predecessor");
    assert_eq!(
        p.sb.commit_delivery(p.stage(&late)).unwrap().acceptance(),
        Acceptance::New
    );
    assert_eq!(p.counts()[5], 127);
}
#[test]
fn excessive_gap_is_rejected_without_bootstrap_or_ack() {
    let Some(_native) = common::native::isolated() else {
        return;
    };
    let p = Pair::new();
    let (frame, _) = p.frame(1, 4097, "too far");
    assert_eq!(
        p.sb.commit_delivery(p.stage(&frame)).unwrap_err(),
        StorageError::Full
    );
    assert_eq!(p.counts(), [0; 6]);
}
