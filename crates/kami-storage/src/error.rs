use kami_key_provider::KeyError;
use std::fmt;

/// Stable safe diagnostics: deliberately excludes SQL, OS paths and backend messages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageError {
    Key(KeyError),
    Lease,
    PermissionDenied,
    UnsupportedPlatform,
    MissingDatabase,
    EnrollmentRequired,
    AlreadyExists,
    CipherUnavailable,
    WrongKeyOrCorrupt,
    NewerSchema,
    Migration,
    Full,
    Busy,
    Conflict,
    InvalidInput,
    QueueFull,
    Closed,
    Io,
}
impl fmt::Display for StorageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "storage: {self:?}")
    }
}
impl std::error::Error for StorageError {}
impl From<rusqlite::Error> for StorageError {
    fn from(error: rusqlite::Error) -> Self {
        use rusqlite::ErrorCode;
        match error.sqlite_error_code() {
            Some(ErrorCode::DiskFull) => Self::Full,
            Some(ErrorCode::DatabaseBusy | ErrorCode::DatabaseLocked) => Self::Busy,
            Some(ErrorCode::DatabaseCorrupt | ErrorCode::NotADatabase) => Self::WrongKeyOrCorrupt,
            Some(ErrorCode::ConstraintViolation) => Self::Conflict,
            Some(ErrorCode::PermissionDenied | ErrorCode::ReadOnly) => Self::PermissionDenied,
            _ => Self::Io,
        }
    }
}
