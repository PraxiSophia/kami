use crate::{message, StorageError, StoredMessage};
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};

pub(crate) fn enqueue(
    db: &mut Connection,
    msg: StoredMessage,
    quota: u64,
    count_quota: u32,
) -> Result<(), StorageError> {
    let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
    enqueue_in(&tx, msg, quota, count_quota)?;
    tx.commit()?;
    Ok(())
}
pub(crate) fn enqueue_in(
    tx: &rusqlite::Transaction<'_>,
    msg: StoredMessage,
    quota: u64,
    count_quota: u32,
) -> Result<(), StorageError> {
    let f = msg.facts();
    let reserved: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM sender_prepare WHERE conversation=?1 AND sender=?2 AND state=0)",params![f.conversation_id().as_u128().to_be_bytes(),f.sender().as_bytes()],|r|r.get(0))?;
    if reserved {
        return Err(StorageError::Busy);
    }
    let id = f.message_id().as_u128().to_be_bytes();
    let conversation = f.conversation_id().as_u128().to_be_bytes();
    let encoded = message::encode(f);
    let existing: Option<(Vec<u8>, Option<Vec<u8>>, i64)> = tx
        .query_row(
            "SELECT facts,payload,direction FROM message WHERE sender=?1 AND id=?2",
            params![f.sender().as_bytes(), id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?;
    if let Some((facts, payload, direction)) = existing {
        if facts != encoded || direction != 0 || payload.as_deref() != Some(msg.payload()) {
            return Err(StorageError::Conflict);
        }
        return Ok(());
    }
    let count: i64 = tx.query_row(
        "SELECT count(*) FROM message WHERE direction=0 AND state=0",
        [],
        |r| r.get(0),
    )?;
    if count >= i64::from(count_quota) {
        return Err(StorageError::Full);
    }
    let used: i64 = tx.query_row(
        "SELECT COALESCE(sum(length(payload)),0) FROM message WHERE direction=0 AND state=0",
        [],
        |r| r.get(0),
    )?;
    if (used as u64)
        .checked_add(msg.payload().len() as u64)
        .is_none_or(|n| n > quota)
    {
        return Err(StorageError::Full);
    }
    let last: Option<i64> = tx
        .query_row(
            "SELECT last FROM sender_sequence WHERE conversation=?1 AND sender=?2",
            params![conversation, f.sender().as_bytes()],
            |r| r.get(0),
        )
        .optional()?;
    if last.unwrap_or(0).checked_add(1) != Some(f.sequence().value() as i64) {
        return Err(StorageError::Conflict);
    }
    tx.execute("INSERT INTO sender_sequence VALUES(?1,?2,?3) ON CONFLICT(conversation,sender) DO UPDATE SET last=excluded.last", params![conversation, f.sender().as_bytes(), f.sequence().value() as i64])?;
    tx.execute("INSERT INTO message(sender,id,conversation,sequence,facts,direction,payload) VALUES(?1,?2,?3,?4,?5,0,?6)", params![f.sender().as_bytes(), id, conversation, f.sequence().value() as i64, encoded, msg.payload()])?;
    tx.execute(
        "INSERT INTO history VALUES(?1,?2,?3)",
        params![f.sender().as_bytes(), id, msg.payload()],
    )?;
    Ok(())
}

pub(crate) fn next_sequence(
    db: &Connection,
    conversation: kami_core::ConversationId,
    sender: kami_core::DeviceId,
) -> Result<kami_core::SenderSequence, StorageError> {
    let last: Option<i64> = db
        .query_row(
            "SELECT last FROM sender_sequence WHERE conversation=?1 AND sender=?2",
            params![conversation.as_u128().to_be_bytes(), sender.as_bytes()],
            |r| r.get(0),
        )
        .optional()?;
    let next = last
        .unwrap_or(0)
        .checked_add(1)
        .filter(|v| *v > 0)
        .ok_or(StorageError::Full)?;
    Ok(kami_core::SenderSequence::new(next as u64))
}

pub(crate) fn pending(db: &Connection, limit: usize) -> Result<Vec<StoredMessage>, StorageError> {
    if limit == 0 || limit > 1024 {
        return Err(StorageError::InvalidInput);
    }
    let mut statement = db.prepare("SELECT facts,payload FROM message m WHERE direction=0 AND state=0 AND NOT EXISTS(SELECT 1 FROM e2ee_frame f WHERE f.sender=m.sender AND f.id=m.id) ORDER BY conversation,sender,sequence LIMIT ?1")?;
    let mut rows = statement.query([limit as i64])?;
    let mut messages = Vec::new();
    while let Some(row) = rows.next()? {
        messages.push(StoredMessage::new(
            message::decode(&row.get::<_, Vec<u8>>(0)?)?,
            row.get(1)?,
        )?);
    }
    Ok(messages)
}
