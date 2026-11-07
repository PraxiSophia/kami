//! Canonical receiver/receipt transactions; never a second session authority.
use crate::{
    session_state::{self, AccountRow, SessionRow},
    Acceptance, StorageError, TrustState,
};
use kami_core::MessageFacts;
use kami_e2ee::{AuthenticatedDelivery, AuthenticatedReceiptCommit, ReplayPermit};
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use sha2::{Digest, Sha256};

pub const MAX_CONTACT_MESSAGES: i64 = 1024;
pub const MAX_CONVERSATION_MESSAGES: i64 = 512;
pub const MAX_GAP_DISTANCE: u64 = 4096;
pub const MAX_UNRESOLVED_GAPS: i64 = 128;
#[derive(Debug)]
pub struct DurableDelivery {
    facts: MessageFacts,
    acceptance: Acceptance,
    receipt: Vec<u8>,
}
impl DurableDelivery {
    pub fn facts(&self) -> MessageFacts {
        self.facts
    }
    pub fn acceptance(&self) -> Acceptance {
        self.acceptance
    }
    pub fn receipt(&self) -> &[u8] {
        &self.receipt
    }
}
pub(crate) struct ReceiptTransition {
    account: AccountRow,
    previous: SessionRow,
    next: SessionRow,
    facts: MessageFacts,
    pub(crate) frame: Vec<u8>,
    target_digest: [u8; 32],
    incoming: bool,
}
impl ReceiptTransition {
    fn new(stage: AuthenticatedReceiptCommit) -> Result<Self, StorageError> {
        let target_digest = stage.target_frame_digest();
        let m = stage.into_material();
        let facts = m.facts();
        let frame = m.frame().to_vec();
        let incoming = m.incoming();
        let account_generation = m.account_generation();
        let generation = m.session_generation();
        let account_digest = m.account_digest();
        let previous_digest = m.previous_digest().ok_or(StorageError::Conflict)?;
        let digest = m.next_digest();
        let id = m.session_id();
        let contact = m.contact();
        let pin = m.pin().to_vec();
        let context = m.context().to_vec();
        let trust_generation = m.trust_generation();
        if m.advances_account() || generation < 2 {
            return Err(StorageError::Conflict);
        }
        let (account, next_account, previous, next) =
            m.into_snapshots().consume(|a, b, c, d| (a, b, c, d));
        if next_account.is_some() {
            return Err(StorageError::Conflict);
        }
        let row = |generation, state, digest| SessionRow {
            id,
            generation,
            state,
            digest,
            contact,
            pin: pin.clone(),
            context: context.clone(),
            trust_generation,
        };
        Ok(Self {
            account: AccountRow {
                generation: account_generation,
                digest: account_digest,
                state: account,
            },
            previous: row(
                generation - 1,
                previous.ok_or(StorageError::Conflict)?,
                previous_digest,
            ),
            next: row(generation, next, digest),
            facts,
            frame,
            target_digest,
            incoming,
        })
    }
}
fn artifact(
    db: &Connection,
    facts: MessageFacts,
    incoming: bool,
) -> Result<Option<Vec<u8>>, StorageError> {
    type ArtifactRow = (Vec<u8>, [u8; 32], i64, [u8; 32], i64, [u8; 32]);
    let row: Option<ArtifactRow> = db.query_row(
        "SELECT a.frame,a.artifact_id,a.session_generation,a.session_digest,s.generation,s.digest FROM delivery_artifact a JOIN e2ee_frame f USING(sender,id) JOIN e2ee_session s ON s.id=f.session JOIN e2ee_account ac ON ac.singleton=s.account WHERE a.sender=?1 AND a.id=?2 AND a.direction=?3 AND length(a.frame) BETWEEN 1 AND 65536 AND a.session_generation>f.session_generation AND (a.direction=1 OR a.session_generation=f.session_generation+1) AND f.account_generation<=ac.generation AND (f.account_generation!=ac.generation OR f.account_digest=ac.digest)",
        params![facts.sender().as_bytes(), facts.message_id().as_u128().to_be_bytes(), i64::from(incoming)],
        |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?))).optional()?;
    let Some((frame, digest, generation, state_digest, current_generation, current_digest)) = row
    else {
        return Ok(None);
    };
    if Sha256::digest(&frame).as_slice() != digest
        || generation > current_generation
        || (generation == current_generation && state_digest != current_digest)
    {
        return Err(StorageError::Conflict);
    }
    let outer = kami_protocol::decode_frame(&frame).map_err(|_| StorageError::Conflict)?;
    if outer.kind() != kami_protocol::MessageKind::DeviceDelivered
        || outer.sender() != facts.recipient()
        || outer.recipient() != facts.sender()
        || outer.conversation_id() != facts.conversation_id()
        || outer.receipt_target() != Some(facts.message_id())
    {
        return Err(StorageError::Conflict);
    }
    Ok(Some(frame))
}
fn usage(db: &Connection) -> Result<(i64, i64), StorageError> {
    Ok(db.query_row("SELECT count(*),coalesce(sum(length(f.frame)+coalesce(length(a.frame),0)+coalesce(length(h.payload),0)),0) FROM e2ee_frame f LEFT JOIN delivery_artifact a USING(sender,id) LEFT JOIN history h USING(sender,id)",[],|r|Ok((r.get(0)?,r.get(1)?)))?)
}
pub(crate) fn limits(
    db: &Connection,
    facts: MessageFacts,
    bytes: usize,
    quota: u64,
    count: u32,
) -> Result<(), StorageError> {
    let (total, used) = usage(db)?;
    let (contact, conversation, high): (i64,i64,i64) = db.query_row("SELECT count(*),coalesce(sum(conversation=?2),0),coalesce(max(CASE WHEN conversation=?2 THEN sequence END),0) FROM message WHERE sender=?1 AND direction=1", params![facts.sender().as_bytes(),facts.conversation_id().as_u128().to_be_bytes()],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?;
    let gaps: i64 = db.query_row(
        "SELECT count(*) FROM receiver_gap WHERE sender=?1",
        [facts.sender().as_bytes()],
        |r| r.get(0),
    )?;
    let gap_delta:i64=db.query_row(
        "SELECT (?3>1 AND NOT EXISTS(SELECT 1 FROM message WHERE sender=?1 AND conversation=?2 AND sequence=?3-1))-(SELECT count(*) FROM receiver_gap g JOIN message m USING(sender,id) WHERE m.sender=?1 AND m.conversation=?2 AND m.sequence-1=?3)",
        params![facts.sender().as_bytes(),facts.conversation_id().as_u128().to_be_bytes(),facts.sequence().value() as i64],|r|r.get(0))?;
    if total >= i64::from(count)
        || (used as u64).saturating_add(bytes as u64) > quota
        || contact >= MAX_CONTACT_MESSAGES
        || conversation >= MAX_CONVERSATION_MESSAGES
        || facts.sequence().value() > (high as u64).saturating_add(MAX_GAP_DISTANCE)
        || gaps + gap_delta > MAX_UNRESOLVED_GAPS
    {
        return Err(StorageError::Full);
    }
    Ok(())
}
fn apply(db: &Connection, r: &ReceiptTransition) -> Result<(), StorageError> {
    if session_state::load_account(db)?.as_ref() != Some(&r.account)
        || session_state::load_session(db, r.next.id)?.as_ref() != Some(&r.previous)
        || r.previous.generation.checked_add(1) != Some(r.next.generation)
        || r.previous.id != r.next.id
        || r.previous.context != r.next.context
        || r.previous.contact != r.next.contact
        || r.previous.pin != r.next.pin
        || r.previous.trust_generation != r.next.trust_generation
    {
        return Err(StorageError::Conflict);
    }
    let trust = crate::contact_trust::load(db, r.next.contact)?.ok_or(StorageError::Conflict)?;
    if trust.state() != TrustState::Verified
        || trust.generation() != r.next.trust_generation
        || trust.pinned() != Some(r.next.pin.as_slice())
    {
        return Err(StorageError::Conflict);
    }
    let target: Vec<u8> = db.query_row(
        "SELECT frame FROM e2ee_frame WHERE sender=?1 AND id=?2 AND session=?3 AND direction=?4",
        params![
            r.facts.sender().as_bytes(),
            r.facts.message_id().as_u128().to_be_bytes(),
            r.next.id,
            i64::from(!r.incoming)
        ],
        |row| row.get(0),
    )?;
    if Sha256::digest(&target).as_slice() != r.target_digest {
        return Err(StorageError::Conflict);
    }
    db.execute(
        "UPDATE e2ee_session SET generation=?2,digest=?3,state=?4 WHERE id=?1",
        params![
            r.next.id,
            r.next.generation as i64,
            r.next.digest,
            r.next.state.as_slice()
        ],
    )?;
    db.execute(
        "INSERT INTO delivery_artifact VALUES(?1,?2,?3,?4,?5,?6,?7)",
        params![
            r.facts.sender().as_bytes(),
            r.facts.message_id().as_u128().to_be_bytes(),
            i64::from(r.incoming),
            r.frame,
            Sha256::digest(&r.frame).as_slice(),
            r.next.generation as i64,
            r.next.digest
        ],
    )?;
    Ok(())
}
pub(crate) fn finish_inbound(
    db: &Connection,
    text: &SessionRow,
    facts: MessageFacts,
    r: ReceiptTransition,
) -> Result<(), StorageError> {
    if r.incoming || r.facts != facts || &r.previous != text {
        return Err(StorageError::Conflict);
    }
    apply(db, &r)?;
    db.execute("INSERT INTO receiver_gap SELECT sender,id,0 FROM message m WHERE sender=?1 AND id=?2 AND sequence>1 AND NOT EXISTS(SELECT 1 FROM message p WHERE p.sender=m.sender AND p.conversation=m.conversation AND p.sequence=m.sequence-1)",params![facts.sender().as_bytes(),facts.message_id().as_u128().to_be_bytes()])?;
    // Late predecessors resolve observations, including permanent gaps.
    db.execute("DELETE FROM receiver_gap WHERE (sender,id) IN (SELECT g.sender,g.id FROM receiver_gap g JOIN message m USING(sender,id) WHERE EXISTS(SELECT 1 FROM message p WHERE p.sender=m.sender AND p.conversation=m.conversation AND p.sequence=m.sequence-1))",[])?;
    Ok(())
}
pub(crate) fn commit(
    db: &mut Connection,
    stage: AuthenticatedDelivery,
    quota: u64,
    count: u32,
) -> Result<DurableDelivery, StorageError> {
    let facts = stage.facts();
    let (inbound, receipt) = stage.into_parts();
    let receipt = ReceiptTransition::new(receipt)?;
    let exists: bool = db.query_row(
        "SELECT EXISTS(SELECT 1 FROM e2ee_frame WHERE sender=?1 AND id=?2)",
        params![
            facts.sender().as_bytes(),
            facts.message_id().as_u128().to_be_bytes()
        ],
        |r| r.get(0),
    )?;
    session_state::commit_prepared_inner(db, inbound, quota, count, None, Some(receipt))?;
    let receipt = artifact(db, facts, false)?.ok_or(StorageError::Conflict)?;
    Ok(DurableDelivery {
        facts,
        receipt,
        acceptance: if exists {
            Acceptance::Duplicate
        } else {
            Acceptance::New
        },
    })
}
pub(crate) fn recover(
    db: &Connection,
    permit: ReplayPermit,
    frame: Vec<u8>,
) -> Result<Option<DurableDelivery>, StorageError> {
    let (remote, local, conversation, generation, pin) = permit.into_context();
    let outer = kami_protocol::decode_frame(&frame).map_err(|_| StorageError::InvalidInput)?;
    if outer.sender() != remote
        || outer.recipient() != local
        || outer.conversation_id() != conversation
        || outer.kind() != kami_protocol::MessageKind::Text
    {
        return Err(StorageError::Conflict);
    }
    let current =
        crate::contact_trust::load(db, *remote.as_bytes())?.ok_or(StorageError::Conflict)?;
    if current.state() != TrustState::Verified
        || current.generation() != generation
        || current.pinned() != Some(pin.as_slice())
    {
        return Err(StorageError::Conflict);
    }
    let row:Option<(Vec<u8>,Vec<u8>,Vec<u8>)>=db.query_row("SELECT m.facts,f.frame,s.pin FROM message m JOIN e2ee_frame f USING(sender,id) JOIN e2ee_session s ON s.id=f.session WHERE m.sender=?1 AND m.id=?2 AND m.direction=1 AND f.direction=1",params![remote.as_bytes(),outer.message_id().as_u128().to_be_bytes()],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
    let Some((encoded, stored_frame, stored_pin)) = row else {
        let legacy: bool = db.query_row(
            "SELECT EXISTS(SELECT 1 FROM message WHERE sender=?1 AND id=?2)",
            params![
                remote.as_bytes(),
                outer.message_id().as_u128().to_be_bytes()
            ],
            |r| r.get(0),
        )?;
        return if legacy {
            Err(StorageError::Conflict)
        } else {
            Ok(None)
        };
    };
    let facts = crate::message::decode(&encoded)?;
    if stored_frame != frame
        || stored_pin != pin
        || facts.sender() != remote
        || facts.recipient() != local
        || facts.conversation_id() != conversation
        || facts.message_id() != outer.message_id()
        || Some(facts.sequence()) != outer.sequence()
        || facts.content_binding().as_bytes() != outer.authentication_context()
        || crate::inbox::receipt(db, facts.sender(), facts.message_id())? != Some(facts)
    {
        return Err(StorageError::Conflict);
    }
    let receipt = artifact(db, facts, false)?.ok_or(StorageError::Conflict)?;
    Ok(Some(DurableDelivery {
        facts,
        receipt,
        acceptance: Acceptance::Duplicate,
    }))
}
pub(crate) fn recover_receipt(
    db: &Connection,
    permit: ReplayPermit,
    frame: Vec<u8>,
) -> Result<Option<MessageFacts>, StorageError> {
    let (remote, local, conversation, generation, pin) = permit.into_context();
    let outer = kami_protocol::decode_frame(&frame).map_err(|_| StorageError::InvalidInput)?;
    if outer.kind() != kami_protocol::MessageKind::DeviceDelivered
        || outer.sender() != remote
        || outer.recipient() != local
        || outer.conversation_id() != conversation
    {
        return Err(StorageError::Conflict);
    }
    let id = outer.receipt_target().ok_or(StorageError::Conflict)?;
    let row = crate::contact_trust::load(db, *remote.as_bytes())?.ok_or(StorageError::Conflict)?;
    if row.state() != TrustState::Verified
        || row.generation() != generation
        || row.pinned() != Some(pin.as_slice())
    {
        return Err(StorageError::Conflict);
    }
    let original = crate::sender_state::load(db, local, id)?;
    if original.facts().recipient() != remote || original.facts().conversation_id() != conversation
    {
        return Err(StorageError::Conflict);
    }
    let stored_pin:Vec<u8>=db.query_row("SELECT s.pin FROM e2ee_frame f JOIN e2ee_session s ON s.id=f.session WHERE f.sender=?1 AND f.id=?2",params![local.as_bytes(),id.as_u128().to_be_bytes()],|r|r.get(0))?;
    if stored_pin != pin {
        return Err(StorageError::Conflict);
    }
    match artifact(db, original.facts(), true)? {
        Some(stored) if stored == frame && original.state() == crate::SendState::Delivered => {
            Ok(Some(original.facts()))
        }
        Some(_) => Err(StorageError::Conflict),
        None => Ok(None),
    }
}
pub(crate) fn consume(
    db: &mut Connection,
    stage: AuthenticatedReceiptCommit,
    quota: u64,
) -> Result<Acceptance, StorageError> {
    let r = ReceiptTransition::new(stage)?;
    if !r.incoming {
        return Err(StorageError::Conflict);
    }
    let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let trust = crate::contact_trust::load(&tx, r.next.contact)?.ok_or(StorageError::Conflict)?;
    if trust.state() != TrustState::Verified
        || trust.generation() != r.next.trust_generation
        || trust.pinned() != Some(r.next.pin.as_slice())
    {
        return Err(StorageError::Conflict);
    }
    let f = r.facts;
    let original = crate::sender_state::load(&tx, f.sender(), f.message_id())?;
    if original.facts() != f || Sha256::digest(original.frame()).as_slice() != r.target_digest {
        return Err(StorageError::Conflict);
    }
    if let Some(stored) = artifact(&tx, f, true)? {
        if stored != r.frame || original.state() != crate::SendState::Delivered {
            return Err(StorageError::Conflict);
        }
        tx.commit()?;
        return Ok(Acceptance::Duplicate);
    }
    if original.state() == crate::SendState::Delivered {
        return Err(StorageError::Conflict);
    }
    let (_, used) = usage(&tx)?;
    if (used as u64).saturating_add(r.frame.len() as u64) > quota {
        return Err(StorageError::Full);
    }
    apply(&tx, &r)?;
    tx.execute(
        "INSERT INTO receipt VALUES(?1,?2,?3)",
        params![
            f.sender().as_bytes(),
            f.message_id().as_u128().to_be_bytes(),
            crate::message::encode(f)
        ],
    )?;
    tx.execute(
        "UPDATE message SET state=1 WHERE sender=?1 AND id=?2 AND direction=0",
        params![
            f.sender().as_bytes(),
            f.message_id().as_u128().to_be_bytes()
        ],
    )?;
    // Terminal attempt lane; the canonical message owns Delivered versus Canceled.
    tx.execute(
        "UPDATE sender_attempt SET state=3,token=NULL,deadline=0 WHERE sender=?1 AND id=?2",
        params![
            f.sender().as_bytes(),
            f.message_id().as_u128().to_be_bytes()
        ],
    )?;
    tx.commit()?;
    Ok(Acceptance::New)
}
