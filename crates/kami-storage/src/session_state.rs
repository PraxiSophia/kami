//! Private WP3 rows and WP2-B typed authenticated-stage consumer.
//! Neutral constructors are never public authentication or trusted-write APIs.
//! Exact old bytes are compared (stronger than a caller-supplied digest alone).
//! No DB value here is an independent non-rollbackable anchor.
use crate::{Acceptance, StorageError, StoredMessage, TrustState};
use kami_core::MessageFacts;
use rusqlite::{params, Connection, TransactionBehavior};
use zeroize::Zeroizing;

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct AccountRow {
    pub generation: u64,
    pub digest: [u8; 32],
    pub state: Zeroizing<Vec<u8>>,
}
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct SessionRow {
    pub id: [u8; 32],
    pub generation: u64,
    pub contact: [u8; 32],
    pub trust_generation: u64,
    pub pin: Vec<u8>,
    pub context: Vec<u8>,
    pub digest: [u8; 32],
    pub state: Zeroizing<Vec<u8>>,
}
impl std::fmt::Debug for AccountRow {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("AccountRow([REDACTED])")
    }
}
impl std::fmt::Debug for SessionRow {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SessionRow([REDACTED])")
    }
}
fn generation(n: u64) -> bool {
    n > 0 && n <= i64::MAX as u64
}
impl AccountRow {
    pub fn new(
        generation_value: u64,
        bytes: Vec<u8>,
        digest: [u8; 32],
    ) -> Result<Self, StorageError> {
        let state = Zeroizing::new(bytes);
        if !generation(generation_value) || state.is_empty() || state.len() > 1_048_576 {
            return Err(StorageError::InvalidInput);
        }
        Ok(Self {
            generation: generation_value,
            state,
            digest,
        })
    }
}
impl SessionRow {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: [u8; 32],
        n: u64,
        contact: [u8; 32],
        trust_generation: u64,
        pin: Vec<u8>,
        context: Vec<u8>,
        state: Vec<u8>,
        digest: [u8; 32],
    ) -> Result<Self, StorageError> {
        let account = AccountRow::new(n, state, digest)?;
        if !generation(trust_generation)
            || pin.is_empty()
            || pin.len() > 512
            || context.is_empty()
            || context.len() > 2048
        {
            return Err(StorageError::InvalidInput);
        }
        Ok(Self {
            id,
            generation: n,
            contact,
            trust_generation,
            pin,
            context,
            state: account.state,
            digest,
        })
    }
}
pub(crate) fn load_account(db: &Connection) -> Result<Option<AccountRow>, StorageError> {
    let mut stmt = db.prepare(
        "SELECT version,generation,length(state),state,digest FROM e2ee_account WHERE singleton=1",
    )?;
    let mut rows = stmt.query([])?;
    let Some(r) = rows.next()? else {
        return Ok(None);
    };
    if r.get::<_, i64>(0)? != 1 || !(1..=1_048_576).contains(&r.get::<_, i64>(2)?) {
        return Err(StorageError::WrongKeyOrCorrupt);
    }
    AccountRow::new(r.get::<_, i64>(1)? as u64, r.get(3)?, r.get(4)?)
        .map(Some)
        .map_err(|_| StorageError::WrongKeyOrCorrupt)
}
pub(crate) fn load_session(
    db: &Connection,
    id: [u8; 32],
) -> Result<Option<SessionRow>, StorageError> {
    let mut stmt = db.prepare("SELECT version,generation,contact,trust_generation,length(pin),length(context),length(state),pin,context,state,digest FROM e2ee_session WHERE id=?1")?;
    let mut rows = stmt.query([id])?;
    let Some(r) = rows.next()? else {
        return Ok(None);
    };
    if r.get::<_, i64>(0)? != 1
        || !(1..=512).contains(&r.get::<_, i64>(4)?)
        || !(1..=2048).contains(&r.get::<_, i64>(5)?)
        || !(1..=1_048_576).contains(&r.get::<_, i64>(6)?)
    {
        return Err(StorageError::WrongKeyOrCorrupt);
    }
    SessionRow::new(
        id,
        r.get::<_, i64>(1)? as u64,
        r.get(2)?,
        r.get::<_, i64>(3)? as u64,
        r.get(7)?,
        r.get(8)?,
        r.get(9)?,
        r.get(10)?,
    )
    .map(Some)
    .map_err(|_| StorageError::WrongKeyOrCorrupt)
}
fn check_trust(db: &Connection, session: &SessionRow) -> Result<(), StorageError> {
    let row = crate::contact_trust::load(db, session.contact)?.ok_or(StorageError::Conflict)?;
    if row.state() != TrustState::Verified
        || row.generation() != session.trust_generation
        || row.pinned() != Some(session.pin.as_slice())
    {
        return Err(StorageError::Conflict);
    }
    Ok(())
}
fn put_account(
    db: &Connection,
    previous: Option<&AccountRow>,
    next: &AccountRow,
) -> Result<(), StorageError> {
    AccountRow::new(next.generation, next.state.to_vec(), next.digest)?;
    if load_account(db)?.as_ref() != previous
        || previous.map_or(Some(1), |r| r.generation.checked_add(1)) != Some(next.generation)
    {
        return Err(StorageError::Conflict);
    }
    db.execute("INSERT INTO e2ee_account VALUES(1,1,?1,?2,?3) ON CONFLICT(singleton) DO UPDATE SET generation=excluded.generation,digest=excluded.digest,state=excluded.state", params![next.generation as i64, next.digest, next.state.as_slice()])?;
    Ok(())
}
pub(crate) fn commit_account(
    db: &mut Connection,
    previous: Option<AccountRow>,
    next: AccountRow,
) -> Result<(), StorageError> {
    let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
    put_account(&tx, previous.as_ref(), &next)?;
    tx.commit()?;
    Ok(())
}
/// First inbound prekey consumption uses next_account in this SAME transaction.
/// Receipt ratchet/Delivered operations deliberately have no command here.
pub(crate) struct Commit {
    pub account: AccountRow,
    pub next_account: Option<AccountRow>,
    pub previous: Option<SessionRow>,
    pub next: SessionRow,
    pub message: StoredMessage,
    pub frame: Vec<u8>,
    pub received_at: Option<i64>,
}
pub(crate) fn commit(
    db: &mut Connection,
    c: Commit,
    quota: u64,
    count: u32,
) -> Result<Acceptance, StorageError> {
    commit_inner(db, c, quota, count, false)
}
fn commit_inner(
    db: &mut Connection,
    c: Commit,
    quota: u64,
    count: u32,
    initialize: bool,
) -> Result<Acceptance, StorageError> {
    if c.frame.is_empty()
        || c.frame.len() > 65536
        || (c.received_at.is_none() && c.message.payload() != c.frame)
    {
        return Err(StorageError::InvalidInput);
    }
    let n = &c.next;
    SessionRow::new(
        n.id,
        n.generation,
        n.contact,
        n.trust_generation,
        n.pin.clone(),
        n.context.clone(),
        n.state.to_vec(),
        n.digest,
    )?;
    let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
    check_trust(&tx, n)?;
    // Bootstrap only from the opaque authenticated command, inside this transaction.
    if initialize && load_account(&tx)?.is_none() {
        let existing: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM e2ee_session) OR EXISTS(SELECT 1 FROM e2ee_frame)",
            [],
            |r| r.get(0),
        )?;
        if c.previous.is_some() || n.generation != 1 || existing {
            return Err(StorageError::Conflict);
        }
        put_account(&tx, None, &c.account)?;
    }
    #[cfg(test)]
    test_pause("before");
    if load_account(&tx)?.as_ref() != Some(&c.account)
        || load_session(&tx, n.id)?.as_ref() != c.previous.as_ref()
        || c.previous
            .as_ref()
            .map_or(Some(1), |r| r.generation.checked_add(1))
            != Some(n.generation)
    {
        return Err(StorageError::Conflict);
    }
    if c.previous.as_ref().is_some_and(|p| {
        p.id != n.id
            || p.contact != n.contact
            || p.context != n.context
            || p.pin != n.pin
            || p.trust_generation != n.trust_generation
    }) {
        return Err(StorageError::Conflict);
    }
    let f = c.message.facts();
    // Only reconcile may reuse an existing logical ID; never adopt legacy rows.
    let existing: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM message WHERE sender=?1 AND id=?2)",
        params![
            f.sender().as_bytes(),
            f.message_id().as_u128().to_be_bytes()
        ],
        |r| r.get(0),
    )?;
    if existing {
        return Err(StorageError::Conflict);
    }
    let remote = if c.received_at.is_some() {
        f.sender()
    } else {
        f.recipient()
    };
    if remote.as_bytes() != &n.contact {
        return Err(StorageError::Conflict);
    }
    let account_generation = if let Some(ref next) = c.next_account {
        put_account(&tx, Some(&c.account), next)?;
        next.generation
    } else {
        c.account.generation
    };
    #[cfg(test)]
    test_pause("account");
    tx.execute("INSERT INTO e2ee_session(id,version,generation,contact,trust_generation,pin,context,digest,state) VALUES(?1,1,?2,?3,?4,?5,?6,?7,?8) ON CONFLICT(id) DO UPDATE SET generation=excluded.generation,digest=excluded.digest,state=excluded.state",params![n.id,n.generation as i64,n.contact,n.trust_generation as i64,n.pin,n.context,n.digest,n.state.as_slice()])?;
    #[cfg(test)]
    test_pause("session");
    let result = if let Some(at) = c.received_at {
        crate::inbox::accept_in(&tx, c.message, at)?
    } else {
        crate::transactions::enqueue_in(&tx, c.message, quota, count)?;
        Acceptance::New
    };
    // Existing legacy messages cannot be promoted to authenticated replay evidence.
    if result != Acceptance::New {
        return Err(StorageError::Conflict);
    }
    #[cfg(test)]
    test_pause("message");
    let account_digest = c.next_account.as_ref().unwrap_or(&c.account).digest;
    tx.execute(
        "INSERT INTO e2ee_frame VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",
        params![
            f.sender().as_bytes(),
            f.message_id().as_u128().to_be_bytes(),
            n.id,
            i64::from(c.received_at.is_some()),
            n.generation as i64,
            account_generation as i64,
            n.digest,
            account_digest,
            c.frame
        ],
    )?;
    #[cfg(test)]
    test_pause("frame");
    tx.commit()?;
    #[cfg(test)]
    test_pause("committed");
    Ok(result)
}
/// Consume only authenticated stage evidence; neutral rows remain private.
/// Returning bytes confirms persistence/reconciliation, never authorizes an ACK.
pub(crate) fn commit_authenticated(
    db: &mut Connection,
    stage: kami_e2ee::AuthenticatedCommit,
    quota: u64,
    count: u32,
) -> Result<Vec<u8>, StorageError> {
    let m = stage.into_material();
    let id = m.session_id();
    let contact = m.contact();
    let trust_generation = m.trust_generation();
    let pin = m.pin().to_vec();
    let context = m.context().to_vec();
    let facts = m.facts();
    let incoming = m.incoming();
    let frame = m.frame().to_vec();
    let message = StoredMessage::new(facts, m.payload().to_vec())?;
    let received_at = incoming.then_some(m.received_at_ms());
    let account_generation = m.account_generation();
    let session_generation = m.session_generation();
    let account_digest = m.account_digest();
    let next_account_digest = m.next_account_digest();
    let previous_digest = m.previous_digest();
    let next_digest = m.next_digest();
    let (account, next_account, previous, next) =
        m.into_snapshots().consume(|a, b, c, d| (a, b, c, d));
    let row = |generation, state, digest| SessionRow {
        id,
        generation,
        contact,
        trust_generation,
        pin: pin.clone(),
        context: context.clone(),
        digest,
        state,
    };
    let next = row(session_generation, next, next_digest);
    // An exact retransmission is reconciled without applying an old ratchet again.
    let exists: bool = db.query_row(
        "SELECT EXISTS(SELECT 1 FROM e2ee_frame WHERE sender=?1 AND id=?2)",
        params![
            facts.sender().as_bytes(),
            facts.message_id().as_u128().to_be_bytes()
        ],
        |r| r.get(0),
    )?;
    if exists {
        return reconcile(db, &next, facts, &frame, incoming)?.ok_or(StorageError::Conflict);
    }
    let c = Commit {
        account: AccountRow {
            generation: account_generation,
            digest: account_digest,
            state: account,
        },
        next_account: next_account
            .zip(next_account_digest)
            .map(|(state, digest)| AccountRow {
                generation: account_generation + 1,
                digest,
                state,
            }),
        previous: previous
            .zip(previous_digest)
            .map(|(state, digest)| row(session_generation - 1, state, digest)),
        next,
        message,
        frame: frame.clone(),
        received_at,
    };
    commit_inner(db, c, quota, count, true)?;
    Ok(frame)
}

/// Reject legacy operations that would bypass atomic ratchet/replay ownership.
pub(crate) fn legacy_guard(
    db: &Connection,
    sender: kami_core::DeviceId,
    id: kami_core::MessageId,
) -> Result<(), StorageError> {
    let protected: bool = db.query_row(
        "SELECT EXISTS(SELECT 1 FROM e2ee_frame WHERE sender=?1 AND id=?2)",
        params![sender.as_bytes(), id.as_u128().to_be_bytes()],
        |r| r.get(0),
    )?;
    if protected {
        Err(StorageError::Conflict)
    } else {
        Ok(())
    }
}
#[cfg(test)]
fn test_pause(phase: &str) {
    if std::env::var("KAMI_WP3_PHASE").ok().as_deref() == Some(phase) {
        let path =
            std::path::PathBuf::from(std::env::var_os("KAMI_WP3_CHILD").expect("test child path"));
        std::fs::write(path.join("ready"), b"ready").unwrap();
        loop {
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
    }
}

/// Exact committed-frame reconciliation, never a pre-authentication ACK oracle.
/// Caller must revalidate live end-peer/proofs; this private seam checks durable trust.
pub(crate) fn reconcile(
    db: &Connection,
    expected: &SessionRow,
    facts: MessageFacts,
    frame: &[u8],
    incoming: bool,
) -> Result<Option<Vec<u8>>, StorageError> {
    if frame.is_empty() || frame.len() > 65536 {
        return Err(StorageError::InvalidInput);
    }
    check_trust(db, expected)?;
    let current = load_session(db, expected.id)?.ok_or(StorageError::Conflict)?;
    if current.contact != expected.contact
        || current.pin != expected.pin
        || current.context != expected.context
        || current.trust_generation != expected.trust_generation
        || current.generation < expected.generation
    {
        return Err(StorageError::Conflict);
    }
    if current.generation == expected.generation && &current != expected {
        return Err(StorageError::Conflict);
    }
    let mut stmt=db.prepare("SELECT f.session,f.direction,f.session_generation,f.account_generation,length(f.frame),f.frame,m.facts,m.state,f.session_digest,f.account_digest FROM e2ee_frame f JOIN message m USING(sender,id) WHERE f.sender=?1 AND f.id=?2")?;
    let mut rows = stmt.query(params![
        facts.sender().as_bytes(),
        facts.message_id().as_u128().to_be_bytes()
    ])?;
    let Some(r) = rows.next()? else {
        return Ok(None);
    };
    if !(1..=65536).contains(&r.get::<_, i64>(4)?) {
        return Err(StorageError::WrongKeyOrCorrupt);
    }
    let bytes: Vec<u8> = r.get(5)?;
    let account = load_account(db)?.ok_or(StorageError::Conflict)?;
    if r.get::<_, [u8; 32]>(0)? != expected.id
        || r.get::<_, i64>(1)? != i64::from(incoming)
        || r.get::<_, i64>(2)? as u64 > current.generation
        || r.get::<_, i64>(3)? as u64 > account.generation
        || bytes != frame
        || r.get::<_, Vec<u8>>(6)? != crate::message::encode(facts)
        || (!incoming && r.get::<_, i64>(7)? != 0)
    {
        return Err(StorageError::Conflict);
    }
    if r.get::<_, i64>(2)? as u64 != expected.generation
        || r.get::<_, [u8; 32]>(8)? != expected.digest
        || (r.get::<_, i64>(3)? as u64 == account.generation
            && r.get::<_, [u8; 32]>(9)? != account.digest)
    {
        return Err(StorageError::Conflict);
    }
    if incoming && crate::inbox::receipt(db, facts.sender(), facts.message_id())? != Some(facts) {
        return Err(StorageError::Conflict);
    }
    Ok(Some(bytes))
}
