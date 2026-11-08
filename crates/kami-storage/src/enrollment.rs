use crate::StorageError;
use kami_key_provider::ProfileKeyId;
use rusqlite::{Connection, OptionalExtension};

pub(crate) fn read(
    db: &Connection,
    profile: &ProfileKeyId,
) -> Result<Option<Vec<u8>>, StorageError> {
    let count: i64 = db.query_row("SELECT count(*) FROM enrollment_complete", [], |r| r.get(0))?;
    if count > 1 {
        return Err(StorageError::WrongKeyOrCorrupt);
    }
    let row: Option<Vec<u8>> = db
        .query_row(
            "SELECT length(canonical),canonical FROM enrollment_complete WHERE singleton=1",
            [],
            |r| {
                if r.get::<_, i64>(0)? != 66 {
                    return Err(rusqlite::Error::InvalidQuery);
                }
                r.get(1)
            },
        )
        .optional()
        .map_err(|_| StorageError::WrongKeyOrCorrupt)?;
    if let Some(row) = &row {
        kami_identity::validate_enrollment_snapshot(profile, row)
            .map_err(|_| StorageError::WrongKeyOrCorrupt)?;
    } else if count != 0 {
        return Err(StorageError::WrongKeyOrCorrupt);
    }
    Ok(row)
}
pub(crate) fn commit(
    db: &mut Connection,
    profile: &ProfileKeyId,
    row: &[u8],
    fresh: bool,
) -> Result<Vec<u8>, StorageError> {
    if !fresh {
        return Err(StorageError::EnrollmentRequired);
    }
    kami_identity::validate_enrollment_snapshot(profile, row)
        .map_err(|_| StorageError::InvalidInput)?;
    let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    if read(&tx, profile)?.is_some() {
        return Err(StorageError::Conflict);
    }
    tx.execute("INSERT INTO enrollment_complete VALUES(1,?1)", [row])?;
    tx.commit()?; // synchronous=FULL WAL commit, before native anchor creation.
    let committed = read(db, profile)?.ok_or(StorageError::WrongKeyOrCorrupt)?;
    if committed != row {
        return Err(StorageError::WrongKeyOrCorrupt);
    }
    Ok(committed)
}
