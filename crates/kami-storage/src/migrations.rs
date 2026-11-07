use crate::StorageError;
use rusqlite::Connection;
#[cfg(test)]
mod tests;

pub(crate) const VERSION: i64 = 5;
pub(crate) fn migrate(connection: &mut Connection, create: bool) -> Result<(), StorageError> {
    migrate_with_entropy(connection, create, |key| {
        getrandom::fill(key).map_err(|_| StorageError::Io)
    })
}
fn migrate_with_entropy(
    connection: &mut Connection,
    create: bool,
    entropy: impl FnOnce(&mut [u8]) -> Result<(), StorageError>,
) -> Result<(), StorageError> {
    let version: i64 = connection.pragma_query_value(None, "user_version", |r| r.get(0))?;
    if version > VERSION {
        return Err(StorageError::NewerSchema);
    }
    if version < 0 || (version == 0 && !create) {
        return Err(StorageError::Migration);
    }
    if version < VERSION {
        let tx = connection.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        if version == 0 {
            tx.execute_batch(include_str!("../migrations/001_initial.sql"))
                .map_err(migration_error)?;
        }
        if version < 2 {
            tx.execute_batch(include_str!("../migrations/002_contact_trust.sql"))
                .map_err(migration_error)?;
        }
        if version < 3 {
            tx.execute_batch(include_str!("../migrations/003_e2ee_state.sql"))
                .map_err(migration_error)?;
        }
        if version < 4 {
            tx.execute_batch(include_str!("../migrations/004_sender_state.sql"))
                .map_err(migration_error)?;
            let mut key = zeroize::Zeroizing::new([0u8; 32]);
            entropy(key.as_mut())?;
            tx.execute(
                "INSERT INTO sender_binding_key VALUES(1,?1)",
                [key.as_slice()],
            )?;
        }
        if version < 5 {
            tx.execute_batch(include_str!("../migrations/005_receiver_state.sql"))
                .map_err(migration_error)?;
        }
        tx.pragma_update(None, "user_version", VERSION)?;
        tx.commit()?;
    }
    Ok(())
}
fn migration_error(error: rusqlite::Error) -> StorageError {
    match StorageError::from(error) {
        StorageError::Full => StorageError::Full,
        StorageError::Busy => StorageError::Busy,
        StorageError::WrongKeyOrCorrupt => StorageError::WrongKeyOrCorrupt,
        _ => StorageError::Migration,
    }
}
#[cfg(test)]
mod sender_tests {
    use super::*;
    #[test]
    fn entropy_failure_rolls_back_v4() {
        let mut db = Connection::open_in_memory().unwrap();
        assert_eq!(
            migrate_with_entropy(&mut db, true, |_| Err(StorageError::Io)),
            Err(StorageError::Io)
        );
        assert_eq!(
            db.pragma_query_value::<i64, _>(None, "user_version", |r| r.get(0))
                .unwrap(),
            0
        );
        let tables: i64 = db
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type='table'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(tables, 0);
    }
}
