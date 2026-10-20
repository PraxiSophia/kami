use crate::{message, StorageError};
use kami_core::{DeviceId, MessageFacts, MessageId};
use rusqlite::{params, Connection, OptionalExtension};

pub(crate) fn cancel(
    db: &mut Connection,
    sender: DeviceId,
    id: MessageId,
) -> Result<(), StorageError> {
    let tx = db.transaction()?;
    let updated = tx.execute("UPDATE message SET state=2, payload=NULL WHERE sender=?1 AND id=?2 AND direction=0 AND state IN (0,2)", params![sender.as_bytes(),id.as_u128().to_be_bytes()])?;
    if updated == 0 {
        return Err(StorageError::Conflict);
    }
    tx.commit()?;
    Ok(())
}
pub(crate) fn apply_receipt(
    db: &mut Connection,
    f: MessageFacts,
    authenticated_device: DeviceId,
) -> Result<(), StorageError> {
    if authenticated_device != f.recipient() {
        return Err(StorageError::Conflict);
    }
    let tx = db.transaction()?;
    let encoded = message::encode(f);
    let id = f.message_id().as_u128().to_be_bytes();
    let previous: Option<Vec<u8>> = tx
        .query_row(
            "SELECT facts FROM message WHERE sender=?1 AND id=?2 AND direction=0",
            params![f.sender().as_bytes(), id],
            |r| r.get(0),
        )
        .optional()?;
    if previous.as_deref() != Some(&encoded) {
        return Err(StorageError::Conflict);
    }
    tx.execute(
        "INSERT INTO receipt VALUES(?1,?2,?3) ON CONFLICT(sender,id) DO NOTHING",
        params![f.sender().as_bytes(), id, encoded],
    )?;
    tx.execute(
        "UPDATE message SET state=1, payload=NULL WHERE sender=?1 AND id=?2",
        params![f.sender().as_bytes(), id],
    )?;
    tx.commit()?;
    Ok(())
}
