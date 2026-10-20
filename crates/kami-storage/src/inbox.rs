use crate::{message, StorageError, StoredMessage};
use kami_core::{ConversationId, DeviceId, MessageFacts, MessageId};
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};

/// Either variant grants ACK eligibility only AFTER the durable transaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Acceptance {
    New,
    Duplicate,
}
pub(crate) fn accept(
    db: &mut Connection,
    msg: StoredMessage,
    received_at: i64,
) -> Result<Acceptance, StorageError> {
    if received_at < 0 {
        return Err(StorageError::InvalidInput);
    }
    let f = msg.facts();
    let id = f.message_id().as_u128().to_be_bytes();
    let encoded = message::encode(f);
    let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let previous: Option<(Vec<u8>, i64)> = tx
        .query_row(
            "SELECT facts,direction FROM message WHERE sender=?1 AND id=?2",
            params![f.sender().as_bytes(), id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    if let Some((old, direction)) = previous {
        if old != encoded || direction != 1 {
            return Err(StorageError::Conflict);
        }
        tx.commit()?;
        return Ok(Acceptance::Duplicate);
    }
    tx.execute("INSERT INTO message(sender,id,conversation,sequence,facts,direction,received_at) VALUES(?1,?2,?3,?4,?5,1,?6)", params![f.sender().as_bytes(),id,f.conversation_id().as_u128().to_be_bytes(),f.sequence().value() as i64,encoded,received_at])?;
    tx.execute(
        "INSERT INTO history VALUES(?1,?2,?3)",
        params![f.sender().as_bytes(), id, msg.payload()],
    )?;
    tx.execute(
        "INSERT INTO receipt VALUES(?1,?2,?3)",
        params![f.sender().as_bytes(), id, encoded],
    )?;
    tx.commit()?;
    Ok(Acceptance::New)
}
pub(crate) fn receipt(
    db: &Connection,
    sender: DeviceId,
    id: MessageId,
) -> Result<Option<MessageFacts>, StorageError> {
    let bytes: Option<Vec<u8>> = db
        .query_row(
            "SELECT facts FROM receipt WHERE sender=?1 AND id=?2",
            params![sender.as_bytes(), id.as_u128().to_be_bytes()],
            |r| r.get(0),
        )
        .optional()?;
    bytes.map(|b| message::decode(&b)).transpose()
}
pub(crate) fn delete_history(
    db: &mut Connection,
    sender: DeviceId,
    id: MessageId,
) -> Result<(), StorageError> {
    let tx = db.transaction()?;
    tx.execute(
        "DELETE FROM history WHERE sender=?1 AND id=?2",
        params![sender.as_bytes(), id.as_u128().to_be_bytes()],
    )?;
    tx.commit()?;
    Ok(())
}
pub(crate) fn history(
    db: &Connection,
    conversation: ConversationId,
    limit: usize,
) -> Result<Vec<StoredMessage>, StorageError> {
    if limit == 0 || limit > 1024 {
        return Err(StorageError::InvalidInput);
    }
    let mut stmt = db.prepare("SELECT m.facts,h.payload FROM message m JOIN history h USING(sender,id) WHERE m.conversation=?1 ORDER BY m.sender,m.sequence LIMIT ?2")?;
    let mut rows = stmt.query(params![conversation.as_u128().to_be_bytes(), limit as i64])?;
    let mut messages = Vec::new();
    while let Some(row) = rows.next()? {
        messages.push(StoredMessage::new(
            message::decode(&row.get::<_, Vec<u8>>(0)?)?,
            row.get(1)?,
        )?);
    }
    Ok(messages)
}
