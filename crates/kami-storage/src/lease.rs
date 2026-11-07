use std::fs::{File, OpenOptions};
use std::io;
use std::path::Path;

/// Persistent OS lockfile; never unlink it, including during shutdown.
pub struct ProfileLease {
    _file: File,
    pub(crate) directory: std::path::PathBuf,
}

impl ProfileLease {
    /// Diagnostics only, written through the already-locked handle. Never read
    /// this record to decide ownership; stale/torn metadata cannot steal a lock.
    pub fn record_owner(&mut self) -> io::Result<()> {
        use std::io::{Seek, SeekFrom, Write};
        use std::sync::atomic::{AtomicU64, Ordering};
        static INSTANCE: AtomicU64 = AtomicU64::new(1);
        let start = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(io::Error::other)?
            .as_nanos();
        let instance = INSTANCE.fetch_add(1, Ordering::Relaxed);
        self._file.set_len(0)?;
        self._file.seek(SeekFrom::Start(0))?;
        writeln!(
            self._file,
            "profile-owner-v1\npid={}\nstarted_ns={start}\ninstance={instance}",
            std::process::id()
        )?;
        self._file.flush()
    }
    pub fn acquire(profile: &Path) -> io::Result<Self> {
        secure_directory(profile)?;
        let path = profile.join("profile.lock");
        if let Ok(metadata) = std::fs::symlink_metadata(&path) {
            if !metadata.is_file() || metadata.file_type().is_symlink() {
                return Err(io::Error::from(io::ErrorKind::PermissionDenied));
            }
        }
        let mut options = OpenOptions::new();
        options.read(true).write(true).create(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options
                .mode(0o600)
                .custom_flags(rustix::fs::OFlags::NOFOLLOW.bits() as i32);
        }
        let file = options.open(path)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            let metadata = file.metadata()?;
            if metadata.uid() != rustix::process::geteuid().as_raw()
                || metadata.mode() & 0o077 != 0
                || metadata.nlink() != 1
            {
                return Err(io::Error::from(io::ErrorKind::PermissionDenied));
            }
        }
        file.try_lock().map_err(io::Error::other)?;
        Ok(Self {
            _file: file,
            directory: profile.to_owned(),
        })
    }
}

/// Reject substituted paths and insecure directories, not silently chmod them.
pub(crate) fn secure_directory(profile: &Path) -> io::Result<()> {
    if !profile.is_absolute() {
        return Err(io::Error::from(io::ErrorKind::InvalidInput));
    }
    for ancestor in profile.ancestors() {
        let metadata = std::fs::symlink_metadata(ancestor)?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(io::Error::from(io::ErrorKind::PermissionDenied));
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            let uid = metadata.uid();
            if (uid != 0 && uid != rustix::process::geteuid().as_raw())
                || metadata.mode() & 0o022 != 0
                || (ancestor == profile
                    && (uid != rustix::process::geteuid().as_raw() || metadata.mode() & 0o077 != 0))
            {
                return Err(io::Error::from(io::ErrorKind::PermissionDenied));
            }
        }
    }
    // Windows ACL/reparse-point qualification is deliberately fail-closed.
    #[cfg(not(unix))]
    return Err(io::Error::from(io::ErrorKind::Unsupported));
    #[cfg(unix)]
    Ok(())
}
