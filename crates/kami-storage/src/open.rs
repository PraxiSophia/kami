use crate::{migrations, ProfileLease, StorageError};
use kami_key_provider::{KeyProvider, ProfileKeyId};
use rusqlite::{Connection, OpenFlags};
use std::io::Write;
use zeroize::Zeroizing;

pub(crate) fn connect(
    lease: &ProfileLease,
    profile: &ProfileKeyId,
    provider: &dyn KeyProvider,
    create: bool,
    max_pages: u32,
) -> Result<Connection, StorageError> {
    let key = provider
        .load_database_key(profile)
        .map_err(StorageError::Key)?;
    let path = lease.directory.join("messages.db");
    let marker = lease.directory.join("enrollment");
    if create {
        if path.try_exists().map_err(|_| StorageError::Io)?
            || marker.try_exists().map_err(|_| StorageError::Io)?
        {
            return Err(StorageError::AlreadyExists);
        }
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut evidence = options.open(&marker).map_err(|_| StorageError::Io)?;
        evidence
            .write_all(b"kami-enrollment-v1\n")
            .map_err(|_| StorageError::Io)?;
        evidence.sync_all().map_err(|_| StorageError::Io)?;
        std::fs::File::open(&lease.directory)
            .and_then(|f| f.sync_all())
            .map_err(|_| StorageError::Io)?;
        let mut db_options = std::fs::OpenOptions::new();
        db_options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            db_options.mode(0o600);
        }
        db_options.open(&path).map_err(|_| StorageError::Io)?;
    } else {
        if !path.try_exists().map_err(|_| StorageError::Io)? {
            return Err(StorageError::MissingDatabase);
        }
        if !marker.try_exists().map_err(|_| StorageError::Io)? {
            return Err(StorageError::EnrollmentRequired);
        }
    }
    for name in [
        "messages.db",
        "messages.db-wal",
        "messages.db-shm",
        "messages.db-journal",
        "enrollment",
    ] {
        let file = lease.directory.join(name);
        match std::fs::symlink_metadata(file) {
            Ok(meta) => {
                if !meta.is_file() || meta.file_type().is_symlink() {
                    return Err(StorageError::PermissionDenied);
                }
                #[cfg(unix)]
                {
                    use std::os::unix::fs::MetadataExt;
                    if meta.uid() != rustix::process::geteuid().as_raw()
                        || meta.mode() & 0o077 != 0
                        || meta.nlink() != 1
                    {
                        return Err(StorageError::PermissionDenied);
                    }
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err(StorageError::Io),
        }
    }
    let mut db = Connection::open_with_flags(
        &path,
        OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_NO_MUTEX
            | OpenFlags::SQLITE_OPEN_NOFOLLOW,
    )?;
    // Key before any schema/page access. Only this zeroizing buffer exposes the raw key.
    let mut sql = Zeroizing::new(String::with_capacity(100));
    sql.push_str("PRAGMA key = \"x'");
    key.with_bytes(|bytes| {
        use std::fmt::Write;
        for b in bytes {
            let _ = write!(sql, "{b:02x}");
        }
    });
    sql.push_str("'\";");
    db.execute_batch(&sql)?;
    drop(sql);
    drop(key);
    let cipher: String = db
        .pragma_query_value(None, "cipher_version", |r| r.get(0))
        .map_err(|_| StorageError::CipherUnavailable)?;
    if !cipher.starts_with("4.") {
        return Err(StorageError::CipherUnavailable);
    }
    db.execute_batch("PRAGMA cipher_compatibility=4; PRAGMA cipher_memory_security=ON; PRAGMA cipher_log_level=NONE;")?;
    db.set_limit(rusqlite::limits::Limit::SQLITE_LIMIT_ATTACHED, 0)?;
    db.set_limit(rusqlite::limits::Limit::SQLITE_LIMIT_LENGTH, 1024 * 1024)?;
    let temp_compilation: bool = db.query_row("SELECT sqlite_compileoption_used('TEMP_STORE=2') OR sqlite_compileoption_used('TEMP_STORE=3')",[],|r|r.get(0))?;
    let page_size: String = db.pragma_query_value(None, "cipher_page_size", |r| r.get(0))?;
    let header: String =
        db.pragma_query_value(None, "cipher_plaintext_header_size", |r| r.get(0))?;
    let hmac: String = db.pragma_query_value(None, "cipher_use_hmac", |r| r.get(0))?;
    if !temp_compilation || page_size != "4096" || header != "0" || hmac != "1" {
        return Err(StorageError::CipherUnavailable);
    }
    db.query_row("SELECT count(*) FROM sqlite_schema", [], |_| Ok(()))?;
    db.busy_timeout(std::time::Duration::from_millis(250))?;
    db.execute_batch("PRAGMA foreign_keys=ON; PRAGMA temp_store=MEMORY; PRAGMA synchronous=FULL; PRAGMA secure_delete=ON;")?;
    let journal: String = db.pragma_query_value(None, "journal_mode", |r| r.get(0))?;
    if journal != "wal" {
        let selected: String = db.query_row("PRAGMA journal_mode=WAL", [], |r| r.get(0))?;
        if selected != "wal" {
            return Err(StorageError::CipherUnavailable);
        }
    }
    let sync: i64 = db.pragma_query_value(None, "synchronous", |r| r.get(0))?;
    let temp: i64 = db.pragma_query_value(None, "temp_store", |r| r.get(0))?;
    let foreign: i64 = db.pragma_query_value(None, "foreign_keys", |r| r.get(0))?;
    if sync != 2 || temp != 2 || foreign != 1 {
        return Err(StorageError::CipherUnavailable);
    }
    let actual_pages: u32 =
        db.query_row(&format!("PRAGMA max_page_count={max_pages}"), [], |r| {
            r.get(0)
        })?;
    if actual_pages > max_pages {
        return Err(StorageError::Full);
    }
    // Existing encrypted profiles must be namespace-bound BEFORE an additive
    // migration can mutate their schema. Unknown/empty schemas retain migration's
    // existing fail-closed diagnostics.
    let version: i64 = db.pragma_query_value(None, "user_version", |r| r.get(0))?;
    if !create && (1..=migrations::VERSION).contains(&version) {
        let namespace: String =
            db.query_row("SELECT namespace FROM profile WHERE singleton=1", [], |r| {
                r.get(0)
            })?;
        if namespace != profile.as_str() {
            return Err(StorageError::WrongKeyOrCorrupt);
        }
    }
    migrations::migrate(&mut db, create)?;
    // Schema identity binds a keyed database to the requested credential namespace.
    if create {
        db.execute("INSERT INTO profile VALUES(1, ?1)", [profile.as_str()])?;
    }
    let namespace: String =
        db.query_row("SELECT namespace FROM profile WHERE singleton=1", [], |r| {
            r.get(0)
        })?;
    if namespace != profile.as_str() {
        return Err(StorageError::WrongKeyOrCorrupt);
    }
    let integrity: String = db.query_row("PRAGMA integrity_check", [], |r| r.get(0))?;
    if integrity != "ok" {
        return Err(StorageError::WrongKeyOrCorrupt);
    }
    let mut foreign = db.prepare("PRAGMA foreign_key_check")?;
    if foreign.query([])?.next()?.is_some() {
        return Err(StorageError::WrongKeyOrCorrupt);
    }
    drop(foreign);
    std::fs::File::open(&lease.directory)
        .and_then(|f| f.sync_all())
        .map_err(|_| StorageError::Io)?;
    Ok(db)
}
