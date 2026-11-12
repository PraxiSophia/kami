//! SQLCipher lifecycle metadata and global exact-prior-state CAS.
//! Fence arguments are assertions, not network provenance; runtime supplies live
//! driver evidence and rechecks it after every blocking worker transition.
use crate::{
    session_state::{self, AccountRow},
    StorageError, TrustState,
};
use kami_identity::{ContactTrust, OlmBindingProof, OlmPrekeyKind};
use kami_protocol::prekey_exchange::{self as wire, Acknowledgement, Exchange, Query, Selection};
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use sha2::{Digest, Sha256};

pub struct PrekeyFence {
    pub contact: ContactTrust,
    pub peer: Vec<u8>,
    pub now_ms: i64,
}
pub enum PrekeyOperation {
    Invalidate {
        device: Option<[u8; 32]>,
        through: u64,
        retire: bool,
        now_ms: i64,
    },
    Begin {
        fence: PrekeyFence,
        requester: [u8; 32],
        request: [u8; 32],
        retry: bool,
    },
    Install(kami_e2ee::prekeys::LifecycleCommit),
    State {
        fence: PrekeyFence,
        request: [u8; 32],
    },
    Freeze {
        fence: PrekeyFence,
        query: Query,
        proof: Box<OlmBindingProof>,
        account_generation: u64,
        account_digest: [u8; 32],
    },
    Retain {
        fence: PrekeyFence,
        selection: Selection,
    },
    Acknowledge {
        fence: PrekeyFence,
        acknowledgement: Acknowledgement,
    },
    Finish {
        fence: PrekeyFence,
        acknowledgement: Acknowledgement,
    },
}
pub struct PrekeyState {
    pub account_generation: Option<u64>,
    pub account_digest: Option<[u8; 32]>,
    pub available: Vec<OlmBindingProof>,
    pub frozen: Option<Exchange>,
    pub minimum_generation: u64,
}
pub enum PrekeyOutcome {
    Installed,
    State(PrekeyState),
    Exchange(Exchange),
}
fn conflict<T>() -> Result<T, StorageError> {
    Err(StorageError::Conflict)
}
fn encoded(v: &Exchange) -> Result<Vec<u8>, StorageError> {
    wire::encode(v).map_err(|_| StorageError::InvalidInput)
}
fn decoded(v: &[u8]) -> Result<Exchange, StorageError> {
    wire::decode(v).map_err(|_| StorageError::WrongKeyOrCorrupt)
}
fn clock(db: &Connection, now: i64) -> Result<(), StorageError> {
    let floor: i64 = db.query_row(
        "SELECT floor_ms FROM prekey_clock WHERE singleton=1",
        [],
        |r| r.get(0),
    )?;
    if now < floor || now < 0 {
        return conflict();
    }
    db.execute(
        "UPDATE prekey_clock SET floor_ms=?1 WHERE singleton=1",
        [now],
    )?;
    Ok(())
}
fn fence(db: &Connection, f: &PrekeyFence) -> Result<([u8; 32], Vec<u8>), StorageError> {
    clock(db, f.now_ms)?;
    let pin = f.contact.pinned().ok_or(StorageError::Conflict)?;
    let device = *pin.device_key();
    let row = crate::contact_trust::load(db, device)?.ok_or(StorageError::Conflict)?;
    if f.contact.state() != kami_identity::ContactTrustState::Verified
        || row.state() != TrustState::Verified
        || row.generation() != f.contact.generation()
        || row.pinned() != Some(pin.to_bytes().as_slice())
        || f.peer != pin.binding().peer_id().to_bytes()
    {
        return conflict();
    }
    Ok((device, pin.to_bytes()))
}
fn valid(proof: &OlmBindingProof, now: i64) -> bool {
    let p = proof.data();
    now >= 0 && p.not_before_ms <= now as u64 && (now as u64) < p.expires_at_ms
}
fn read_blob(
    db: &Connection,
    sql: &str,
    request: [u8; 32],
) -> Result<Option<Vec<u8>>, StorageError> {
    let mut stmt = db.prepare(sql)?;
    let mut rows = stmt.query([request])?;
    let Some(row) = rows.next()? else {
        return Ok(None);
    };
    if !(1..=wire::MAX_BODY_BYTES as i64).contains(&row.get::<_, i64>(0)?) {
        return Err(StorageError::WrongKeyOrCorrupt);
    }
    Ok(Some(row.get(1)?))
}
fn pending(db: &Connection, table: &str, now: i64) -> Result<(), StorageError> {
    let sql = match table { "prekey_publication" => "SELECT count(*) FROM prekey_publication WHERE acknowledged=0 AND revoked=0 AND expires_at_ms>?1", _ => "SELECT count(*) FROM prekey_received WHERE completed=0 AND expires_at_ms>?1" };
    let n: i64 = db.query_row(sql, [now], |r| r.get(0))?;
    if n >= 16 {
        return Err(StorageError::Busy);
    }
    Ok(())
}
pub(crate) fn execute(
    db: &mut Connection,
    operation: PrekeyOperation,
) -> Result<PrekeyOutcome, StorageError> {
    let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let result = match operation {
        PrekeyOperation::Invalidate { device, through, retire, now_ms } => {
            if through == 0 || through > i64::MAX as u64 { return Err(StorageError::InvalidInput); }
            clock(&tx, now_ms)?;
            if let Some(device) = device {
                let (revoked, retired) = if retire { (0, through as i64) } else { (through as i64, 0) };
                tx.execute("INSERT INTO prekey_authority(device,binding_floor,bundle_floor,account_floor,revoked_through,retired_through) VALUES(?1,1,1,1,?2,?3) ON CONFLICT(device) DO UPDATE SET revoked_through=max(revoked_through,excluded.revoked_through),retired_through=max(retired_through,excluded.retired_through)",params![device,revoked,retired])?;
            } else {
                let floor_sql = if retire { "UPDATE prekey_clock SET retired_through=max(retired_through,?1) WHERE singleton=1" } else { "UPDATE prekey_clock SET revoked_through=max(revoked_through,?1) WHERE singleton=1" };
                tx.execute(floor_sql,[through as i64])?;
                let sql = if retire { "UPDATE prekey_inventory SET retired=1 WHERE bundle_generation<=?1" } else { "UPDATE prekey_inventory SET revoked=1 WHERE bundle_generation<=?1" };
                tx.execute(sql,[through as i64])?;
                tx.execute("UPDATE prekey_publication SET revoked=1 WHERE bundle_generation<=?1",[through as i64])?;
            }
            PrekeyOutcome::Installed
        }
        PrekeyOperation::Begin { fence: f, requester, request, retry } => {
            let (device, pin) = fence(&tx, &f)?;
            if let Some(bytes) = read_blob(&tx, "SELECT length(query),query FROM prekey_query WHERE request=?1", request)? {
                let valid: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM prekey_query WHERE request=?1 AND peer=?2 AND device=?3 AND trust_generation=?4 AND pin=?5)",params![request,f.peer,device,f.contact.generation() as i64,pin],|r|r.get(0))?;
                let value = decoded(&bytes)?;
                if !valid || !matches!(&value, Exchange::Query(q) if q.requester==requester && q.publisher==device) { return conflict(); }
                PrekeyOutcome::Exchange(value)
            } else {
                if retry { return conflict(); }
                let count: i64 = tx.query_row("SELECT count(*) FROM prekey_query WHERE completed=0", [], |r| r.get(0))?;
                if count >= 16 { return Err(StorageError::Busy); }
                let floor = tx.query_row("SELECT bundle_floor FROM prekey_authority WHERE device=?1",[device],|r|r.get::<_,i64>(0)).optional()?.unwrap_or(1) as u64;
                let value = Exchange::Query(Query { request, requester, publisher: device, minimum_generation: floor });
                let bytes = encoded(&value)?;
                tx.execute("INSERT INTO prekey_query(request,peer,device,trust_generation,pin,query) VALUES(?1,?2,?3,?4,?5,?6)",params![request,f.peer,device,f.contact.generation() as i64,pin,bytes])?;
                PrekeyOutcome::Exchange(value)
            }
        }
        PrekeyOperation::Install(commit) => commit.consume(|previous, generation, digest, state, proofs| {
            let previous = previous.map(|(n,d,s)| AccountRow::new(n, s.to_vec(), d)).transpose()?;
            let next = AccountRow::new(generation, state.to_vec(), digest)?;
            // Exact committed replay is reconciliation, never a second mutation.
            if session_state::load_account(&tx)?.as_ref() == Some(&next) { return Ok(PrekeyOutcome::Installed); }
            session_state::put_account(&tx, previous.as_ref(), &next)?;
            if proofs.is_empty() || proofs.len() > kami_e2ee::prekeys::MAX_INVENTORY + 2 { return Err(StorageError::InvalidInput); }
            for proof in proofs {
                let p = proof.data(); let bytes = proof.to_bytes();
                tx.execute("INSERT INTO prekey_inventory(reference,kind,proof,bundle_generation,not_before_ms,expires_at_ms) VALUES(?1,?2,?3,?4,?5,?6) ON CONFLICT(reference) DO UPDATE SET proof=excluded.proof,bundle_generation=excluded.bundle_generation,not_before_ms=excluded.not_before_ms,expires_at_ms=excluded.expires_at_ms WHERE kind=excluded.kind AND bundle_generation<=excluded.bundle_generation", params![p.prekey_reference, p.prekey_kind as i64, bytes, p.bundle_generation as i64, p.not_before_ms as i64, p.expires_at_ms as i64])?;
            }
            tx.execute("UPDATE prekey_inventory SET revoked=1 WHERE bundle_generation<=(SELECT revoked_through FROM prekey_clock WHERE singleton=1)",[])?;
            tx.execute("UPDATE prekey_inventory SET retired=1 WHERE bundle_generation<=(SELECT retired_through FROM prekey_clock WHERE singleton=1)",[])?;
            Ok(PrekeyOutcome::Installed)
        })?,
        PrekeyOperation::State { fence: f, request } => {
            let (device, pin) = fence(&tx, &f)?;
            let account = session_state::load_account(&tx)?;
            let minimum_generation = tx.query_row("SELECT bundle_floor FROM prekey_authority WHERE device=?1", [device], |r| r.get::<_, i64>(0)).optional()?.unwrap_or(1) as u64;
            let frozen = frozen(&tx, &f, device, &pin, request)?;
            let mut stmt = tx.prepare("SELECT length(proof),proof FROM prekey_inventory i WHERE revoked=0 AND retired=0 AND consumed=0 AND not_before_ms<=?1 AND expires_at_ms>?1 AND (kind=2 OR NOT EXISTS(SELECT 1 FROM prekey_publication p WHERE p.reference=i.reference)) ORDER BY kind, bundle_generation DESC, reference LIMIT 52")?;
            let mut rows = stmt.query([f.now_ms])?; let mut available = Vec::new();
            while let Some(row) = rows.next()? {
                if !(1..=1024).contains(&row.get::<_, i64>(0)?) { return Err(StorageError::WrongKeyOrCorrupt); }
                let bytes: Vec<u8> = row.get(1)?;
                let proof = OlmBindingProof::parse_bounded(&bytes).map_err(|_| StorageError::WrongKeyOrCorrupt)?;
                available.push(proof);
            }
            PrekeyOutcome::State(PrekeyState { account_generation: account.as_ref().map(|a| a.generation), account_digest: account.map(|a| a.digest), available, frozen, minimum_generation })
        }
        PrekeyOperation::Freeze { fence: f, query, proof, account_generation, account_digest } => {
            let (device, pin) = fence(&tx, &f)?;
            let bytes = encoded(&Exchange::Query(query.clone()))?;
            if query.requester != device || query.publisher != *proof.device_key() || !valid(&proof, f.now_ms) || proof.data().bundle_generation < query.minimum_generation { return conflict(); }
            if let Some(value) = frozen(&tx, &f, device, &pin, query.request)? {
                let old = read_blob(&tx, "SELECT length(query),query FROM prekey_publication WHERE request=?1", query.request)?.ok_or(StorageError::Conflict)?;
                if old != bytes { return conflict(); } value
            } else {
                pending(&tx, "prekey_publication", f.now_ms)?;
                let account = session_state::load_account(&tx)?.ok_or(StorageError::Conflict)?;
                if account.generation != account_generation || account.digest != account_digest { return conflict(); }
                let p = proof.data();
                let retained: Option<Vec<u8>> = tx.query_row("SELECT proof FROM prekey_inventory WHERE reference=?1 AND revoked=0 AND retired=0 AND consumed=0 AND not_before_ms<=?2 AND expires_at_ms>?2", params![p.prekey_reference,f.now_ms], |r| r.get(0)).optional()?;
                if retained != Some(proof.to_bytes()) { return conflict(); }
                let has_otk: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM prekey_inventory i WHERE kind=1 AND revoked=0 AND retired=0 AND consumed=0 AND not_before_ms<=?1 AND expires_at_ms>?1 AND NOT EXISTS(SELECT 1 FROM prekey_publication p WHERE p.reference=i.reference))", [f.now_ms], |r| r.get(0))?;
                if p.prekey_kind == OlmPrekeyKind::Fallback {
                    let count: i64 = tx.query_row("SELECT count(*) FROM prekey_publication WHERE reference=?1", [&p.prekey_reference], |r| r.get(0))?;
                    if has_otk || count >= 16 { return conflict(); }
                }
                let value = Exchange::Selection(Selection { request: query.request, publisher: query.publisher, recipient: device, account_generation, bundle_generation: p.bundle_generation, proof: proof.to_bytes() });
                let response = encoded(&value)?; let digest: [u8; 32] = Sha256::digest(&response).into();
                tx.execute("INSERT INTO prekey_publication(request,query,peer,device,trust_generation,pin,account_generation,account_digest,reference,kind,bundle_generation,response,response_digest,not_before_ms,expires_at_ms) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15)",params![query.request,bytes,f.peer,device,f.contact.generation() as i64,pin,account_generation as i64,account_digest,p.prekey_reference,p.prekey_kind as i64,p.bundle_generation as i64,response,digest,p.not_before_ms as i64,p.expires_at_ms as i64])?;
                value
            }.pipe(PrekeyOutcome::Exchange)
        }
        PrekeyOperation::Retain { fence: f, selection } => {
            let (device, pin) = fence(&tx, &f)?;
            let query = read_blob(&tx, "SELECT length(query),query FROM prekey_query WHERE request=?1", selection.request)?.ok_or(StorageError::Conflict)?;
            let Exchange::Query(query) = decoded(&query)? else { return conflict(); };
            let correlated: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM prekey_query WHERE request=?1 AND peer=?2 AND device=?3 AND trust_generation=?4 AND pin=?5)",params![selection.request,f.peer,device,f.contact.generation() as i64,pin],|r|r.get(0))?;
            if !correlated || query.requester != selection.recipient || query.publisher != selection.publisher || selection.bundle_generation < query.minimum_generation { return conflict(); }
            let response = encoded(&Exchange::Selection(selection.clone()))?;
            let proof = OlmBindingProof::parse_bounded(&selection.proof).map_err(|_| StorageError::InvalidInput)?;
            let p = proof.data();
            if selection.publisher != device || proof.device_key() != &device || p.publisher_peer.to_bytes() != f.peer || p.bundle_generation != selection.bundle_generation || !valid(&proof, f.now_ms) { return conflict(); }
            proof.verify(device, f.contact.pinned().ok_or(StorageError::Conflict)?.binding(), p, f.now_ms as u64).map_err(|_| StorageError::Conflict)?;
            let floors: Option<(i64,i64,i64,i64,i64)> = tx.query_row("SELECT binding_floor,bundle_floor,account_floor,revoked_through,retired_through FROM prekey_authority WHERE device=?1", [device], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?))).optional()?;
            if floors.is_some_and(|(b,g,a,r,t)| p.binding_generation < b as u64 || p.bundle_generation < g as u64 || selection.account_generation < a as u64 || p.bundle_generation <= r as u64 || p.bundle_generation <= t as u64) { return conflict(); }
            check_conflicting_selection(&tx, &selection, &proof)?;
            let digest = Sha256::digest(&response).into();
            let acknowledgement = Acknowledgement { request: selection.request, sender: selection.recipient, recipient: device, bundle_generation: selection.bundle_generation, response_digest: digest };
            let ack = encoded(&Exchange::Ack(acknowledgement.clone()))?;
            if let Some(old) = read_blob(&tx, "SELECT length(response),response FROM prekey_received WHERE request=?1", selection.request)? {
                if old != response { return conflict(); }
                check_received(&tx, &f, device, &pin, selection.request)?;
            } else {
                pending(&tx, "prekey_received", f.now_ms)?;
                tx.execute("INSERT INTO prekey_received(request,peer,device,trust_generation,pin,response,ack,bundle_generation,expires_at_ms) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)", params![selection.request,f.peer,device,f.contact.generation() as i64,pin,response,ack,selection.bundle_generation as i64,p.expires_at_ms as i64])?;
            }
            tx.execute("INSERT INTO prekey_authority(device,binding_floor,bundle_floor,account_floor) VALUES(?1,?2,?3,?4) ON CONFLICT(device) DO UPDATE SET binding_floor=max(binding_floor,excluded.binding_floor),bundle_floor=max(bundle_floor,excluded.bundle_floor),account_floor=max(account_floor,excluded.account_floor)", params![device,p.binding_generation as i64,p.bundle_generation as i64,selection.account_generation as i64])?;
            PrekeyOutcome::Exchange(Exchange::Ack(acknowledgement))
        }
        PrekeyOperation::Acknowledge { fence: f, acknowledgement: a } => {
            let (device, pin) = fence(&tx, &f)?;
            let Some(Exchange::Selection(s)) = frozen(&tx, &f, device, &pin, a.request)? else { return conflict(); };
            let expected: [u8; 32] = Sha256::digest(encoded(&Exchange::Selection(s.clone()))?).into();
            if a.sender != device || a.recipient != s.publisher || a.bundle_generation != s.bundle_generation || a.response_digest != expected { return conflict(); }
            tx.execute("UPDATE prekey_publication SET acknowledged=1 WHERE request=?1", [a.request])?;
            // Upstream's mark_keys_as_published is global. Per-peer ACKs never
            // mark unrelated pending provider keys/batches globally published.
            PrekeyOutcome::Exchange(Exchange::AckResult(a))
        }
        PrekeyOperation::Finish { fence: f, acknowledgement: a } => {
            let (device, pin) = fence(&tx, &f)?; check_received(&tx, &f, device, &pin, a.request)?;
            let ack = read_blob(&tx, "SELECT length(ack),ack FROM prekey_received WHERE request=?1", a.request)?.ok_or(StorageError::Conflict)?;
            if ack != encoded(&Exchange::Ack(a.clone()))? { return conflict(); }
            tx.execute("UPDATE prekey_received SET completed=1 WHERE request=?1", [a.request])?;
            tx.execute("UPDATE prekey_query SET completed=1 WHERE request=?1", [a.request])?;
            PrekeyOutcome::Exchange(Exchange::AckResult(a))
        }
    };
    tx.commit()?;
    Ok(result)
}
// History is capped at 1024 durable rows; inspect one bounded frame at a time.
fn check_conflicting_selection(
    db: &Connection,
    selection: &Selection,
    proof: &OlmBindingProof,
) -> Result<(), StorageError> {
    let mut stmt = db.prepare(
        "SELECT length(response),response FROM prekey_received WHERE device=?1 LIMIT 1025",
    )?;
    let mut rows = stmt.query([selection.publisher])?;
    let mut count = 0;
    while let Some(row) = rows.next()? {
        count += 1;
        if count > 1024 || !(1..=wire::MAX_BODY_BYTES as i64).contains(&row.get::<_, i64>(0)?) {
            return Err(StorageError::WrongKeyOrCorrupt);
        }
        let bytes: Vec<u8> = row.get(1)?;
        let Exchange::Selection(old) = decoded(&bytes)? else {
            return Err(StorageError::WrongKeyOrCorrupt);
        };
        let previous = OlmBindingProof::parse_bounded(&old.proof)
            .map_err(|_| StorageError::WrongKeyOrCorrupt)?;
        let (a, b) = (previous.data(), proof.data());
        if a.olm_ed25519 != b.olm_ed25519 || a.olm_curve25519 != b.olm_curve25519 {
            return conflict();
        }
        if a.prekey_reference == b.prekey_reference
            && (previous != *proof
                || (old.request != selection.request && b.prekey_kind == OlmPrekeyKind::OneTime))
        {
            return conflict();
        }
    }
    Ok(())
}
fn check_received(
    db: &Connection,
    f: &PrekeyFence,
    device: [u8; 32],
    pin: &[u8],
    request: [u8; 32],
) -> Result<(), StorageError> {
    let valid: bool = db.query_row("SELECT EXISTS(SELECT 1 FROM prekey_received r JOIN prekey_authority a ON a.device=r.device WHERE request=?1 AND r.peer=?2 AND r.device=?3 AND r.trust_generation=?4 AND r.pin=?5 AND r.expires_at_ms>?6 AND r.bundle_generation>=a.bundle_floor AND r.bundle_generation>a.revoked_through AND r.bundle_generation>a.retired_through)",params![request,f.peer,device,f.contact.generation() as i64,pin,f.now_ms],|r|r.get(0))?;
    if !valid {
        return conflict();
    }
    Ok(())
}
fn frozen(
    db: &Connection,
    f: &PrekeyFence,
    device: [u8; 32],
    pin: &[u8],
    request: [u8; 32],
) -> Result<Option<Exchange>, StorageError> {
    let Some(bytes) = read_blob(
        db,
        "SELECT length(response),response FROM prekey_publication WHERE request=?1",
        request,
    )?
    else {
        return Ok(None);
    };
    let valid: bool = db.query_row("SELECT EXISTS(SELECT 1 FROM prekey_publication p JOIN prekey_inventory i ON i.reference=p.reference WHERE p.request=?1 AND p.peer=?2 AND p.device=?3 AND p.trust_generation=?4 AND p.pin=?5 AND p.not_before_ms<=?6 AND p.expires_at_ms>?6 AND p.revoked=0 AND p.consumed=0 AND i.revoked=0 AND i.retired=0 AND i.consumed=0 AND i.bundle_generation=p.bundle_generation)",params![request,f.peer,device,f.contact.generation() as i64,pin,f.now_ms],|r|r.get(0))?;
    if !valid {
        return conflict();
    }
    Ok(Some(decoded(&bytes)?))
}
/// Called inside the existing session/message IMMEDIATE transaction.
pub(crate) fn use_selection(
    db: &Connection,
    usage: &kami_e2ee::prekeys::PrekeyUse,
    session: [u8; 32],
    contact: [u8; 32],
    now: i64,
    first: bool,
    advances_account: bool,
) -> Result<(), StorageError> {
    clock(db, now)?;
    if first {
        if usage.local() {
            let valid: bool = db.query_row("SELECT EXISTS(SELECT 1 FROM prekey_publication p JOIN prekey_inventory i USING(reference) WHERE p.request=?1 AND p.reference=?2 AND p.response_digest=?3 AND p.device=?4 AND p.bundle_generation=?5 AND p.kind=?6 AND p.acknowledged=1 AND p.revoked=0 AND p.consumed=0 AND p.not_before_ms<=?7 AND p.expires_at_ms>?7 AND i.revoked=0 AND i.retired=0 AND i.consumed=0 AND i.bundle_generation=p.bundle_generation)",params![usage.request(),usage.reference(),usage.response_digest(),contact,usage.bundle_generation() as i64,usage.kind() as i64,now],|r|r.get(0))?;
            if !valid || advances_account != (usage.kind() == OlmPrekeyKind::OneTime) {
                return conflict();
            }
            if usage.kind() == OlmPrekeyKind::OneTime {
                let changed = db.execute(
                    "UPDATE prekey_inventory SET consumed=1 WHERE reference=?1 AND consumed=0",
                    [usage.reference()],
                )?;
                if changed != 1 {
                    return conflict();
                }
                db.execute(
                    "UPDATE prekey_publication SET consumed=1 WHERE request=?1",
                    [usage.request()],
                )?;
            } else {
                let count: i64 = db.query_row(
                    "SELECT count(*) FROM prekey_session WHERE local=1 AND reference=?1",
                    [usage.reference()],
                    |r| r.get(0),
                )?;
                if count >= 16 {
                    return conflict();
                }
            }
        } else {
            let valid: bool = db.query_row("SELECT EXISTS(SELECT 1 FROM prekey_received r JOIN prekey_authority a ON a.device=r.device WHERE r.request=?1 AND r.device=?2 AND r.bundle_generation=?3 AND r.completed=1 AND r.expires_at_ms>?4 AND r.bundle_generation>=a.bundle_floor AND r.bundle_generation>a.revoked_through AND r.bundle_generation>a.retired_through)",params![usage.request(),contact,usage.bundle_generation() as i64,now],|r|r.get(0))?;
            let bytes = read_blob(
                db,
                "SELECT length(response),response FROM prekey_received WHERE request=?1",
                usage.request(),
            )?
            .ok_or(StorageError::Conflict)?;
            let digest: [u8; 32] = Sha256::digest(&bytes).into();
            if !valid || digest != usage.response_digest() || advances_account {
                return conflict();
            }
        }
        db.execute(
            "INSERT INTO prekey_session VALUES(?1,?2,?3,?4,?5,?6,?7)",
            params![
                session,
                usage.request(),
                i64::from(usage.local()),
                contact,
                usage.reference(),
                usage.kind() as i64,
                usage.response_digest()
            ],
        )?;
    } else {
        let valid: bool = db.query_row("SELECT EXISTS(SELECT 1 FROM prekey_session WHERE session=?1 AND request=?2 AND local=?3 AND contact=?4 AND reference=?5 AND response_digest=?6)",params![session,usage.request(),i64::from(usage.local()),contact,usage.reference(),usage.response_digest()],|r|r.get(0))?;
        if !valid || advances_account {
            return conflict();
        }
    }
    check_session_policy(db, session)
}
pub(crate) fn check_session_policy(db: &Connection, session: [u8; 32]) -> Result<(), StorageError> {
    let record: Option<(Vec<u8>, i64)> = db
        .query_row(
            "SELECT request,local FROM prekey_session WHERE session=?1",
            [session],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    let Some((request, local)) = record else {
        return Ok(());
    };
    let valid: bool = if local == 1 {
        db.query_row("SELECT EXISTS(SELECT 1 FROM prekey_publication p JOIN prekey_inventory i USING(reference) WHERE p.request=?1 AND p.revoked=0 AND i.revoked=0 AND i.retired=0 AND p.expires_at_ms>(SELECT floor_ms FROM prekey_clock WHERE singleton=1))",[request],|r|r.get(0))?
    } else {
        db.query_row("SELECT EXISTS(SELECT 1 FROM prekey_received r JOIN prekey_authority a ON a.device=r.device WHERE r.request=?1 AND r.bundle_generation>=a.bundle_floor AND r.bundle_generation>a.revoked_through AND r.bundle_generation>a.retired_through AND r.expires_at_ms>(SELECT floor_ms FROM prekey_clock WHERE singleton=1))",[request],|r|r.get(0))?
    };
    if !valid {
        return conflict();
    }
    Ok(())
}
// Local helper to keep the freeze branch's ownership explicit.
trait Pipe: Sized {
    fn pipe<T>(self, f: impl FnOnce(Self) -> T) -> T {
        f(self)
    }
}
impl<T> Pipe for T {}
