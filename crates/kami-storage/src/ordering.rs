use crate::{message, StorageError, StoredMessage};
use kami_core::{ConversationId, DeviceId};
use rusqlite::{params, Connection};

#[derive(Debug)]
pub struct OrderedMessage {
    pub message: StoredMessage,
    pub gap_before: bool,
}
/// Durable presentation eligibility survives clock rollback/restart; gaps reconcile against retained facts.
pub(crate) fn history(
    db: &mut Connection,
    conversation: ConversationId,
    sender: DeviceId,
    now: i64,
    wait: i64,
    limit: usize,
) -> Result<Vec<OrderedMessage>, StorageError> {
    if now < 0 || !(0..=60_000).contains(&wait) || limit == 0 || limit > 1024 {
        return Err(StorageError::InvalidInput);
    }
    let tx = db.transaction()?;
    let conversation = conversation.as_u128().to_be_bytes();
    let frontier: Option<i64> = tx.query_row("SELECT min(m.sequence) FROM message m WHERE m.conversation=?1 AND m.sender=?2 AND m.sequence>1 AND NOT EXISTS(SELECT 1 FROM message p WHERE p.conversation=m.conversation AND p.sender=m.sender AND p.sequence=m.sequence-1)", params![conversation,sender.as_bytes()], |r| r.get(0))?;
    tx.execute("UPDATE message SET visible=1 WHERE conversation=?1 AND sender=?2 AND (sequence < ?3 OR received_at <= ?4)", params![conversation,sender.as_bytes(),frontier.unwrap_or(i64::MAX),now.saturating_sub(wait)])?;
    // A retained but hidden predecessor cannot bridge a presentation gap. Use
    // visible retained facts (including deleted history) so late arrivals reconcile
    // gaps without treating local deletion as a missing network sequence.
    let mut stmt = tx.prepare("SELECT m.facts,h.payload, (m.sequence>1 AND NOT EXISTS(SELECT 1 FROM message p WHERE p.conversation=m.conversation AND p.sender=m.sender AND p.sequence=m.sequence-1 AND p.visible=1)) FROM message m JOIN history h USING(sender,id) WHERE m.conversation=?1 AND m.sender=?2 AND m.visible=1 ORDER BY m.sequence LIMIT ?3")?;
    let mut rows = stmt.query(params![conversation, sender.as_bytes(), limit as i64])?;
    let mut messages = Vec::new();
    while let Some(row) = rows.next()? {
        messages.push(OrderedMessage {
            message: StoredMessage::new(message::decode(&row.get::<_, Vec<u8>>(0)?)?, row.get(1)?)?,
            gap_before: row.get(2)?,
        });
    }
    drop(rows);
    drop(stmt);
    tx.commit()?;
    Ok(messages)
}
