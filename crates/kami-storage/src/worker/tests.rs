use super::*;
use kami_key_provider::test_provider::TestKeyProvider;

fn wp3_fixture(incoming: bool) -> crate::session_state::Commit {
    use crate::session_state::*;
    let (sender, recipient) = if incoming {
        ([2; 32], [1; 32])
    } else {
        ([1; 32], [2; 32])
    };
    let facts = MessageFacts::new(
        MessageId::from_u128(1),
        ConversationId::from_u128(1),
        DeviceId::from_bytes(sender),
        DeviceId::from_bytes(recipient),
        kami_core::SenderSequence::new(1),
        kami_core::ContentBinding::from_bytes([9; 32]),
    );
    Commit {
        account: AccountRow::new(1, b"WP3-SECRET-ACCOUNT-FIXTURE".to_vec(), [1; 32]).unwrap(),
        next_account: incoming
            .then(|| AccountRow::new(2, b"WP3-SECRET-CONSUMED-FIXTURE".to_vec(), [2; 32]).unwrap()),
        previous: None,
        next: SessionRow::new(
            [4; 32],
            1,
            [2; 32],
            1,
            vec![3],
            vec![5],
            b"WP3-SECRET-SESSION-FIXTURE".to_vec(),
            [6; 32],
        )
        .unwrap(),
        message: crate::StoredMessage::new(facts, b"WP3-FRAME-FIXTURE".to_vec()).unwrap(),
        frame: b"WP3-FRAME-FIXTURE".to_vec(),
        received_at: incoming.then_some(1),
    }
}
fn wp3_profile(path: &std::path::Path, create: bool) -> Storage {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    let profile = ProfileKeyId::new("0123456789abcdef0123456789abcdef").unwrap();
    let provider = Arc::new(TestKeyProvider::seeded(profile.clone(), [42; 32]));
    let lease = ProfileLease::acquire(path).unwrap();
    if create {
        Storage::create(lease, profile, provider, Config::default())
    } else {
        Storage::open(lease, profile, provider, Config::default())
    }
    .unwrap()
}
fn wp3_seed(storage: &Storage) {
    storage
        .compare_and_swap_contact(
            None,
            crate::ContactTrustRow::new(
                [2; 32],
                crate::TrustState::Verified,
                1,
                Some(vec![3]),
                vec![3],
            )
            .unwrap(),
        )
        .unwrap();
    storage
        .call(|r| Request::E2eeAccount(None, wp3_fixture(false).account, r))
        .unwrap();
}

#[test]
fn wp3_worker_lost_reply_reopen_and_legacy_paths_fail_closed() {
    let exe = std::env::current_exe().unwrap();
    let dir = tempfile::tempdir_in(exe.parent().unwrap()).unwrap();
    let storage = wp3_profile(dir.path(), true);
    wp3_seed(&storage);
    let (reply, response) = mpsc::sync_channel(1);
    drop(response);
    storage
        .sender
        .as_ref()
        .unwrap()
        .try_send(Request::E2eeCommit(Box::new(wp3_fixture(false)), reply))
        .unwrap();
    // Barrier: the abandoned reply does not roll back the accepted commit.
    let state = storage.call(|r| Request::E2eeLoad([4; 32], r)).unwrap();
    assert!(state.1.is_some());
    storage.shutdown().unwrap();
    let storage = wp3_profile(dir.path(), false);
    let c = wp3_fixture(false);
    assert_eq!(
        storage
            .call(|r| Request::E2eeReconcile(c.next, c.message.facts(), c.frame, false, r))
            .unwrap(),
        Some(b"WP3-FRAME-FIXTURE".to_vec())
    );
    assert!(
        storage.pending(10).unwrap().is_empty(),
        "quarantined frames must not escape through legacy pending"
    );
    assert_eq!(
        storage.apply_authenticated_receipt(
            wp3_fixture(false).message.facts(),
            DeviceId::from_bytes([2; 32])
        ),
        Err(StorageError::Conflict)
    );
    storage.shutdown().unwrap();
}

#[test]
fn wp3_crash_child() {
    let Some(path) = std::env::var_os("KAMI_WP3_CHILD") else {
        return;
    };
    let storage = wp3_profile(std::path::Path::new(&path), true);
    wp3_seed(&storage);
    let incoming = std::env::var("KAMI_WP3_INCOMING").unwrap() == "true";
    storage
        .call(|r| Request::E2eeCommit(Box::new(wp3_fixture(incoming)), r))
        .unwrap();
    panic!("missing crash checkpoint");
}
#[test]
fn wp3_process_kill_atomicity_and_encrypted_artifacts() {
    struct Child(std::process::Child);
    impl Drop for Child {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    let exe = std::env::current_exe().unwrap();
    for incoming in [false, true] {
        for phase in [
            "before",
            "account",
            "session",
            "message",
            "frame",
            "committed",
        ] {
            let dir = tempfile::tempdir_in(exe.parent().unwrap()).unwrap();
            let mut child = Child(
                std::process::Command::new(&exe)
                    .args(["--exact", "worker::tests::wp3_crash_child", "--nocapture"])
                    .env("KAMI_WP3_CHILD", dir.path())
                    .env("KAMI_WP3_INCOMING", incoming.to_string())
                    .env("KAMI_WP3_PHASE", phase)
                    .spawn()
                    .unwrap(),
            );
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
            while !dir.path().join("ready").exists() {
                assert!(
                    child.0.try_wait().unwrap().is_none(),
                    "child exited before checkpoint {phase}"
                );
                assert!(std::time::Instant::now() < deadline, "checkpoint timeout");
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            for entry in std::fs::read_dir(dir.path()).unwrap() {
                let bytes = std::fs::read(entry.unwrap().path()).unwrap();
                for secret in [
                    b"WP3-SECRET-ACCOUNT-FIXTURE".as_slice(),
                    b"WP3-SECRET-CONSUMED-FIXTURE",
                    b"WP3-SECRET-SESSION-FIXTURE",
                    b"WP3-FRAME-FIXTURE",
                ] {
                    assert!(!bytes.windows(secret.len()).any(|w| w == secret));
                }
            }
            child.0.kill().unwrap();
            child.0.wait().unwrap();
            let storage = wp3_profile(dir.path(), false);
            let (account, session) = storage.call(|r| Request::E2eeLoad([4; 32], r)).unwrap();
            let committed = phase == "committed";
            assert_eq!(
                account.unwrap().generation,
                if committed && incoming { 2 } else { 1 }
            );
            assert_eq!(session.is_some(), committed);
            let c = wp3_fixture(incoming);
            assert_eq!(
                storage
                    .history(c.message.facts().conversation_id(), 10)
                    .unwrap()
                    .len(),
                usize::from(committed)
            );
            if committed {
                assert_eq!(
                    storage
                        .call(|r| Request::E2eeReconcile(
                            c.next,
                            c.message.facts(),
                            c.frame,
                            incoming,
                            r
                        ))
                        .unwrap(),
                    Some(b"WP3-FRAME-FIXTURE".to_vec())
                );
            }
            storage.shutdown().unwrap();
        }
    }
}

#[test]
fn wp3_migrates_each_version_and_preserves_existing_rows() {
    for version in 0..=3 {
        let mut db = rusqlite::Connection::open_in_memory().unwrap();
        if version > 0 {
            db.execute_batch(include_str!("../../migrations/001_initial.sql"))
                .unwrap();
            db.execute("INSERT INTO profile VALUES(1,'fixture')", [])
                .unwrap();
        }
        if version >= 2 {
            db.execute_batch(include_str!("../../migrations/002_contact_trust.sql"))
                .unwrap();
        }
        if version >= 3 {
            db.execute_batch(include_str!("../../migrations/003_e2ee_state.sql"))
                .unwrap();
        }
        db.pragma_update(None, "user_version", version).unwrap();
        crate::migrations::migrate(&mut db, version == 0).unwrap();
        assert_eq!(
            db.pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))
                .unwrap(),
            crate::migrations::VERSION
        );
        assert_eq!(
            db.query_row(
                "SELECT count(*) FROM sqlite_schema WHERE name='e2ee_account'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            1
        );
        crate::migrations::migrate(&mut db, false).unwrap();
        if version > 0 {
            assert_eq!(
                db.query_row("SELECT namespace FROM profile", [], |r| r
                    .get::<_, String>(0))
                    .unwrap(),
                "fixture"
            );
        }
    }
}
#[test]
fn wp3_atomic_state_message_replay_and_retry() {
    use crate::session_state::*;
    let mut db = rusqlite::Connection::open_in_memory().unwrap();
    db.pragma_update(None, "foreign_keys", true).unwrap();
    crate::migrations::migrate(&mut db, true).unwrap();
    let trust = crate::ContactTrustRow::new(
        [2; 32],
        crate::TrustState::Verified,
        1,
        Some(vec![3]),
        vec![3],
    )
    .unwrap();
    crate::contact_trust::cas(&mut db, None, trust).unwrap();
    let account = AccountRow::new(1, vec![1], [1; 32]).unwrap();
    commit_account(&mut db, None, account.clone()).unwrap();
    let session =
        SessionRow::new([4; 32], 1, [2; 32], 1, vec![3], vec![5], vec![6], [6; 32]).unwrap();
    let facts = kami_core::MessageFacts::new(
        MessageId::from_u128(1),
        ConversationId::from_u128(1),
        DeviceId::from_bytes([1; 32]),
        DeviceId::from_bytes([2; 32]),
        kami_core::SenderSequence::new(1),
        kami_core::ContentBinding::from_bytes([9; 32]),
    );
    let command = || Commit {
        account: account.clone(),
        next_account: None,
        previous: None,
        next: session.clone(),
        message: crate::StoredMessage::new(facts, vec![7]).unwrap(),
        frame: vec![7],
        received_at: None,
    };
    let mut mismatched = command();
    mismatched.message = crate::StoredMessage::new(facts, vec![99]).unwrap();
    assert_eq!(
        commit(&mut db, mismatched, 1024, 1),
        Err(StorageError::InvalidInput)
    );
    assert_eq!(commit(&mut db, command(), 0, 1), Err(StorageError::Full));
    assert!(load_session(&db, [4; 32]).unwrap().is_none());
    assert_eq!(
        commit(&mut db, command(), 1024, 1),
        Ok(crate::Acceptance::New)
    );
    let mut bad_payload = command();
    bad_payload.next.generation = 2;
    bad_payload.previous = Some(session.clone());
    bad_payload.message = crate::StoredMessage::new(facts, vec![99]).unwrap();
    assert_eq!(
        commit(&mut db, bad_payload, 1024, 1),
        Err(StorageError::InvalidInput)
    );
    assert_eq!(
        reconcile(&db, &session, facts, &[7], false).unwrap(),
        Some(vec![7])
    );
    assert_eq!(
        reconcile(&db, &session, facts, &[99], false),
        Err(StorageError::Conflict)
    );
    assert_eq!(
        commit(&mut db, command(), 1024, 1),
        Err(StorageError::Conflict)
    );
    assert_eq!(load_session(&db, [4; 32]).unwrap(), Some(session.clone()));
    let mut fork = account.clone();
    fork.state[0] = 99;
    assert_eq!(
        commit_account(
            &mut db,
            Some(fork),
            AccountRow::new(2, vec![2], [2; 32]).unwrap()
        ),
        Err(StorageError::Conflict)
    );
    let blocked = crate::ContactTrustRow::new(
        [2; 32],
        crate::TrustState::Blocked,
        2,
        Some(vec![3]),
        vec![3],
    )
    .unwrap();
    crate::contact_trust::cas(&mut db, Some(1), blocked).unwrap();
    assert_eq!(
        reconcile(&db, &session, facts, &[7], false),
        Err(StorageError::Conflict)
    );
}

#[test]
fn wp3_receive_commit_failure_never_grants_ack() {
    use crate::session_state::*;
    let mut db = rusqlite::Connection::open_in_memory().unwrap();
    db.pragma_update(None, "foreign_keys", true).unwrap();
    crate::migrations::migrate(&mut db, true).unwrap();
    crate::contact_trust::cas(
        &mut db,
        None,
        crate::ContactTrustRow::new(
            [2; 32],
            crate::TrustState::Verified,
            1,
            Some(vec![3]),
            vec![3],
        )
        .unwrap(),
    )
    .unwrap();
    let account = AccountRow::new(1, vec![1], [1; 32]).unwrap();
    commit_account(&mut db, None, account.clone()).unwrap();
    let session =
        SessionRow::new([4; 32], 1, [2; 32], 1, vec![3], vec![5], vec![6], [6; 32]).unwrap();
    let facts = MessageFacts::new(
        MessageId::from_u128(1),
        ConversationId::from_u128(1),
        DeviceId::from_bytes([2; 32]),
        DeviceId::from_bytes([1; 32]),
        kami_core::SenderSequence::new(1),
        kami_core::ContentBinding::from_bytes([9; 32]),
    );
    let command = || Commit {
        account: account.clone(),
        next_account: Some(AccountRow::new(2, vec![2], [2; 32]).unwrap()),
        previous: None,
        next: session.clone(),
        message: crate::StoredMessage::new(facts, vec![8]).unwrap(),
        frame: vec![7],
        received_at: Some(1),
    };
    db.execute_batch("CREATE TRIGGER fail_frame BEFORE INSERT ON e2ee_frame BEGIN SELECT RAISE(ABORT,'fault'); END;").unwrap();
    assert_eq!(
        commit(&mut db, command(), 1024, 1),
        Err(StorageError::Conflict)
    );
    assert_eq!(load_account(&db).unwrap(), Some(account.clone()));
    assert!(load_session(&db, [4; 32]).unwrap().is_none());
    assert!(
        crate::inbox::receipt(&db, facts.sender(), facts.message_id())
            .unwrap()
            .is_none()
    );
    db.execute_batch("DROP TRIGGER fail_frame").unwrap();
    assert_eq!(
        commit(&mut db, command(), 1024, 1),
        Ok(crate::Acceptance::New)
    );
    assert_eq!(load_account(&db).unwrap().unwrap().generation, 2);
    assert_eq!(
        reconcile(&db, &session, facts, &[7], true).unwrap(),
        Some(vec![7])
    );
    crate::inbox::delete_history(&mut db, facts.sender(), facts.message_id()).unwrap();
    assert_eq!(
        reconcile(&db, &session, facts, &[7], true).unwrap(),
        Some(vec![7])
    );
    assert_eq!(
        reconcile(&db, &session, facts, &[8], true),
        Err(StorageError::Conflict)
    );
}

#[test]
fn wp3_v3_failure_rolls_back_entire_upgrade() {
    for version in [1, 2] {
        let mut db = rusqlite::Connection::open_in_memory().unwrap();
        db.execute_batch(include_str!("../../migrations/001_initial.sql"))
            .unwrap();
        if version == 2 {
            db.execute_batch(include_str!("../../migrations/002_contact_trust.sql"))
                .unwrap();
        }
        db.execute_batch(
            "CREATE TABLE e2ee_session(conflict INTEGER); INSERT INTO profile VALUES(1,'fixture');",
        )
        .unwrap();
        db.pragma_update(None, "user_version", version).unwrap();
        assert_eq!(
            crate::migrations::migrate(&mut db, false),
            Err(StorageError::Migration)
        );
        assert_eq!(
            db.pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))
                .unwrap(),
            version
        );
        assert_eq!(
            db.query_row(
                "SELECT count(*) FROM sqlite_schema WHERE name='e2ee_account'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            0
        );
        assert_eq!(
            db.query_row(
                "SELECT count(*) FROM sqlite_schema WHERE name='contact_trust'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            i64::from(version == 2)
        );
        assert_eq!(
            db.query_row("SELECT namespace FROM profile", [], |r| r
                .get::<_, String>(0))
                .unwrap(),
            "fixture"
        );
    }
}

#[test]
fn wp3_bounds_corruption_and_same_generation_digest_forks_fail_closed() {
    use crate::session_state::*;
    for generation in [0, u64::MAX] {
        assert!(AccountRow::new(generation, vec![1], [1; 32]).is_err());
    }
    for bytes in [vec![], vec![0; 1_048_577]] {
        assert!(AccountRow::new(1, bytes, [1; 32]).is_err());
    }
    let mut db = rusqlite::Connection::open_in_memory().unwrap();
    db.pragma_update(None, "foreign_keys", true).unwrap();
    crate::migrations::migrate(&mut db, true).unwrap();
    crate::contact_trust::cas(
        &mut db,
        None,
        crate::ContactTrustRow::new(
            [2; 32],
            crate::TrustState::Verified,
            1,
            Some(vec![3]),
            vec![3],
        )
        .unwrap(),
    )
    .unwrap();
    let c = wp3_fixture(false);
    commit_account(&mut db, None, c.account.clone()).unwrap();
    commit(&mut db, c, 1024, 10).unwrap();
    let c = wp3_fixture(false);
    let mut fork = c.next.clone();
    fork.digest[0] ^= 1;
    assert_eq!(
        reconcile(&db, &fork, c.message.facts(), &c.frame, false),
        Err(StorageError::Conflict)
    );
    // Digest-only equality is insufficient: exact old bytes also participate in CAS.
    let mut wrong = c.next.clone();
    wrong.state[0] ^= 1;
    assert_eq!(
        reconcile(&db, &wrong, c.message.facts(), &c.frame, false),
        Err(StorageError::Conflict)
    );
    for update in [
        "UPDATE e2ee_session SET state=zeroblob(1048577)",
        "UPDATE e2ee_session SET version=2",
        "UPDATE e2ee_session SET generation=0",
        "UPDATE e2ee_session SET pin=zeroblob(513)",
        "UPDATE e2ee_session SET context=zeroblob(2049)",
    ] {
        db.execute_batch("SAVEPOINT corruption; PRAGMA ignore_check_constraints=ON;")
            .unwrap();
        db.execute_batch(update).unwrap();
        assert_eq!(
            load_session(&db, [4; 32]),
            Err(StorageError::WrongKeyOrCorrupt)
        );
        db.execute_batch("ROLLBACK TO corruption; RELEASE corruption;")
            .unwrap();
    }
    db.execute_batch("UPDATE e2ee_account SET state=zeroblob(1048577)")
        .unwrap();
    assert_eq!(load_account(&db), Err(StorageError::WrongKeyOrCorrupt));
}

#[test]
fn wp3_stale_concurrent_candidates_and_old_frame_retry_after_advancement() {
    let exe = std::env::current_exe().unwrap();
    let dir = tempfile::tempdir_in(exe.parent().unwrap()).unwrap();
    let storage = wp3_profile(dir.path(), true);
    wp3_seed(&storage);
    let barrier = std::sync::Barrier::new(2);
    std::thread::scope(|scope| {
        let a = scope.spawn(|| {
            barrier.wait();
            storage.call(|r| Request::E2eeCommit(Box::new(wp3_fixture(false)), r))
        });
        let b = scope.spawn(|| {
            barrier.wait();
            storage.call(|r| Request::E2eeCommit(Box::new(wp3_fixture(false)), r))
        });
        let results = [a.join().unwrap(), b.join().unwrap()];
        assert_eq!(
            results
                .iter()
                .filter(|r| **r == Ok(crate::Acceptance::New))
                .count(),
            1
        );
        assert_eq!(
            results
                .iter()
                .filter(|r| **r == Err(StorageError::Conflict))
                .count(),
            1
        );
    });
    let old = wp3_fixture(false);
    let mut next = wp3_fixture(false);
    next.previous = Some(next.next.clone());
    next.next.generation = 2;
    next.next.state.push(99);
    next.next.digest = [99; 32];
    let f = next.message.facts();
    next.message = crate::StoredMessage::new(
        MessageFacts::new(
            MessageId::from_u128(2),
            f.conversation_id(),
            f.sender(),
            f.recipient(),
            kami_core::SenderSequence::new(2),
            f.content_binding(),
        ),
        vec![8],
    )
    .unwrap();
    next.frame = vec![8];
    storage
        .call(|r| Request::E2eeCommit(Box::new(next), r))
        .unwrap();
    assert_eq!(
        storage
            .call(|r| Request::E2eeReconcile(old.next, old.message.facts(), old.frame, false, r))
            .unwrap(),
        Some(b"WP3-FRAME-FIXTURE".to_vec())
    );
    storage.shutdown().unwrap();
}

#[test]
fn wp3_does_not_promote_legacy_outbox_to_ratchet_evidence() {
    let exe = std::env::current_exe().unwrap();
    let dir = tempfile::tempdir_in(exe.parent().unwrap()).unwrap();
    let storage = wp3_profile(dir.path(), true);
    wp3_seed(&storage);
    storage.enqueue(wp3_fixture(false).message).unwrap();
    assert_eq!(
        storage.call(|r| Request::E2eeCommit(Box::new(wp3_fixture(false)), r)),
        Err(StorageError::Conflict)
    );
    let (_, session) = storage.call(|r| Request::E2eeLoad([4; 32], r)).unwrap();
    assert!(session.is_none());
    assert_eq!(storage.pending(10).unwrap().len(), 1);
    storage.shutdown().unwrap();
}

#[test]
fn bounded_queue_rejects_without_dropping_accepted_requests() {
    let exe = std::env::current_exe().unwrap();
    let dir = tempfile::tempdir_in(exe.parent().unwrap()).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    let profile = ProfileKeyId::new("0123456789abcdef0123456789abcdef").unwrap();
    let provider = Arc::new(TestKeyProvider::seeded(profile.clone(), [42; 32]));
    let storage = Storage::create(
        ProfileLease::acquire(dir.path()).unwrap(),
        profile,
        provider,
        Config {
            queue_capacity: 1,
            ..Config::default()
        },
    )
    .unwrap();
    let (entered, wait) = mpsc::sync_channel(1);
    let (release, hold) = mpsc::sync_channel(1);
    storage
        .sender
        .as_ref()
        .unwrap()
        .try_send(Request::Pause(entered, hold))
        .unwrap();
    wait.recv().unwrap();
    let (reply, response) = mpsc::sync_channel(1);
    storage
        .sender
        .as_ref()
        .unwrap()
        .try_send(Request::Cipher(reply))
        .unwrap();
    assert_eq!(storage.cipher_version(), Err(StorageError::QueueFull));
    release.send(()).unwrap();
    assert!(response.recv().unwrap().unwrap().starts_with("4."));
    storage.shutdown().unwrap();
}
