//! Runtime-neutral sender state; only the existing SQLCipher worker calls SQL here.
use crate::StorageError;
use hmac::{Hmac, KeyInit, Mac};
use kami_core::{ConversationId, DeviceId, MessageId, SenderSequence};
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use sha2::Sha256;
use zeroize::Zeroizing;

pub const PREPARATION_LIFETIME_MS: i64 = 60_000;
/// Caller must retain the stable ID and immutable submission fields across restart.
/// Text is held only in zeroizing process memory, never in reservation rows.
#[derive(Clone)]
pub struct SendRequest {
    pub message: MessageId,
    pub conversation: ConversationId,
    pub sender: DeviceId,
    pub recipient: DeviceId,
    pub created_at_ms: u64,
    text: Zeroizing<String>,
}
impl std::fmt::Debug for SendRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SendRequest")
            .field("message", &self.message)
            .finish_non_exhaustive()
    }
}
impl SendRequest {
    pub fn new(
        message: MessageId,
        conversation: ConversationId,
        sender: DeviceId,
        recipient: DeviceId,
        created_at_ms: u64,
        text: String,
    ) -> Result<Self, StorageError> {
        let text = Zeroizing::new(text);
        if text.is_empty()
            || text.len() > 65_536
            || created_at_ms > i64::MAX as u64
            || sender == recipient
        {
            return Err(StorageError::InvalidInput);
        }
        Ok(Self {
            message,
            conversation,
            sender,
            recipient,
            created_at_ms,
            text,
        })
    }
    pub fn text(&self) -> &str {
        &self.text
    }
}
/// Opaque durable reservation, not an authenticated-message capability.
#[derive(Clone, PartialEq, Eq)]
pub struct PreparedSend {
    pub(crate) sender: DeviceId,
    pub(crate) id: MessageId,
    pub(crate) token: [u8; 32],
    sequence: SenderSequence,
}
impl PreparedSend {
    pub fn sequence(&self) -> SenderSequence {
        self.sequence
    }
}
impl std::fmt::Debug for PreparedSend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PreparedSend")
            .field("id", &self.id)
            .field("sequence", &self.sequence)
            .finish_non_exhaustive()
    }
}
#[derive(Debug, PartialEq, Eq)]
pub enum Preparation {
    Prepared(PreparedSend),
    Committed(QueuedSend),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QueuedSend {
    facts: kami_core::MessageFacts,
    frame: Vec<u8>,
    attempts: u32,
    state: SendState,
    eligible: i64,
    failure: Option<kami_core::AttemptFailure>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SendState {
    Queued,
    InFlight,
    Exhausted,
    Canceled,
    HeldIdentity,
    Delivered,
}
impl TryFrom<i64> for SendState {
    type Error = StorageError;
    fn try_from(value: i64) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::Queued),
            1 => Ok(Self::InFlight),
            2 => Ok(Self::Exhausted),
            3 => Ok(Self::Canceled),
            4 => Ok(Self::HeldIdentity),
            5 => Ok(Self::Delivered),
            _ => Err(StorageError::WrongKeyOrCorrupt),
        }
    }
}
impl QueuedSend {
    pub fn next_eligible_ms(&self) -> i64 {
        self.eligible
    }
    pub fn last_failure(&self) -> Option<kami_core::AttemptFailure> {
        self.failure
    }
    pub fn state(&self) -> SendState {
        self.state
    }
    pub fn facts(&self) -> kami_core::MessageFacts {
        self.facts
    }
    pub fn frame(&self) -> &[u8] {
        &self.frame
    }
    pub fn attempts(&self) -> u32 {
        self.attempts
    }
}
pub(crate) fn load(
    db: &Connection,
    sender: DeviceId,
    id: MessageId,
) -> Result<QueuedSend, StorageError> {
    let mut statement=db.prepare("SELECT length(m.facts),length(f.frame),m.facts,f.frame,a.attempts,CASE WHEN m.state=1 THEN 5 ELSE a.state END,m.payload=f.frame,p.conversation,p.recipient,p.sequence,a.token,a.deadline,a.eligible,a.failure FROM sender_prepare p JOIN message m USING(sender,id) JOIN e2ee_frame f USING(sender,id) JOIN sender_attempt a USING(sender,id) WHERE p.sender=?1 AND p.id=?2 AND p.state=1 AND m.direction=0 AND f.direction=0")?;
    let mut rows = statement.query(params![sender.as_bytes(), id.as_u128().to_be_bytes()])?;
    let row = rows.next()?.ok_or(StorageError::WrongKeyOrCorrupt)?;
    if row.get::<_, i64>(0)? != 136
        || !(1..=65536).contains(&row.get::<_, i64>(1)?)
        || row.get::<_, Option<bool>>(6)? != Some(true)
    {
        return Err(StorageError::WrongKeyOrCorrupt);
    }
    let facts = crate::message::decode(&row.get::<_, Vec<u8>>(2)?)?;
    let attempts = row.get::<_, u32>(4)?;
    let state = SendState::try_from(row.get::<_, i64>(5)?)?;
    let token = row.get::<_, Option<[u8; 32]>>(10)?;
    let deadline = row.get::<_, i64>(11)?;
    if facts.sender() != sender
        || facts.message_id() != id
        || facts.conversation_id().as_u128().to_be_bytes() != row.get::<_, [u8; 16]>(7)?
        || facts.recipient().as_bytes() != &row.get::<_, [u8; 32]>(8)?
        || facts.sequence().value() as i64 != row.get::<_, i64>(9)?
        || attempts > MAX_ATTEMPTS
        || (state == SendState::InFlight && (token.is_none() || deadline <= 0 || attempts == 0))
        || (state != SendState::InFlight && (token.is_some() || deadline != 0))
        || row.get::<_, i64>(12)? < 0
    {
        return Err(StorageError::WrongKeyOrCorrupt);
    }
    Ok(QueuedSend {
        facts,
        frame: row.get(3)?,
        attempts,
        state,
        eligible: row.get(12)?,
        failure: match row.get::<_, Option<i64>>(13)? {
            None => None,
            Some(0) => Some(kami_core::AttemptFailure::AckLost),
            Some(1) => Some(kami_core::AttemptFailure::ConnectionLost),
            Some(2) => Some(kami_core::AttemptFailure::Unavailable),
            _ => return Err(StorageError::WrongKeyOrCorrupt),
        },
    })
}
pub(crate) fn recover(
    db: &Connection,
    after: Option<(ConversationId, DeviceId, SenderSequence)>,
    limit: usize,
) -> Result<Vec<QueuedSend>, StorageError> {
    if limit == 0 || limit > 1024 || after.is_some_and(|(_, _, s)| s.value() > i64::MAX as u64) {
        return Err(StorageError::InvalidInput);
    }
    let cursor =
        after.map(|(c, s, n)| (c.as_u128().to_be_bytes(), *s.as_bytes(), n.value() as i64));
    let mut statement=db.prepare("SELECT a.sender,a.id FROM sender_attempt a JOIN sender_prepare p USING(sender,id) WHERE a.state!=3 AND (?1 IS NULL OR (p.conversation,a.sender,p.sequence)>(?1,?2,?3)) ORDER BY p.conversation,a.sender,p.sequence LIMIT ?4")?;
    let keys = statement
        .query_map(
            params![
                cursor.as_ref().map(|c| c.0),
                cursor.as_ref().map(|c| c.1),
                cursor.as_ref().map(|c| c.2),
                limit as i64
            ],
            |r| Ok((r.get::<_, [u8; 32]>(0)?, r.get::<_, [u8; 16]>(1)?)),
        )?
        .collect::<Result<Vec<_>, _>>()?;
    keys.into_iter()
        .map(|(sender, id)| {
            load(
                db,
                DeviceId::from_bytes(sender),
                MessageId::from_u128(u128::from_be_bytes(id)),
            )
        })
        .collect()
}
pub(crate) fn consume(
    tx: &Connection,
    p: &PreparedSend,
    f: kami_core::MessageFacts,
    now: i64,
) -> Result<(), StorageError> {
    if now < 0 || p.sender != f.sender() || p.id != f.message_id() || p.sequence != f.sequence() {
        return Err(StorageError::Conflict);
    }
    let changed=tx.execute("UPDATE sender_prepare SET state=1 WHERE sender=?1 AND id=?2 AND token=?3 AND conversation=?4 AND recipient=?5 AND sequence=?6 AND state=0 AND expires>?7",params![p.sender.as_bytes(),p.id.as_u128().to_be_bytes(),p.token,f.conversation_id().as_u128().to_be_bytes(),f.recipient().as_bytes(),f.sequence().value() as i64,now])?;
    if changed != 1 {
        return Err(StorageError::Conflict);
    }
    Ok(())
}
pub(crate) fn check_committed(
    db: &Connection,
    p: &PreparedSend,
    f: kami_core::MessageFacts,
) -> Result<(), StorageError> {
    let matched:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM sender_prepare WHERE sender=?1 AND id=?2 AND token=?3 AND state=1)",params![p.sender.as_bytes(),p.id.as_u128().to_be_bytes(),p.token],|r|r.get(0))?;
    if !matched || load(db, p.sender, p.id)?.facts != f {
        return Err(StorageError::Conflict);
    }
    Ok(())
}

pub(crate) fn hold_contact(db: &Connection, contact: [u8; 32]) -> Result<(), StorageError> {
    db.execute("UPDATE sender_attempt SET state=4,token=NULL,deadline=0 WHERE state IN (0,1,2) AND (sender,id) IN (SELECT f.sender,f.id FROM e2ee_frame f JOIN e2ee_session s ON s.id=f.session LEFT JOIN contact_trust c ON c.contact=s.contact WHERE s.contact=?1 AND (c.contact IS NULL OR c.state!=1 OR c.generation!=sender_attempt.trust_generation OR c.pinned!=s.pin))",[contact])?;
    Ok(())
}
pub(crate) fn hold(
    db: &mut Connection,
    sender: DeviceId,
    id: MessageId,
) -> Result<(), StorageError> {
    let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let message = load(&tx, sender, id)?;
    if matches!(message.state, SendState::Canceled | SendState::Delivered) {
        return Err(StorageError::Conflict);
    }
    tx.execute("UPDATE sender_attempt SET state=4,token=NULL,deadline=0,eligible=0 WHERE sender=?1 AND id=?2",params![sender.as_bytes(),id.as_u128().to_be_bytes()])?;
    tx.commit()?;
    Ok(())
}
pub(crate) fn resume(
    db: &mut Connection,
    sender: DeviceId,
    id: MessageId,
    generation: u64,
    now: i64,
) -> Result<(), StorageError> {
    if now < 0 || generation == 0 || generation > i64::MAX as u64 {
        return Err(StorageError::InvalidInput);
    }
    let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let message = load(&tx, sender, id)?;
    if message.state != SendState::HeldIdentity {
        return Err(StorageError::Conflict);
    }
    let valid:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM e2ee_frame f JOIN e2ee_session s ON s.id=f.session JOIN contact_trust c ON c.contact=s.contact WHERE f.sender=?1 AND f.id=?2 AND s.contact=?3 AND c.state=1 AND c.generation=?4 AND c.pinned=s.pin)",params![sender.as_bytes(),id.as_u128().to_be_bytes(),message.facts.recipient().as_bytes(),generation as i64],|r|r.get(0))?;
    if !valid {
        return Err(StorageError::Conflict);
    }
    tx.execute("UPDATE sender_attempt SET state=?3,eligible=?4,trust_generation=?5 WHERE sender=?1 AND id=?2 AND state=4",params![sender.as_bytes(),id.as_u128().to_be_bytes(),if message.attempts>=MAX_ATTEMPTS {2} else {0},now,generation as i64])?;
    tx.commit()?;
    Ok(())
}
pub(crate) fn cancel(
    db: &mut Connection,
    sender: DeviceId,
    id: MessageId,
) -> Result<(), StorageError> {
    let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
    if load(&tx, sender, id)?.state == SendState::Delivered {
        return Err(StorageError::Conflict);
    }
    tx.execute(
        "UPDATE sender_attempt SET state=3,token=NULL,deadline=0 WHERE sender=?1 AND id=?2",
        params![sender.as_bytes(), id.as_u128().to_be_bytes()],
    )?;
    // Preserve payload and authenticated replay evidence; history deletion is separate.
    tx.execute(
        "UPDATE message SET state=2 WHERE sender=?1 AND id=?2 AND direction=0",
        params![sender.as_bytes(), id.as_u128().to_be_bytes()],
    )?;
    tx.commit()?;
    Ok(())
}
pub const ATTEMPT_LEASE_MS: i64 = 30_000;
pub const MAX_ATTEMPTS: u32 = 8;
/// Pure shared policy, also enforced atomically when the worker records failure.
pub fn backoff_ms(attempt: u32) -> Option<i64> {
    if attempt == 0 {
        return None;
    }
    Some(
        1_000i64
            .checked_mul(1i64.checked_shl(attempt - 1).unwrap_or(i64::MAX))
            .unwrap_or(i64::MAX)
            .min(300_000),
    )
}
#[derive(Clone, PartialEq, Eq)]
pub struct SendLease {
    message: QueuedSend,
    token: [u8; 32],
    expires: i64,
}
impl SendLease {
    pub fn message(&self) -> &QueuedSend {
        &self.message
    }
    pub fn expires_at_ms(&self) -> i64 {
        self.expires
    }
}
impl std::fmt::Debug for SendLease {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SendLease")
            .field("facts", &self.message.facts)
            .field("expires", &self.expires)
            .finish_non_exhaustive()
    }
}
pub(crate) fn claim(
    db: &mut Connection,
    sender: DeviceId,
    id: MessageId,
    now: i64,
) -> Result<Option<SendLease>, StorageError> {
    let expires = now
        .checked_add(ATTEMPT_LEASE_MS)
        .filter(|_| now >= 0)
        .ok_or(StorageError::InvalidInput)?;
    let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let original = load(&tx, sender, id)?;
    hold_contact(&tx, *original.facts.recipient().as_bytes())?;
    let mut message = load(&tx, sender, id)?;
    let (state, deadline, eligible): (i64, i64, i64) = tx.query_row(
        "SELECT state,deadline,eligible FROM sender_attempt WHERE sender=?1 AND id=?2",
        params![sender.as_bytes(), id.as_u128().to_be_bytes()],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )?;
    if (state != 0 && state != 1)
        || (state == 1 && deadline > now)
        || (state == 0 && eligible > now)
    {
        tx.commit()?;
        return Ok(None);
    }
    let token = random_token()?;
    if state == 0 {
        message.attempts = message
            .attempts
            .checked_add(1)
            .filter(|n| *n <= MAX_ATTEMPTS)
            .ok_or(StorageError::Conflict)?;
    }
    tx.execute("UPDATE sender_attempt SET state=1,token=?3,deadline=?4,attempts=?5 WHERE sender=?1 AND id=?2",params![sender.as_bytes(),id.as_u128().to_be_bytes(),token,expires,i64::from(message.attempts)])?;
    message.state = SendState::InFlight;
    tx.commit()?;
    Ok(Some(SendLease {
        message,
        token,
        expires,
    }))
}
pub(crate) fn fail(
    db: &mut Connection,
    lease: SendLease,
    reason: kami_core::AttemptFailure,
    now: i64,
) -> Result<(), StorageError> {
    let attempts = lease.message.attempts;
    let delay = backoff_ms(attempts).ok_or(StorageError::WrongKeyOrCorrupt)?;
    let eligible = now
        .checked_add(delay)
        .filter(|_| now >= 0)
        .ok_or(StorageError::InvalidInput)?;
    let f = lease.message.facts;
    let failure = match reason {
        kami_core::AttemptFailure::AckLost => 0,
        kami_core::AttemptFailure::ConnectionLost => 1,
        kami_core::AttemptFailure::Unavailable => 2,
    };
    let changed=db.execute("UPDATE sender_attempt SET state=?4,token=NULL,deadline=0,eligible=?5,failure=?7 WHERE sender=?1 AND id=?2 AND state=1 AND token=?3 AND deadline>?6",params![f.sender().as_bytes(),f.message_id().as_u128().to_be_bytes(),lease.token,if attempts>=MAX_ATTEMPTS {2} else {0},eligible,now,failure])?;
    if changed != 1 {
        return Err(StorageError::Conflict);
    }
    Ok(())
}
fn authenticator(db: &Connection, r: &SendRequest) -> Result<Hmac<Sha256>, StorageError> {
    let key = Zeroizing::new(db.query_row(
        "SELECT key FROM sender_binding_key WHERE singleton=1",
        [],
        |row| row.get::<_, Vec<u8>>(0),
    )?);
    if key.len() != 32 {
        return Err(StorageError::WrongKeyOrCorrupt);
    }
    let mut mac = Hmac::<Sha256>::new_from_slice(key.as_slice())
        .map_err(|_| StorageError::WrongKeyOrCorrupt)?;
    mac.update(b"kami-sender-request-v1\0");
    mac.update(&r.message.as_u128().to_be_bytes());
    mac.update(&r.conversation.as_u128().to_be_bytes());
    mac.update(r.sender.as_bytes());
    mac.update(r.recipient.as_bytes());
    mac.update(&r.created_at_ms.to_be_bytes());
    mac.update(&(r.text.len() as u64).to_be_bytes());
    mac.update(r.text.as_bytes());
    Ok(mac)
}
fn random_token() -> Result<[u8; 32], StorageError> {
    let mut bytes = [0; 32];
    getrandom::fill(&mut bytes).map_err(|_| StorageError::Io)?;
    Ok(bytes)
}
pub(crate) fn cleanup(db: &mut Connection, now: i64) -> Result<usize, StorageError> {
    if now < 0 {
        return Err(StorageError::InvalidInput);
    }
    Ok(db.execute(
        "UPDATE sender_prepare SET state=2 WHERE state=0 AND expires<=?1",
        [now],
    )?)
}
pub(crate) fn prepare(
    db: &mut Connection,
    r: SendRequest,
    now: i64,
    quota: u32,
) -> Result<Preparation, StorageError> {
    if r.sender == r.recipient || r.created_at_ms > i64::MAX as u64 {
        return Err(StorageError::InvalidInput);
    }
    let expires = now
        .checked_add(PREPARATION_LIFETIME_MS)
        .filter(|_| now >= 0)
        .ok_or(StorageError::InvalidInput)?;
    let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let mac = authenticator(&tx, &r)?;
    let existing = tx
        .query_row(
            "SELECT request_mac,sequence,token,state FROM sender_prepare WHERE sender=?1 AND id=?2",
            params![r.sender.as_bytes(), r.message.as_u128().to_be_bytes()],
            |row| {
                Ok((
                    row.get::<_, Vec<u8>>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, [u8; 32]>(2)?,
                    row.get::<_, i64>(3)?,
                ))
            },
        )
        .optional()?;
    let occupied_id: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM message WHERE sender=?1 AND id=?2)",
        params![r.sender.as_bytes(), r.message.as_u128().to_be_bytes()],
        |row| row.get(0),
    )?;
    let known = existing.is_some();
    if let Some((binding, sequence, token, state)) = existing {
        mac.clone()
            .verify_slice(&binding)
            .map_err(|_| StorageError::Conflict)?;
        if state == 1 {
            return Ok(Preparation::Committed(load(&tx, r.sender, r.message)?));
        }
        if occupied_id || sequence <= 0 {
            return Err(StorageError::WrongKeyOrCorrupt);
        }
        if state == 0 {
            return Ok(Preparation::Prepared(PreparedSend {
                sender: r.sender,
                id: r.message,
                token,
                sequence: SenderSequence::new(sequence as u64),
            }));
        }
        if state != 2 {
            return Err(StorageError::WrongKeyOrCorrupt);
        }
    }
    if occupied_id {
        return Err(StorageError::Conflict);
    }
    let occupied:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM sender_prepare WHERE conversation=?1 AND sender=?2 AND state=0)",params![r.conversation.as_u128().to_be_bytes(),r.sender.as_bytes()],|row|row.get(0))?;
    if occupied {
        return Err(StorageError::Busy);
    }
    let count: i64 = tx.query_row("SELECT count(*) FROM sender_prepare", [], |row| row.get(0))?;
    if !known && count >= i64::from(quota) {
        return Err(StorageError::Full);
    }
    let sequence = crate::transactions::next_sequence(&tx, r.conversation, r.sender)?;
    let token = random_token()?;
    let binding = mac.finalize().into_bytes();
    tx.execute("INSERT INTO sender_prepare VALUES(?1,?2,?3,?4,?5,?6,?7,?8,0) ON CONFLICT(sender,id) DO UPDATE SET sequence=excluded.sequence,token=excluded.token,expires=excluded.expires,state=0",params![r.sender.as_bytes(),r.message.as_u128().to_be_bytes(),r.conversation.as_u128().to_be_bytes(),r.recipient.as_bytes(),binding.as_slice(),sequence.value() as i64,token,expires])?;
    tx.commit()?;
    Ok(Preparation::Prepared(PreparedSend {
        sender: r.sender,
        id: r.message,
        token,
        sequence,
    }))
}
