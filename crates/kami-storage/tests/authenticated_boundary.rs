//! External-crate boundary probes without adding test dependencies.
use std::{path::PathBuf, process::Command};

fn library(name: &str) -> PathBuf {
    // Cargo chooses one coherent normal-dependency graph, rather than whichever
    // stale rlib happens to have the newest timestamp in a dirty target directory.
    static ARTIFACTS: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    let artifacts = ARTIFACTS.get_or_init(|| {
        let output = Command::new("cargo")
            .args([
                "build",
                "--locked",
                "--offline",
                "-p",
                "kami-storage",
                "--message-format=json",
            ])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap()
    });
    for line in artifacts.lines() {
        if let Some((_, files)) = line.split_once("\"filenames\":[") {
            if let Some(path) = files.split('"').nth(1) {
                if path.ends_with(".rlib") && {
                    let file = PathBuf::from(path);
                    let file = file.file_name().unwrap().to_string_lossy();
                    file.starts_with(&format!("lib{name}-")) || file == format!("lib{name}.rlib")
                } {
                    return PathBuf::from(path);
                }
            }
        }
    }
    // tempfile is a test-only helper, not part of the cross-crate typed graph.
    assert_eq!(name, "tempfile");
    let deps = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
    std::fs::read_dir(deps)
        .unwrap()
        .map(|e| e.unwrap().path())
        .find(|p| {
            p.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("libtempfile-")
                && p.extension().is_some_and(|e| e == "rlib")
        })
        .unwrap()
}
fn probe(source: &str, run: bool) -> std::process::Output {
    let dir = tempfile::tempdir_in(std::env::temp_dir()).unwrap();
    let input = dir.path().join("probe.rs");
    let source = format!(
        "#[path={:?}] mod common;\n{}",
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/common/mod.rs"),
        source
    );
    std::fs::write(&input, source).unwrap();
    let output = dir.path().join("probe");
    let mut command = Command::new("rustc");
    command
        .args(["--edition=2021", "--test"])
        .arg(&input)
        .arg("-o")
        .arg(&output)
        .arg("-L")
        .arg(format!(
            "dependency={}",
            std::env::current_exe().unwrap().parent().unwrap().display()
        ));
    for name in [
        "kami_storage",
        "kami_e2ee",
        "kami_identity",
        "kami_core",
        "kami_key_provider",
        "vodozemac",
        "libp2p_identity",
        "kami_protocol",
        "tempfile",
        "rusqlite",
        "zeroize",
    ] {
        command
            .arg("--extern")
            .arg(format!("{name}={}", library(name).display()));
    }
    let compiled = command.output().unwrap();
    if !compiled.status.success() || !run {
        return compiled;
    }
    Command::new(output)
        .args(["--nocapture", "--test-threads=1"])
        .output()
        .unwrap()
}

#[test]
fn authenticated_positive_boundary() {
    let result = probe(POSITIVE, true);
    assert!(
        result.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    println!("{}", String::from_utf8_lossy(&result.stdout));
}

#[test]
fn neutral_candidates_cannot_cross_storage_boundary() {
    let result = probe("fn bad(s: &kami_storage::Storage, m: kami_storage::StoredMessage) { let _ = s.commit_authenticated(m); }", false);
    assert!(!result.status.success());
    assert!(
        String::from_utf8_lossy(&result.stderr).contains("E0308"),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}

const POSITIVE: &str = r#"
use kami_e2ee::{AuthenticatedCommit, QuarantinedSession, Endpoint};
use kami_core::{DeviceId, ConversationId, MessageId, SenderSequence};
use kami_identity::{ContactTrust, Invitation, OlmBindingData, OlmBindingProof, OlmPrekeyKind, PeerBindingProof, TrustTransition, VerificationEvidence};
use libp2p_identity::Keypair;
use vodozemac::olm::Account;
use kami_storage::{Storage, StorageError, ContactTrustRow, TrustState};

// Controlled identities/selected bundles, NOT live network authentication.
fn endpoint(account: &Account, seed: u8, fallback: bool) -> (Endpoint, ContactTrust) {
    let device = Keypair::ed25519_from_bytes([seed; 32]).unwrap();
    let public = device.public().try_into_ed25519().unwrap().to_bytes();
    let transport = Keypair::ed25519_from_bytes([seed + 10; 32]).unwrap().public();
    let peer = transport.to_peer_id();
    let mut fields = Vec::new();
    for bytes in [transport.encode_protobuf(), peer.to_bytes()] {
        fields.extend((bytes.len() as u16).to_be_bytes()); fields.extend(bytes);
    }
    let mut transcript = b"kami-peer-binding-v1\0".to_vec(); transcript.extend(&fields);
    let mut wire = vec![1]; wire.extend(fields); wire.extend(device.sign(&transcript).unwrap());
    let peer_binding = PeerBindingProof::parse_bounded(&wire).unwrap();
    let pending = ContactTrust::pending(Invitation::new(public, peer_binding.clone()).unwrap());
    let evidence = VerificationEvidence::independent_comparison(&pending, pending.candidate().fingerprint(), true).unwrap();
    let trust = pending.transition(TrustTransition::Verify(evidence)).unwrap();
    let prekey = if fallback { *account.fallback_key().values().next().unwrap() } else { *account.one_time_keys().values().next().unwrap() };
    let selected = OlmBindingData {
        publisher_peer: peer, olm_ed25519: *account.ed25519_key().as_bytes(),
        olm_curve25519: *account.curve25519_key().as_bytes(),
        prekey_kind: if fallback { OlmPrekeyKind::Fallback } else { OlmPrekeyKind::OneTime },
        prekey_reference: vec![seed], prekey_curve25519: *prekey.as_bytes(),
        binding_generation: 1, bundle_generation: 1, not_before_ms: 1, expires_at_ms: 2000,
    };
    let mut wire = b"kami-olm-binding-v1\0".to_vec(); wire.push(1); wire.extend(public);
    let peer = selected.publisher_peer.to_bytes(); wire.extend((peer.len() as u16).to_be_bytes()); wire.extend(peer);
    wire.extend(selected.olm_ed25519); wire.extend(selected.olm_curve25519); wire.push(selected.prekey_kind as u8);
    wire.extend((selected.prekey_reference.len() as u16).to_be_bytes()); wire.extend(&selected.prekey_reference); wire.extend(selected.prekey_curve25519);
    for n in [selected.binding_generation, selected.bundle_generation, selected.not_before_ms, selected.expires_at_ms] { wire.extend(n.to_be_bytes()); }
    wire.extend(device.sign(&wire).unwrap());
    (Endpoint::new(DeviceId::from_bytes(public), peer_binding, OlmBindingProof::parse_bounded(&wire).unwrap(), selected), trust)
}
fn seed(s: &Storage, t: &ContactTrust, device: DeviceId) {
    assert_eq!(t.generation(), 2);
    s.compare_and_swap_contact(None, ContactTrustRow::new(*device.as_bytes(), TrustState::Pending, 1, None, t.candidate().to_bytes()).unwrap()).unwrap();
    s.compare_and_swap_contact(Some(1), ContactTrustRow::new(*device.as_bytes(), TrustState::Verified, t.generation(), Some(t.pinned().unwrap().to_bytes()), t.candidate().to_bytes()).unwrap()).unwrap();
}
fn counts(dir: &tempfile::TempDir) -> [i64; 4] {
    let db = common::raw(dir);
    ["e2ee_account", "e2ee_session", "message", "e2ee_frame"].map(|t| db.query_row(&format!("SELECT count(*) FROM {t}"), [], |r| r.get(0)).unwrap())
}
fn prepared(s: &QuarantinedSession, id: u128, seq: u64, t: &ContactTrust) -> (AuthenticatedCommit, QuarantinedSession) {
    s.prepare_text(MessageId::from_u128(id), SenderSequence::new(seq), 1000, "WP2B-PRIVATE-TEXT", t, &t.pinned().unwrap().binding().peer_id(), 1000).unwrap()
}
#[test]
fn outbound_inbound_atomic_retry_cas_prekey_and_receipt_boundaries() {
  for fallback in [false, true] {
    let mut alice = Account::new(); alice.generate_one_time_keys(1); alice.generate_fallback_key();
    let mut bob = Account::new(); bob.generate_one_time_keys(1); bob.generate_fallback_key();
    let (a, ta) = endpoint(&alice, 7, fallback); let (b, tb) = endpoint(&bob, 8, fallback);
    let aid = DeviceId::from_bytes(Keypair::ed25519_from_bytes([7;32]).unwrap().public().try_into_ed25519().unwrap().to_bytes());
    let bid = DeviceId::from_bytes(Keypair::ed25519_from_bytes([8;32]).unwrap().public().try_into_ed25519().unwrap().to_bytes());
    let ad = common::directory(); let bd = common::directory(); let sa = common::create(&ad); let sb = common::create(&bd);
    seed(&sa, &tb, bid); seed(&sb, &ta, aid);
    let source = QuarantinedSession::outbound(alice, a.clone(), b.clone(), ConversationId::from_u128(1), &tb, &tb.pinned().unwrap().binding().peer_id(), 1000).unwrap();
    let (c, advanced) = prepared(&source, 1, 1, &tb); let facts = c.facts();
    let retry = advanced.prepare_retry(&tb, &tb.pinned().unwrap().binding().peer_id(), 1000).unwrap();
    let blocked = tb.transition(TrustTransition::Block).unwrap();
    assert!(advanced.prepare_retry(&blocked, &tb.pinned().unwrap().binding().peer_id(), 1000).is_err());
    assert!(advanced.prepare_retry(&tb, &ta.pinned().unwrap().binding().peer_id(), 1000).is_err());
    assert!(advanced.prepare_retry(&tb, &tb.pinned().unwrap().binding().peer_id(), 2000).is_err());
    let (fork, _) = prepared(&source, 2, 1, &tb);
    let frame = sa.commit_authenticated(c).unwrap();
    assert_eq!(counts(&ad), [1,1,1,1]);
    assert_eq!(sa.commit_authenticated(retry).unwrap(), frame);
    let conflict = source.prepare_text(MessageId::from_u128(1), SenderSequence::new(1), 1000, "CONFLICTING-CONTENT", &tb, &tb.pinned().unwrap().binding().peer_id(), 1000).unwrap().0;
    assert_eq!(sa.commit_authenticated(conflict), Err(StorageError::Conflict));
    assert_eq!(sa.commit_authenticated(fork), Err(StorageError::Conflict));
    let expected = kami_protocol::ExpectedContext { message_id: facts.message_id(), conversation_id: facts.conversation_id(), sender: aid, recipient: bid, receipt_target: None };
    // Two first-inbound candidates from the same prekey/account source.
    let copy = Account::from_pickle(bob.pickle());
    let (inbound, received) = QuarantinedSession::inbound(bob, b.clone(), a.clone(), ConversationId::from_u128(1), &frame, &expected, &ta, &ta.pinned().unwrap().binding().peer_id(), 1000).unwrap();
    let (duplicate, _) = QuarantinedSession::inbound(copy, b, a, ConversationId::from_u128(1), &frame, &expected, &ta, &ta.pinned().unwrap().binding().peer_id(), 1000).unwrap();
    assert_eq!(sb.commit_authenticated(inbound).unwrap(), frame);
    assert_eq!(sb.commit_authenticated(duplicate).unwrap(), frame);
    assert_eq!(counts(&bd), [1,1,1,1]);
    assert_eq!(common::raw(&bd).query_row("SELECT generation FROM e2ee_account", [], |r| r.get::<_,i64>(0)).unwrap(), if fallback {1} else {2});
    assert_eq!(sb.history(ConversationId::from_u128(1), 10).unwrap()[0].payload(), b"WP2B-PRIVATE-TEXT");
    assert_eq!(sb.receipt(aid, facts.message_id()), Err(StorageError::Conflict));
    assert_eq!(sa.apply_authenticated_receipt(facts, bid), Err(StorageError::Conflict));
    assert!(sa.pending(10).unwrap().is_empty());
    let (second, _) = prepared(&advanced, 2, 2, &tb); let second_facts = second.facts();
    let frame2 = sa.commit_authenticated(second).unwrap();
    let expected2 = kami_protocol::ExpectedContext { message_id: second_facts.message_id(), ..expected };
    let (second_in, _) = received.prepare_receive(&frame2, &expected2, &ta, &ta.pinned().unwrap().binding().peer_id(), 1000).unwrap();
    sb.commit_authenticated(second_in).unwrap();
    assert!(received.prepare_receive(&frame, &expected, &ta, &ta.pinned().unwrap().binding().peer_id(), 1000).is_err());
    // Reconcile retained exact ciphertext after later session progress and reopen.
    let old_retry = advanced.prepare_retry(&tb, &tb.pinned().unwrap().binding().peer_id(), 1000).unwrap();
    sa.shutdown().unwrap(); let sa = common::open(&ad);
    assert_eq!(sa.commit_authenticated(old_retry).unwrap(), frame);
    assert_eq!(counts(&ad), [1,1,2,2]); assert_eq!(counts(&bd), [1,1,2,2]);
    sa.shutdown().unwrap(); sb.shutdown().unwrap();
  }
}
#[test]
fn stale_trust_and_quota_failure_leave_no_bootstrap_or_ack() {
    let mut alice = Account::new(); alice.generate_one_time_keys(1);
    let mut bob = Account::new(); bob.generate_one_time_keys(1);
    let (a, _) = endpoint(&alice, 7, false); let (b, tb) = endpoint(&bob, 8, false);
    let bid = DeviceId::from_bytes(Keypair::ed25519_from_bytes([8;32]).unwrap().public().try_into_ed25519().unwrap().to_bytes());
    let source = QuarantinedSession::outbound(alice, a, b, ConversationId::from_u128(1), &tb, &tb.pinned().unwrap().binding().peer_id(), 1000).unwrap();
    let dir = common::directory(); let s = common::create(&dir); seed(&s, &tb, bid);
    let (c, _) = prepared(&source, 1, 1, &tb);
    s.compare_and_swap_contact(Some(tb.generation()), ContactTrustRow::new(*bid.as_bytes(), TrustState::Blocked, tb.generation()+1, Some(tb.pinned().unwrap().to_bytes()), tb.candidate().to_bytes()).unwrap()).unwrap();
    assert_eq!(s.commit_authenticated(c), Err(StorageError::Conflict)); assert_eq!(counts(&dir), [0;4]); s.shutdown().unwrap();
    let dir = common::directory();
    let s = Storage::create(kami_storage::ProfileLease::acquire(dir.path()).unwrap(), common::profile(), std::sync::Arc::new(common::Provider(Ok(42))), kami_storage::Config { outbox_bytes: 0, ..Default::default() }).unwrap(); seed(&s, &tb, bid);
    let (c, _) = prepared(&source, 1, 1, &tb); assert!(s.commit_authenticated(c).is_err()); assert_eq!(counts(&dir), [0;4]); s.shutdown().unwrap();
}
#[test]
fn authenticated_fault_phases_roll_back_first_inbound_and_outbound() {
  for incoming in [false, true] {
    for table in ["e2ee_account", "e2ee_session", "message", "e2ee_frame"] {
      let mut alice = Account::new(); alice.generate_one_time_keys(1);
      let mut bob = Account::new(); bob.generate_one_time_keys(1);
      let (a, ta) = endpoint(&alice, 7, false); let (b, tb) = endpoint(&bob, 8, false);
      let aid = DeviceId::from_bytes(Keypair::ed25519_from_bytes([7;32]).unwrap().public().try_into_ed25519().unwrap().to_bytes());
      let bid = DeviceId::from_bytes(Keypair::ed25519_from_bytes([8;32]).unwrap().public().try_into_ed25519().unwrap().to_bytes());
      let source = QuarantinedSession::outbound(alice, a.clone(), b.clone(), ConversationId::from_u128(1), &tb, &tb.pinned().unwrap().binding().peer_id(), 1000).unwrap();
      let (outbound, _) = prepared(&source, 1, 1, &tb); let facts = outbound.facts();
      let candidate = if incoming {
        let material = outbound.into_material(); let frame = material.frame().to_vec();
        let expected = kami_protocol::ExpectedContext { message_id: facts.message_id(), conversation_id: facts.conversation_id(), sender: aid, recipient: bid, receipt_target: None };
        QuarantinedSession::inbound(bob, b, a, ConversationId::from_u128(1), &frame, &expected, &ta, &ta.pinned().unwrap().binding().peer_id(), 1000).unwrap().0
      } else { outbound };
      let dir = common::directory(); let s = common::create(&dir);
      seed(&s, if incoming {&ta} else {&tb}, if incoming {aid} else {bid});
      common::raw(&dir).execute_batch(&format!("CREATE TRIGGER injected BEFORE INSERT ON {table} BEGIN SELECT RAISE(ABORT, 'injected'); END;")).unwrap();
      assert!(s.commit_authenticated(candidate).is_err(), "phase {table}");
      assert_eq!(counts(&dir), [0;4], "phase {table}");
      assert_eq!(s.receipt(aid, facts.message_id()).unwrap(), None);
      s.shutdown().unwrap();
      // SQLCipher and sidecars must not contain staged plaintext/snapshot markers.
      for entry in std::fs::read_dir(dir.path()).unwrap() {
        let bytes = std::fs::read(entry.unwrap().path()).unwrap();
        for secret in [b"WP2B-PRIVATE-TEXT".as_slice(), b"KAMIWP3\0", b"one_time_keys", b"ratchet_key"] {
          assert!(!bytes.windows(secret.len()).any(|w| w == secret));
        }
      }
    }
  }
}
#[test]
fn typed_stale_durable_generation_pin_context_and_digest_never_write() {
  for mutation in [
    "PRAGMA foreign_keys=OFF; DELETE FROM e2ee_account",
    "UPDATE e2ee_account SET digest=zeroblob(32)",
    "UPDATE e2ee_account SET generation=generation+1",
    "UPDATE e2ee_session SET digest=zeroblob(32)",
    "UPDATE e2ee_session SET context=zeroblob(32)",
    "UPDATE e2ee_session SET pin=x'01'",
    "UPDATE e2ee_session SET generation=generation+1",
    "UPDATE contact_trust SET generation=generation+1",
  ] {
    let mut alice = Account::new(); alice.generate_one_time_keys(1);
    let mut bob = Account::new(); bob.generate_one_time_keys(1);
    let (a, _) = endpoint(&alice, 7, false); let (b, tb) = endpoint(&bob, 8, false);
    let bid = DeviceId::from_bytes(Keypair::ed25519_from_bytes([8;32]).unwrap().public().try_into_ed25519().unwrap().to_bytes());
    let source = QuarantinedSession::outbound(alice, a, b, ConversationId::from_u128(1), &tb, &tb.pinned().unwrap().binding().peer_id(), 1000).unwrap();
    let dir = common::directory(); let s = common::create(&dir); seed(&s, &tb, bid);
    let (first, advanced) = prepared(&source, 1, 1, &tb); s.commit_authenticated(first).unwrap();
    let (next, _) = prepared(&advanced, 2, 2, &tb);
    common::raw(&dir).execute_batch(mutation).unwrap();
    let before = counts(&dir);
    assert!(s.commit_authenticated(next).is_err(), "{mutation}");
    assert_eq!(counts(&dir), before, "{mutation}");
    s.shutdown().unwrap();
  }
}
#[test]
fn typed_surface_exists() {
    let _: fn(&kami_storage::Storage, AuthenticatedCommit) -> Result<Vec<u8>, kami_storage::StorageError> = kami_storage::Storage::commit_authenticated;
}
"#;
