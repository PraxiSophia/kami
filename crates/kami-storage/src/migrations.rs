use crate::StorageError;
use rusqlite::Connection;
#[cfg(test)]
mod tests;

pub(crate) const VERSION: i64 = 1;
pub(crate) fn migrate(connection: &mut Connection, create: bool) -> Result<(), StorageError> {
    let version: i64 = connection.pragma_query_value(None, "user_version", |r| r.get(0))?;
    if version > VERSION {
        return Err(StorageError::NewerSchema);
    }
    if version == 0 && !create {
        return Err(StorageError::Migration);
    }
    if version == 0 {
        let tx = connection.transaction()?;
        tx.execute_batch(include_str!("../migrations/001_initial.sql"))
            .map_err(|error| match StorageError::from(error) {
                StorageError::Full => StorageError::Full,
                StorageError::Busy => StorageError::Busy,
                StorageError::WrongKeyOrCorrupt => StorageError::WrongKeyOrCorrupt,
                _ => StorageError::Migration,
            })?;
        tx.pragma_update(None, "user_version", VERSION)?;
        tx.commit()?;
    }
    Ok(())
}
