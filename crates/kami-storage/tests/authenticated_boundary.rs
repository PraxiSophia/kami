//! External-crate boundary probes without adding test dependencies.
use std::{path::PathBuf, process::Command};
#[path = "common/native.rs"]
mod native;

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
    let base = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::current_dir().ok())
        .expect("a usable private test-directory base");
    let dir = tempfile::Builder::new()
        .prefix("kami-storage-probe-")
        .tempdir_in(base)
        .unwrap();
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
    let Some(_native) = native::isolated() else {
        return;
    };
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
// Lifecycle fixtures remain assertion-based qualification, NOT live provenance.
fn lifecycle_sign(data: OlmBindingData, seed: u8) -> OlmBindingProof {
    let key=Keypair::ed25519_from_bytes([seed;32]).unwrap(); let public=key.public().try_into_ed25519().unwrap().to_bytes();
    let mut wire=b"kami-olm-binding-v1\0".to_vec(); wire.push(1); wire.extend(public);
    let peer=data.publisher_peer.to_bytes(); wire.extend((peer.len() as u16).to_be_bytes()); wire.extend(peer);
    wire.extend(data.olm_ed25519); wire.extend(data.olm_curve25519); wire.push(data.prekey_kind as u8);
    wire.extend((data.prekey_reference.len() as u16).to_be_bytes()); wire.extend(&data.prekey_reference); wire.extend(data.prekey_curve25519);
    for n in [data.binding_generation,data.bundle_generation,data.not_before_ms,data.expires_at_ms] {wire.extend(n.to_be_bytes());}
    wire.extend(key.sign(&wire).unwrap()); OlmBindingProof::parse_bounded(&wire).unwrap()
}
fn lifecycle_trust(n:u8)->ContactTrust { let mut a=Account::new(); a.generate_one_time_keys(1); endpoint(&a,n,false).1 }
fn lifecycle_owner(n:u8,s:&Storage)->(kami_e2ee::prekeys::AccountLifecycle,Vec<OlmBindingProof>) {
    let t=lifecycle_trust(n); let pin=t.pinned().unwrap();
    let owner=kami_e2ee::prekeys::AccountLifecycle::create(*pin.device_key(),pin.binding().peer_id(),1,1,100_000).unwrap();
    let commit=owner.prepare_commit(None,|d|Ok(lifecycle_sign(d,n))).unwrap(); let proofs=commit.proofs().to_vec();
    s.prekey(kami_storage::prekeys::PrekeyOperation::Install(commit)).unwrap(); (owner,proofs)
}
fn lifecycle_fence(t:&ContactTrust,now:i64)->kami_storage::prekeys::PrekeyFence {
    kami_storage::prekeys::PrekeyFence {contact:t.clone(),peer:t.pinned().unwrap().binding().peer_id().to_bytes(),now_ms:now}
}
fn lifecycle_freeze(s:&Storage,o:&kami_e2ee::prekeys::AccountLifecycle,p:&OlmBindingProof,t:&ContactTrust,id:u8)->Result<kami_protocol::prekey_exchange::Selection,StorageError> {
    use kami_storage::prekeys::{PrekeyOperation as O,PrekeyOutcome as R}; use kami_protocol::prekey_exchange::{Exchange as E,Query};
    let query=Query {request:[id;32],requester:*t.pinned().unwrap().device_key(),publisher:*p.device_key(),minimum_generation:1};
    match s.prekey(O::Freeze {fence:lifecycle_fence(t,1000),query,proof:Box::new(p.clone()),account_generation:o.generation(),account_digest:o.snapshot_digest().unwrap()})? {R::Exchange(E::Selection(s))=>Ok(s),_=>panic!()}
}
fn lifecycle_handshake(publisher:&Storage,receiver:&Storage,owner:&kami_e2ee::prekeys::AccountLifecycle,proof:&OlmBindingProof,remote:&ContactTrust,local:&ContactTrust,id:u8)->kami_protocol::prekey_exchange::Selection {
    use kami_storage::prekeys::{PrekeyOperation as O,PrekeyOutcome as R}; use kami_protocol::prekey_exchange::Exchange as E;
    receiver.prekey(O::Begin {fence:lifecycle_fence(local,1000),requester:*remote.pinned().unwrap().device_key(),request:[id;32],retry:false}).unwrap();
    let selection=lifecycle_freeze(publisher,owner,proof,remote,id).unwrap();
    let R::Exchange(E::Ack(ack))=receiver.prekey(O::Retain {fence:lifecycle_fence(local,1000),selection:selection.clone()}).unwrap() else {panic!()};
    publisher.prekey(O::Acknowledge {fence:lifecycle_fence(remote,1000),acknowledgement:ack.clone()}).unwrap();
    receiver.prekey(O::Finish {fence:lifecycle_fence(local,1000),acknowledgement:ack}).unwrap(); selection
}
#[test]
fn lifecycle_global_otk_race_exact_freeze_and_ack_conflicts() {
    use kami_storage::prekeys::{PrekeyOperation as O,PrekeyOutcome as R}; use kami_protocol::prekey_exchange::{self as w,Exchange as E,Acknowledgement};
    let ta=lifecycle_trust(7); let tc=lifecycle_trust(9); let dir=common::directory(); let s=common::fresh_create(&dir);
    seed(&s,&ta,DeviceId::from_bytes(*ta.pinned().unwrap().device_key())); seed(&s,&tc,DeviceId::from_bytes(*tc.pinned().unwrap().device_key()));
    let (owner,proofs)=lifecycle_owner(8,&s); let proof=proofs.iter().find(|p|p.data().prekey_kind==OlmPrekeyKind::OneTime).unwrap();
    let (a,c)=std::thread::scope(|scope| {let a=scope.spawn(||lifecycle_freeze(&s,&owner,proof,&ta,31));let c=scope.spawn(||lifecycle_freeze(&s,&owner,proof,&tc,32));(a.join().unwrap(),c.join().unwrap())});
    assert_ne!(a.is_ok(),c.is_ok()); let (selection,t)=if let Ok(a)=a {(a,&ta)} else {(c.unwrap(),&tc)};
    let again=lifecycle_freeze(&s,&owner,proof,t,selection.request[0]).unwrap(); assert_eq!(w::encode(&E::Selection(selection.clone())).unwrap(),w::encode(&E::Selection(again)).unwrap());
    let bytes=w::encode(&E::Selection(selection.clone())).unwrap(); let digest=kami_e2ee::prekeys::response_digest(&bytes).unwrap();
    let ack=Acknowledgement {request:selection.request,sender:selection.recipient,recipient:selection.publisher,bundle_generation:selection.bundle_generation,response_digest:digest};
    for _ in 0..2 {assert!(matches!(s.prekey(O::Acknowledge {fence:lifecycle_fence(t,1000),acknowledgement:ack.clone()}),Ok(R::Exchange(E::AckResult(_)))));}
    let mut bad=ack.clone();bad.response_digest[0]^=1;assert!(s.prekey(O::Acknowledge {fence:lifecycle_fence(t,1000),acknowledgement:bad}).is_err());
    let n:i64=common::raw(&dir).query_row("SELECT count(*) FROM prekey_publication",[],|r|r.get(0)).unwrap();assert_eq!(n,1);
    s.shutdown().unwrap();let s=common::open(&dir);assert!(lifecycle_freeze(&s,&owner,proof,t,selection.request[0]).is_ok());s.shutdown().unwrap();
}
#[test]
fn lifecycle_faults_freshness_relabel_and_fallback_bounds() {
    use kami_storage::prekeys::{PrekeyOperation as O,PrekeyOutcome as R};
    use kami_protocol::prekey_exchange::Exchange as E;
    let ta=lifecycle_trust(7);let tb=lifecycle_trust(8);
    let da=common::directory();let db=common::directory();let sa=common::fresh_create(&da);let sb=common::fresh_create(&db);
    seed(&sa,&tb,DeviceId::from_bytes(*tb.pinned().unwrap().device_key()));seed(&sb,&ta,DeviceId::from_bytes(*ta.pinned().unwrap().device_key()));
    let (owner,proofs)=lifecycle_owner(8,&sb);
    let proof=proofs.iter().find(|p|p.data().prekey_kind==OlmPrekeyKind::OneTime).unwrap();
    common::raw(&db).execute_batch("CREATE TRIGGER freeze_fault BEFORE INSERT ON prekey_publication BEGIN SELECT RAISE(ABORT,'fault'); END;").unwrap();
    assert!(lifecycle_freeze(&sb,&owner,proof,&ta,51).is_err());
    assert_eq!(common::raw(&db).query_row("SELECT count(*) FROM prekey_publication",[],|r|r.get::<_,i64>(0)).unwrap(),0);
    common::raw(&db).execute_batch("DROP TRIGGER freeze_fault").unwrap();
    let sel=lifecycle_freeze(&sb,&owner,proof,&ta,51).unwrap();
    sa.prekey(O::Begin {fence:lifecycle_fence(&tb,1000),requester:sel.recipient,request:sel.request,retry:false}).unwrap();
    common::raw(&da).execute_batch("CREATE TRIGGER retain_fault BEFORE INSERT ON prekey_received BEGIN SELECT RAISE(ABORT,'fault'); END;").unwrap();
    assert!(sa.prekey(O::Retain {fence:lifecycle_fence(&tb,1000),selection:sel.clone()}).is_err());
    assert_eq!(common::raw(&da).query_row("SELECT count(*) FROM prekey_received",[],|r|r.get::<_,i64>(0)).unwrap(),0);
    common::raw(&da).execute_batch("DROP TRIGGER retain_fault").unwrap();
    let mut bad=sel.clone();bad.request=[52;32];assert!(sa.prekey(O::Retain {fence:lifecycle_fence(&tb,1000),selection:bad}).is_err());
    let R::Exchange(E::Ack(ack))=sa.prekey(O::Retain {fence:lifecycle_fence(&tb,1000),selection:sel.clone()}).unwrap() else {panic!()};
    let mut wrong=lifecycle_fence(&ta,1000);wrong.peer=vec![1];assert!(sb.prekey(O::Acknowledge {fence:wrong,acknowledgement:ack.clone()}).is_err());
    sb.prekey(O::Acknowledge {fence:lifecycle_fence(&ta,1000),acknowledgement:ack.clone()}).unwrap();
    sa.prekey(O::Finish {fence:lifecycle_fence(&tb,1000),acknowledgement:ack}).unwrap();
    let mut conflicting=sel.clone();conflicting.request=[53;32];
    let mut data=proof.data().clone();data.prekey_curve25519[0]^=1;
    conflicting.proof=lifecycle_sign(data,8).to_bytes();
    sa.prekey(O::Begin {fence:lifecycle_fence(&tb,1000),requester:sel.recipient,request:conflicting.request,retry:false}).unwrap();
    assert!(sa.prekey(O::Retain {fence:lifecycle_fence(&tb,1000),selection:conflicting}).is_err(),"signed reference equivocation accepted");
    // Exhaust exclusive inventory through durable exposure, never provider deletion.
    for (i,p) in proofs.iter().filter(|p|p.data().prekey_kind==OlmPrekeyKind::OneTime && p.data().prekey_reference!=proof.data().prekey_reference).enumerate() {
        lifecycle_handshake(&sb,&sa,&owner,p,&ta,&tb,60+i as u8);
    }
    let fallback=proofs.iter().find(|p|p.data().prekey_kind==OlmPrekeyKind::Fallback).unwrap();
    for id in 100..116 { lifecycle_handshake(&sb,&sa,&owner,fallback,&ta,&tb,id); }
    assert!(lifecycle_freeze(&sb,&owner,fallback,&ta,116).is_err());
    assert_eq!(owner.one_time_count(),32);
    // Local clock rollback and revocation block even exact previously acknowledged bytes.
    assert!(sb.prekey(O::State {fence:lifecycle_fence(&ta,999),request:sel.request}).is_err());
    sb.prekey(O::Invalidate {device:None,through:10,retire:false,now_ms:1000}).unwrap();
    let renewed=owner.replenish(1,1000,100_000).unwrap();
    let commit=renewed.prepare_commit(Some(&owner),|d|Ok(lifecycle_sign(d,8))).unwrap();
    let new_proof=commit.proofs().iter().find(|p|p.data().bundle_generation==2).unwrap().clone();
    sb.prekey(O::Install(commit)).unwrap();
    assert!(lifecycle_freeze(&sb,&renewed,&new_proof,&ta,117).is_err(),"revocation floor lost on inventory installation");
    assert!(lifecycle_freeze(&sb,&owner,proof,&ta,51).is_err());
    sa.prekey(O::Invalidate {device:Some(sel.publisher),through:1,retire:true,now_ms:1000}).unwrap();
    assert!(sa.prekey(O::Retain {fence:lifecycle_fence(&tb,1000),selection:sel}).is_err());
    // Expiry is checked in storage, independent of the detached signature.
    assert!(sb.prekey(O::State {fence:lifecycle_fence(&ta,100_001),request:[51;32]}).is_err());
    sa.shutdown().unwrap();sb.shutdown().unwrap();
}
#[test]
fn lifecycle_first_message_consumption_is_atomic_and_exact_retry_is_idempotent() {
    use kami_protocol::prekey_exchange::Exchange as E;
    let ta=lifecycle_trust(7);let tb=lifecycle_trust(8);let da=common::directory();let db=common::directory();let sa=common::fresh_create(&da);let sb=common::fresh_create(&db);
    seed(&sa,&tb,DeviceId::from_bytes(*tb.pinned().unwrap().device_key()));seed(&sb,&ta,DeviceId::from_bytes(*ta.pinned().unwrap().device_key()));
    let (ao,ap)=lifecycle_owner(7,&sa);let (bo,bp)=lifecycle_owner(8,&sb);
    let ap=ap.iter().find(|p|p.data().prekey_kind==OlmPrekeyKind::OneTime).unwrap();let bp=bp.iter().find(|p|p.data().prekey_kind==OlmPrekeyKind::OneTime).unwrap();
    let _asel=lifecycle_handshake(&sa,&sb,&ao,ap,&tb,&ta,41);let bsel=lifecycle_handshake(&sb,&sa,&bo,bp,&ta,&tb,42);
    let a=Endpoint::new(DeviceId::from_bytes(*ap.device_key()),ta.pinned().unwrap().binding().clone(),ap.clone(),ap.data().clone());
    let b=Endpoint::new(DeviceId::from_bytes(*bp.device_key()),tb.pinned().unwrap().binding().clone(),bp.clone(),bp.data().clone());
    let sender=ao.prepare_outbound(a.clone(),b.clone(),&bsel,ConversationId::from_u128(1),&tb,&tb.pinned().unwrap().binding().peer_id(),1000).unwrap();
    let (sent,_sender)=sender.prepare_text(MessageId::from_u128(801),SenderSequence::new(1),1000,"lifecycle atomic marker",&tb,&tb.pinned().unwrap().binding().peer_id(),1000).unwrap();let frame=sa.commit_authenticated(sent).unwrap();
    let expected=kami_protocol::ExpectedContext {conversation_id:ConversationId::from_u128(1),message_id:MessageId::from_u128(801),sender:DeviceId::from_bytes(*ap.device_key()),recipient:DeviceId::from_bytes(*bp.device_key()),receipt_target:None};
    let (candidate,session,proposed)=bo.prepare_inbound(b,a,&bsel,ConversationId::from_u128(1),&frame,&expected,&ta,&ta.pinned().unwrap().binding().peer_id(),1000).unwrap();
    common::raw(&db).execute_batch("CREATE TRIGGER prekey_fault BEFORE INSERT ON e2ee_frame BEGIN SELECT RAISE(ABORT,'fault'); END;").unwrap();assert!(sb.commit_authenticated(candidate).is_err());
    assert_eq!(counts(&db),[1,0,0,0]);let consumed:i64=common::raw(&db).query_row("SELECT sum(consumed) FROM prekey_inventory",[],|r|r.get(0)).unwrap();assert_eq!(consumed,0);
    common::raw(&db).execute_batch("DROP TRIGGER prekey_fault").unwrap();
    for _ in 0..2 {let retry=session.prepare_retry(&ta,&ta.pinned().unwrap().binding().peer_id(),1000).unwrap();assert_eq!(sb.commit_authenticated(retry).unwrap(),frame);}
    assert_eq!(proposed.generation(),2);assert_eq!(proposed.one_time_count(),31);assert_eq!(bo.one_time_count(),32);assert_eq!(counts(&db),[1,1,1,1]);
    let consumed:i64=common::raw(&db).query_row("SELECT sum(consumed) FROM prekey_inventory",[],|r|r.get(0)).unwrap();assert_eq!(consumed,1);
    let renewed=proposed.replenish(1,1001,100_000).unwrap();let commit=renewed.prepare_commit(Some(&proposed),|d|Ok(lifecycle_sign(d,8))).unwrap();sb.prekey(kami_storage::prekeys::PrekeyOperation::Install(commit)).unwrap();
    let session=session.synchronize_account(&renewed).unwrap();let _=session.prepare_retry(&ta,&ta.pinned().unwrap().binding().peer_id(),1001).unwrap();
    let _=E::Selection(bsel);sa.shutdown().unwrap();sb.shutdown().unwrap();
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
fn committed_sender() -> (tempfile::TempDir, Storage, kami_storage::SendRequest, Vec<u8>) {
    committed_sender_messages(1)
}
fn committed_sender_messages(count: u128) -> (tempfile::TempDir, Storage, kami_storage::SendRequest, Vec<u8>) {
    let mut alice=Account::new(); alice.generate_one_time_keys(1);
    let mut bob=Account::new(); bob.generate_one_time_keys(1);
    let (a,_)=endpoint(&alice,7,false); let (b,tb)=endpoint(&bob,8,false);
    let aid=DeviceId::from_bytes(Keypair::ed25519_from_bytes([7;32]).unwrap().public().try_into_ed25519().unwrap().to_bytes());
    let bid=DeviceId::from_bytes(Keypair::ed25519_from_bytes([8;32]).unwrap().public().try_into_ed25519().unwrap().to_bytes());
    let dir=common::directory(); let s=common::fresh_create(&dir); seed(&s,&tb,bid);
    let request=kami_storage::SendRequest::new(MessageId::from_u128(91),ConversationId::from_u128(1),aid,bid,1000,"WP2B-PRIVATE-TEXT".into()).unwrap();
    let kami_storage::Preparation::Prepared(reserved)=s.prepare_send(request.clone(),1000).unwrap() else { panic!() };
    let source=QuarantinedSession::outbound(alice,a,b,ConversationId::from_u128(1),&tb,&tb.pinned().unwrap().binding().peer_id(),1000).unwrap();
    let (stage,mut source)=prepared(&source,91,reserved.sequence().value(),&tb);
    let frame=s.commit_prepared(reserved,stage,1000).unwrap();
    for offset in 1..count {
        let mut next=request.clone(); next.message=MessageId::from_u128(91+offset);
        let kami_storage::Preparation::Prepared(reserved)=s.prepare_send(next,1000).unwrap() else {panic!()};
        let (stage,advanced)=prepared(&source,91+offset,reserved.sequence().value(),&tb);
        s.commit_prepared(reserved,stage,1000).unwrap(); source=advanced;
    }
    (dir,s,request,frame)
}
#[test]
fn recovery_pages_progress_past_held_rows_without_duplicate_or_starvation() {
    let (dir,s,r,_)=committed_sender_messages(3);
    s.hold_send(r.sender,r.message).unwrap(); s.shutdown().unwrap(); let s=common::open(&dir);
    let mut cursor=None; let mut sequences=Vec::new();
    loop {
        let page=s.recover_sends_after(cursor,1).unwrap();
        if page.is_empty() {break;}
        let f=page[0].facts(); sequences.push(f.sequence().value());
        cursor=Some((f.conversation_id(),f.sender(),f.sequence()));
        assert!(sequences.len()<=3);
    }
    assert_eq!(sequences,vec![1,2,3]);
    assert_eq!(s.recover_sends_after(Some((r.conversation,r.sender,SenderSequence::new(u64::MAX))),1),Err(StorageError::InvalidInput));
    s.shutdown().unwrap();
}
#[test]
fn failures_persist_reason_eligibility_and_exhaustion_without_losing_frame() {
    let (dir,s,r,frame)=committed_sender(); let mut now=1000;
    for attempt in 1..=8 {
        let lease=s.claim_send(r.sender,r.message,now).unwrap().unwrap();
        assert_eq!(lease.message().attempts(),attempt);
        s.record_send_failure(lease,kami_core::AttemptFailure::AckLost,now).unwrap();
        let record=s.sender_record(r.sender,r.message).unwrap();
        assert_eq!(record.last_failure(),Some(kami_core::AttemptFailure::AckLost));
        assert_eq!(record.frame(),frame);
        assert!(s.claim_send(r.sender,r.message,now).unwrap().is_none());
        now=record.next_eligible_ms();
    }
    assert_eq!(s.sender_record(r.sender,r.message).unwrap().state(),kami_storage::SendState::Exhausted);
    s.shutdown().unwrap();let s=common::open(&dir);
    assert!(s.claim_send(r.sender,r.message,now).unwrap().is_none());s.shutdown().unwrap();
}
#[test]
fn restart_enumeration_recovers_pending_exact_ciphertext_without_request_plaintext() {
    let (dir,s,r,frame)=committed_sender(); s.shutdown().unwrap(); let s=common::open(&dir);
    let pending=s.recover_sends(10).unwrap(); assert_eq!(pending.len(),1);
    assert_eq!(pending[0].frame(),frame); assert_eq!(pending[0].facts().recipient(),r.recipient);
    let f=pending[0].facts();
    assert!(s.recover_sends_after(Some((f.conversation_id(),f.sender(),f.sequence())),10).unwrap().is_empty());
    assert_eq!(s.recover_sends(0),Err(StorageError::InvalidInput));
    s.cancel_send(r.sender,r.message).unwrap(); assert!(s.recover_sends(10).unwrap().is_empty()); s.shutdown().unwrap();
}
#[test]
fn inconsistent_committed_state_never_returns_preparation_or_send_lease() {
    for mutation in ["UPDATE sender_prepare SET state=0", "DELETE FROM sender_attempt", "UPDATE e2ee_frame SET frame=x'01'"] {
        let (dir,s,r,_)=committed_sender(); common::raw(&dir).execute_batch(mutation).unwrap();
        assert!(s.prepare_send(r.clone(),1001).is_err(),"{mutation}");
        assert!(s.claim_send(r.sender,r.message,1001).is_err(),"{mutation}"); s.shutdown().unwrap();
    }
}
#[test]
fn cancellation_retains_frame_and_rejects_late_callbacks_and_legacy_cancel() {
    let (dir,s,r,frame)=committed_sender();
    let lease=s.claim_send(r.sender,r.message,1000).unwrap().unwrap();
    assert_eq!(s.cancel(r.sender,r.message),Err(StorageError::Conflict));
    s.cancel_send(r.sender,r.message).unwrap(); s.cancel_send(r.sender,r.message).unwrap();
    assert_eq!(s.fail_send(lease,1001),Err(StorageError::Conflict));
    assert!(s.claim_send(r.sender,r.message,1002).unwrap().is_none());
    s.shutdown().unwrap(); let s=common::open(&dir);
    let kami_storage::Preparation::Committed(canceled)=s.prepare_send(r.clone(),1002).unwrap() else {panic!()};
    assert_eq!(canceled.state(),kami_storage::SendState::Canceled); assert_eq!(canceled.frame(),frame);
    assert_eq!(canceled.facts().recipient(),r.recipient);
    assert_eq!(common::raw(&dir).query_row("SELECT payload FROM message",[],|r|r.get::<_,Vec<u8>>(0)).unwrap(),frame);
    s.shutdown().unwrap();
}
#[test]
fn explicit_reverification_of_original_identity_resumes_without_resetting_attempts() {
    let (dir,s,r,frame)=committed_sender();
    let lease=s.claim_send(r.sender,r.message,1000).unwrap().unwrap();
    let row=s.load_contact(*r.recipient.as_bytes()).unwrap().unwrap();
    s.compare_and_swap_contact(Some(2),ContactTrustRow::new(*row.contact(),TrustState::Blocked,3,row.pinned().map(|p|p.to_vec()),row.candidate().to_vec()).unwrap()).unwrap();
    assert!(s.resume_send(r.sender,r.message,3,1001).is_err());
    s.compare_and_swap_contact(Some(3),ContactTrustRow::new(*row.contact(),TrustState::Verified,4,row.pinned().map(|p|p.to_vec()),row.candidate().to_vec()).unwrap()).unwrap();
    assert!(s.resume_send(r.sender,r.message,3,1001).is_err());
    s.resume_send(r.sender,r.message,4,1001).unwrap();
    assert_eq!(s.fail_send(lease,1002),Err(StorageError::Conflict));
    let retry=s.claim_send(r.sender,r.message,1002).unwrap().unwrap();
    assert_eq!(retry.message().attempts(),2); assert_eq!(retry.message().frame(),frame);
    s.shutdown().unwrap();
}
#[test]
fn identity_change_holds_original_frame_and_fences_active_callback() {
    let (dir,s,r,frame)=committed_sender();
    let lease=s.claim_send(r.sender,r.message,1000).unwrap().unwrap();
    let row=s.load_contact(*r.recipient.as_bytes()).unwrap().unwrap();
    s.compare_and_swap_contact(Some(row.generation()),ContactTrustRow::new(*row.contact(),TrustState::Blocked,row.generation()+1,row.pinned().map(|p|p.to_vec()),row.candidate().to_vec()).unwrap()).unwrap();
    assert_eq!(s.fail_send(lease,1001),Err(StorageError::Conflict));
    assert!(s.claim_send(r.sender,r.message,1002).unwrap().is_none());
    let kami_storage::Preparation::Committed(held)=s.prepare_send(r.clone(),1002).unwrap() else {panic!()};
    assert_eq!(held.state(),kami_storage::SendState::HeldIdentity);
    assert_eq!(held.facts().recipient(),r.recipient); assert_eq!(held.frame(),frame);
    s.shutdown().unwrap(); let s=common::open(&dir);
    assert!(s.claim_send(r.sender,r.message,100_000).unwrap().is_none()); s.shutdown().unwrap();
}
#[test]
fn stale_attempt_reclaim_is_durable_and_does_not_count_twice() {
    let (dir,s,r,frame)=committed_sender();
    let first=s.claim_send(r.sender,r.message,1000).unwrap().unwrap();
    assert_eq!(first.message().frame(),frame); assert_eq!(first.message().attempts(),1);
    assert!(s.claim_send(r.sender,r.message,1001).unwrap().is_none());
    s.shutdown().unwrap(); let s=common::open(&dir);
    let reclaimed=s.claim_send(r.sender,r.message,first.expires_at_ms()).unwrap().unwrap();
    assert_eq!(reclaimed.message().frame(),frame); assert_eq!(reclaimed.message().attempts(),1);
    assert_ne!(first,reclaimed);
    assert_eq!(s.fail_send(first,31_001),Err(StorageError::Conflict));
    s.fail_send(reclaimed,31_001).unwrap();
    assert!(s.claim_send(r.sender,r.message,31_002).unwrap().is_none());
    let second=s.claim_send(r.sender,r.message,32_001).unwrap().unwrap();
    assert_eq!(second.message().attempts(),2); assert_eq!(second.message().frame(),frame);
    assert_eq!(counts(&dir),[1,1,1,1]); s.shutdown().unwrap();
}
#[test]
fn sender_prepared_commit_recovery_preserves_exact_frame() {
    let mut alice=Account::new(); alice.generate_one_time_keys(1);
    let mut bob=Account::new(); bob.generate_one_time_keys(1);
    let (a,_)=endpoint(&alice,7,false); let (b,tb)=endpoint(&bob,8,false);
    let aid=DeviceId::from_bytes(Keypair::ed25519_from_bytes([7;32]).unwrap().public().try_into_ed25519().unwrap().to_bytes());
    let bid=DeviceId::from_bytes(Keypair::ed25519_from_bytes([8;32]).unwrap().public().try_into_ed25519().unwrap().to_bytes());
    let dir=common::directory(); let s=common::fresh_create(&dir); seed(&s,&tb,bid);
    let request=kami_storage::SendRequest::new(MessageId::from_u128(91),ConversationId::from_u128(1),aid,bid,1000,"WP2B-PRIVATE-TEXT".into()).unwrap();
    let kami_storage::Preparation::Prepared(reserved)=s.prepare_send(request.clone(),1000).unwrap() else { panic!() };
    let source=QuarantinedSession::outbound(alice,a,b,ConversationId::from_u128(1),&tb,&tb.pinned().unwrap().binding().peer_id(),1000).unwrap();
    let (stage,advanced)=prepared(&source,91,reserved.sequence().value(),&tb);
    s.cleanup_prepared(61_000).unwrap();
    let kami_storage::Preparation::Prepared(renewed)=s.prepare_send(request.clone(),61_000).unwrap() else {panic!()};
    assert_eq!(s.commit_prepared(reserved,stage,61_000),Err(StorageError::Conflict));
    assert_eq!(counts(&dir),[0;4]);
    let (stage,_)=prepared(&source,91,renewed.sequence().value(),&tb);
    let reserved=renewed;
    let retry=advanced.prepare_retry(&tb,&tb.pinned().unwrap().binding().peer_id(),1000).unwrap();
    // The old stage-only API cannot bypass a pending reservation.
    assert_eq!(s.commit_authenticated(retry),Err(StorageError::Busy));
    let frame=s.commit_prepared(reserved,stage,61_000).unwrap();
    s.shutdown().unwrap(); let s=common::open(&dir);
    let kami_storage::Preparation::Committed(recovered)=s.prepare_send(request,61_001).unwrap() else { panic!() };
    assert_eq!(recovered.frame(),frame); assert_eq!(recovered.attempts(),0);
    assert_eq!(counts(&dir),[1,1,1,1]);
    assert_eq!(common::raw(&dir).query_row("SELECT state FROM sender_prepare",[],|r|r.get::<_,i64>(0)).unwrap(),1);
    s.shutdown().unwrap();
}
#[test]
fn outbound_inbound_atomic_retry_cas_prekey_and_receipt_boundaries() {
  for fallback in [false, true] {
    let mut alice = Account::new(); alice.generate_one_time_keys(1); alice.generate_fallback_key();
    let mut bob = Account::new(); bob.generate_one_time_keys(1); bob.generate_fallback_key();
    let (a, ta) = endpoint(&alice, 7, fallback); let (b, tb) = endpoint(&bob, 8, fallback);
    let aid = DeviceId::from_bytes(Keypair::ed25519_from_bytes([7;32]).unwrap().public().try_into_ed25519().unwrap().to_bytes());
    let bid = DeviceId::from_bytes(Keypair::ed25519_from_bytes([8;32]).unwrap().public().try_into_ed25519().unwrap().to_bytes());
    let ad = common::directory(); let bd = common::directory(); let sa = common::fresh_create(&ad); let sb = common::fresh_create(&bd);
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
    let dir = common::directory(); let s = common::fresh_create(&dir); seed(&s, &tb, bid);
    let (c, _) = prepared(&source, 1, 1, &tb);
    s.compare_and_swap_contact(Some(tb.generation()), ContactTrustRow::new(*bid.as_bytes(), TrustState::Blocked, tb.generation()+1, Some(tb.pinned().unwrap().to_bytes()), tb.candidate().to_bytes()).unwrap()).unwrap();
    assert_eq!(s.commit_authenticated(c), Err(StorageError::Conflict)); assert_eq!(counts(&dir), [0;4]); s.shutdown().unwrap();
    let dir = common::directory();
    let s = common::fresh_create_with_config(&dir, kami_storage::Config { outbox_bytes: 0, ..Default::default() }); seed(&s, &tb, bid);
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
      let dir = common::directory(); let s = common::fresh_create(&dir);
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
    let dir = common::directory(); let s = common::fresh_create(&dir); seed(&s, &tb, bid);
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
fn receiver_delivery_and_receipt_commit_are_atomic_and_recoverable() {
  for constrained in [true, false] {
    let mut alice=Account::new(); alice.generate_one_time_keys(1);
    let mut bob=Account::new(); bob.generate_one_time_keys(1);
    let (a,ta)=endpoint(&alice,7,false); let (b,tb)=endpoint(&bob,8,false);
    let aid=DeviceId::from_bytes(Keypair::ed25519_from_bytes([7;32]).unwrap().public().try_into_ed25519().unwrap().to_bytes());
    let bid=DeviceId::from_bytes(Keypair::ed25519_from_bytes([8;32]).unwrap().public().try_into_ed25519().unwrap().to_bytes());
    let ad=common::directory(); let bd=common::directory();
    let source=QuarantinedSession::outbound(alice,a.clone(),b.clone(),ConversationId::from_u128(1),&tb,&tb.pinned().unwrap().binding().peer_id(),1000).unwrap();
    let (sent,sender)=prepared(&source,41,1,&tb); let facts=sent.facts();
    let size=sender.prepare_retry(&tb,&tb.pinned().unwrap().binding().peer_id(),1000).unwrap().into_material().frame().len();
    let sa=common::fresh_create_with_config(&ad,kami_storage::Config {outbox_bytes:if constrained {size as u64} else {16*1024*1024},..Default::default()});
    let sb=common::fresh_create(&bd); seed(&sa,&tb,bid); seed(&sb,&ta,aid);
    let request=kami_storage::SendRequest::new(MessageId::from_u128(41),ConversationId::from_u128(1),aid,bid,1000,"WP2B-PRIVATE-TEXT".into()).unwrap();
    let kami_storage::Preparation::Prepared(reserved)=sa.prepare_send(request,1000).unwrap() else {panic!()};
    assert_eq!(reserved.sequence().value(),1);
    let frame=sa.commit_prepared(reserved,sent,1000).unwrap();
    let expected=kami_protocol::ExpectedContext {message_id:facts.message_id(),conversation_id:facts.conversation_id(),sender:aid,recipient:bid,receipt_target:None};
    let (delivery,receiver)=QuarantinedSession::inbound_delivery(bob,b,a,ConversationId::from_u128(1),&frame,&expected,MessageId::from_u128(42),&ta,&ta.pinned().unwrap().binding().peer_id(),1000).unwrap();
    let ack=delivery.receipt_frame().to_vec();
    let result=sb.commit_delivery(delivery).unwrap(); assert_eq!(result.receipt(),ack); assert_eq!(result.acceptance(),kami_storage::Acceptance::New);
    assert_eq!(common::raw(&bd).query_row("SELECT generation FROM e2ee_session",[],|r|r.get::<_,i64>(0)).unwrap(),2);
    assert_eq!(sb.history(facts.conversation_id(),10).unwrap().len(),1);
    // A surviving in-memory receiver may not decrypt the committed frame again.
    assert!(receiver.prepare_receive(&frame,&expected,&ta,&ta.pinned().unwrap().binding().peer_id(),1000).is_err());
    sb.shutdown().unwrap(); let sb=common::open(&bd);
    let permit=kami_e2ee::ReplayPermit::new(&ta,&ta.pinned().unwrap().binding().peer_id(),aid,bid,facts.conversation_id()).unwrap();
    let recovered=sb.recover_delivery(permit,frame.clone()).unwrap().unwrap(); assert_eq!(recovered.receipt(),ack);
    let expected_ack=kami_protocol::ExpectedContext {message_id:MessageId::from_u128(42),sender:bid,recipient:aid,receipt_target:Some(facts.message_id()),..expected};
    let (receipt,_)=sender.prepare_receipt(&ack,&expected_ack,facts,&frame,&tb,&tb.pinned().unwrap().binding().peer_id(),1000).unwrap();
    if constrained {
        assert_eq!(sa.commit_delivery_receipt(receipt),Err(StorageError::Full));
        assert_eq!(sa.sender_record(aid,facts.message_id()).unwrap().state(),kami_storage::SendState::Queued);
        sa.shutdown().unwrap(); let sa=common::open(&ad);
        let (receipt,_)=sender.prepare_receipt(&ack,&expected_ack,facts,&frame,&tb,&tb.pinned().unwrap().binding().peer_id(),1000).unwrap();
        assert_eq!(sa.commit_delivery_receipt(receipt),Err(StorageError::EnrollmentRequired));
        sa.shutdown().unwrap(); sb.shutdown().unwrap(); continue;
    }
    common::raw(&ad).execute_batch("CREATE TRIGGER receipt_fault BEFORE INSERT ON receipt BEGIN SELECT RAISE(ABORT,'injected'); END").unwrap();
    assert!(sa.commit_delivery_receipt(receipt).is_err());
    assert_eq!(sa.sender_record(aid,facts.message_id()).unwrap().state(),kami_storage::SendState::Queued);
    assert_eq!(common::raw(&ad).query_row("SELECT generation FROM e2ee_session",[],|r|r.get::<_,i64>(0)).unwrap(),1);
    assert_eq!(common::raw(&ad).query_row("SELECT count(*) FROM delivery_artifact",[],|r|r.get::<_,i64>(0)).unwrap(),0);
    common::raw(&ad).execute_batch("DROP TRIGGER receipt_fault").unwrap();
    let (receipt,_)=sender.prepare_receipt(&ack,&expected_ack,facts,&frame,&tb,&tb.pinned().unwrap().binding().peer_id(),1000).unwrap();
    sa.commit_delivery_receipt(receipt).unwrap();
    assert_eq!(sa.sender_record(aid,facts.message_id()).unwrap().state(),kami_storage::SendState::Delivered);
    assert!(sa.recover_sends(10).unwrap().is_empty());
    let (duplicate,_)=sender.prepare_receipt(&ack,&expected_ack,facts,&frame,&tb,&tb.pinned().unwrap().binding().peer_id(),1000).unwrap();
    assert_eq!(sa.commit_delivery_receipt(duplicate).unwrap(),kami_storage::Acceptance::Duplicate);
    assert_eq!(common::raw(&ad).query_row("SELECT generation FROM e2ee_session",[],|r|r.get::<_,i64>(0)).unwrap(),2);
    sa.shutdown().unwrap(); let sa=common::open(&ad);
    assert_eq!(sa.recovery_status().unwrap(),kami_storage::RecoveryStatus::Disabled);
    assert_eq!(sa.sender_record(aid,facts.message_id()).unwrap().state(),kami_storage::SendState::Delivered);
    let permit=kami_e2ee::ReplayPermit::new(&tb,&tb.pinned().unwrap().binding().peer_id(),bid,aid,facts.conversation_id()).unwrap();
    assert_eq!(sa.recover_delivery_receipt(permit,ack).unwrap(),Some(facts));
    sa.shutdown().unwrap(); sb.shutdown().unwrap();
  }
}
#[test]
fn typed_surface_exists() {
    let _: fn(&kami_storage::Storage, AuthenticatedCommit) -> Result<Vec<u8>, kami_storage::StorageError> = kami_storage::Storage::commit_authenticated;
}
#[test]
fn coherent_db_wal_rollback_never_reactivates_exposed_consumed_or_retired_authority() {
    use kami_storage::prekeys::{PrekeyOperation as O, PrekeyOutcome as R};
    use kami_protocol::prekey_exchange::Query;
    fn snapshot(dir:&tempfile::TempDir)->tempfile::TempDir {
        common::raw(dir).execute_batch("PRAGMA wal_checkpoint(FULL)").unwrap();
        let copy=common::directory();
        for file in ["messages.db","messages.db-wal"] {
            if dir.path().join(file).exists() {std::fs::copy(dir.path().join(file),copy.path().join(file)).unwrap();}
        }
        copy
    }
    let ta=lifecycle_trust(7); let tb=lifecycle_trust(8);
    let da=common::directory(); let db=common::directory();
    let sa=common::fresh_create(&da); let sb=common::fresh_create(&db);
    seed(&sa,&tb,DeviceId::from_bytes(*tb.pinned().unwrap().device_key()));
    seed(&sb,&ta,DeviceId::from_bytes(*ta.pinned().unwrap().device_key()));
    let (ao,ap)=lifecycle_owner(7,&sa); let (bo,bp)=lifecycle_owner(8,&sb);
    let ap=ap.iter().find(|p|p.data().prekey_kind==OlmPrekeyKind::OneTime).unwrap();
    let bp=bp.iter().find(|p|p.data().prekey_kind==OlmPrekeyKind::OneTime).unwrap();
    let before_exposure=snapshot(&db);
    let ab=lifecycle_handshake(&sb,&sa,&bo,bp,&ta,&tb,71);
    let _ba=lifecycle_handshake(&sa,&sb,&ao,ap,&tb,&ta,72);
    let before_consumption=snapshot(&db);
    let a=Endpoint::new(DeviceId::from_bytes(*ap.device_key()),ta.pinned().unwrap().binding().clone(),ap.clone(),ap.data().clone());
    let b=Endpoint::new(DeviceId::from_bytes(*bp.device_key()),tb.pinned().unwrap().binding().clone(),bp.clone(),bp.data().clone());
    let sender=ao.prepare_outbound(a.clone(),b.clone(),&ab,ConversationId::from_u128(7),&tb,&tb.pinned().unwrap().binding().peer_id(),1000).unwrap();
    let (sent,_)=sender.prepare_text(MessageId::from_u128(701),SenderSequence::new(1),1000,"rollback fixture",&tb,&tb.pinned().unwrap().binding().peer_id(),1000).unwrap();
    let facts=sent.facts(); let frame=sa.commit_authenticated(sent).unwrap();
    let expected=kami_protocol::ExpectedContext {message_id:facts.message_id(),conversation_id:facts.conversation_id(),sender:facts.sender(),recipient:facts.recipient(),receipt_target:None};
    let (candidate,received,_)=bo.prepare_inbound(b,a,&ab,facts.conversation_id(),&frame,&expected,&ta,&ta.pinned().unwrap().binding().peer_id(),1000).unwrap();
    sb.commit_authenticated(candidate).unwrap();
    let after_consumption=snapshot(&db);
    sb.prekey(O::Invalidate {device:None,through:1,retire:true,now_ms:1000}).unwrap();
    sb.shutdown().unwrap();
    for (phase,copy) in [before_exposure,before_consumption,after_consumption].into_iter().enumerate() {
        for file in ["messages.db-wal","messages.db-shm"] {
            if db.path().join(file).exists() { std::fs::remove_file(db.path().join(file)).unwrap(); }
        }
        for file in ["messages.db","messages.db-wal"] {
            if copy.path().join(file).exists() { std::fs::copy(copy.path().join(file),db.path().join(file)).unwrap(); }
        }
        let restored=common::open(&db);
        assert_eq!(restored.recovery_status().unwrap(),kami_storage::RecoveryStatus::Disabled);
        let prior=counts(&db);
        let R::State(state)=restored.prekey(O::State {fence:lifecycle_fence(&ta,1000),request:[99;32]}).unwrap() else {panic!()};
        assert!(state.available.is_empty(),"phase {phase} cannot publish old inventory");
        let query=Query {request:[99;32],requester:*ta.pinned().unwrap().device_key(),publisher:*tb.pinned().unwrap().device_key(),minimum_generation:1};
        assert!(restored.prekey(O::Freeze {fence:lifecycle_fence(&ta,1000),query,proof:Box::new(bp.clone()),account_generation:1,account_digest:bo.snapshot_digest().unwrap()}).is_err());
        let exact=received.prepare_retry(&ta,&ta.pinned().unwrap().binding().peer_id(),1000).unwrap();
        if phase==2 { assert_eq!(restored.commit_authenticated(exact).unwrap(),frame); }
        else { assert!(restored.commit_authenticated(exact).is_err()); }
        let next=received.prepare_text(MessageId::from_u128(702),SenderSequence::new(1),1000,"must not transition",&ta,&ta.pinned().unwrap().binding().peer_id(),1000).unwrap().0;
        assert!(restored.commit_authenticated(next).is_err());
        assert_eq!(counts(&db),prior,"rollback reads must not advance account/session/frame");
        restored.shutdown().unwrap();
    }
    sa.shutdown().unwrap();
}
#[test]
fn generic_worker_refuses_valid_opaque_crypto_and_prekey_stages() {
    let mut alice=Account::new(); alice.generate_one_time_keys(1);
    let mut bob=Account::new(); bob.generate_one_time_keys(1);
    let (a,_)=endpoint(&alice,7,false); let (b,tb)=endpoint(&bob,8,false);
    let dir=common::directory(); let storage=common::create(&dir);
    seed(&storage,&tb,DeviceId::from_bytes(*tb.pinned().unwrap().device_key()));
    let source=QuarantinedSession::outbound(alice,a,b,ConversationId::from_u128(1),&tb,&tb.pinned().unwrap().binding().peer_id(),1000).unwrap();
    let (candidate,_)=prepared(&source,1,1,&tb);
    assert!(storage.commit_authenticated(candidate).is_err());
    let pin=tb.pinned().unwrap();
    let owner=kami_e2ee::prekeys::AccountLifecycle::create(*pin.device_key(),pin.binding().peer_id(),1,1,100_000).unwrap();
    let stage=owner.prepare_commit(None,|d|Ok(lifecycle_sign(d,8))).unwrap();
    assert!(matches!(storage.prekey(kami_storage::prekeys::PrekeyOperation::Install(stage)),Err(StorageError::EnrollmentRequired)));
    assert_eq!(counts(&dir),[0;4]);
    assert_eq!(storage.recovery_status().unwrap(),kami_storage::RecoveryStatus::Disabled);
}
"#;
