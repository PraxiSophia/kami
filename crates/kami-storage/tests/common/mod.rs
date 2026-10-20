#![allow(dead_code)]
use kami_core::{
    ContentBinding, ConversationId, DeviceId, MessageFacts, MessageId, SenderSequence,
};
use kami_key_provider::{DatabaseKey, KeyError, KeyProvider, ProfileKeyId};
use kami_storage::{Config, ProfileLease, Storage, StoredMessage};
use std::sync::Arc;
use zeroize::Zeroizing;
pub struct Provider(pub Result<u8, KeyError>);
impl KeyProvider for Provider {
    fn load_database_key(&self, _: &ProfileKeyId) -> Result<DatabaseKey, KeyError> {
        self.0
            .map(|byte| DatabaseKey::from_bytes(Zeroizing::new([byte; 32])))
    }
}
pub fn profile() -> ProfileKeyId {
    ProfileKeyId::new("0123456789abcdef0123456789abcdef").unwrap()
}
pub fn directory() -> tempfile::TempDir {
    let exe = std::env::current_exe().unwrap();
    let dir = tempfile::tempdir_in(exe.parent().unwrap()).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    dir
}
pub fn create(dir: &tempfile::TempDir) -> Storage {
    Storage::create(
        ProfileLease::acquire(dir.path()).unwrap(),
        profile(),
        Arc::new(Provider(Ok(42))),
        Config::default(),
    )
    .unwrap()
}
pub fn open(dir: &tempfile::TempDir) -> Storage {
    Storage::open(
        ProfileLease::acquire(dir.path()).unwrap(),
        profile(),
        Arc::new(Provider(Ok(42))),
        Config::default(),
    )
    .unwrap()
}
pub fn facts(id: u128, sequence: u64) -> MessageFacts {
    MessageFacts::new(
        MessageId::from_u128(id),
        ConversationId::from_u128(2),
        DeviceId::from_bytes([1; 32]),
        DeviceId::from_bytes([2; 32]),
        SenderSequence::new(sequence),
        ContentBinding::from_bytes([3; 32]),
    )
}
pub fn message(f: MessageFacts) -> StoredMessage {
    StoredMessage::new(f, b"SECRET-CONTENT-storage-regression-marker".to_vec()).unwrap()
}
pub fn raw(dir: &tempfile::TempDir) -> rusqlite::Connection {
    let db = rusqlite::Connection::open(dir.path().join("messages.db")).unwrap();
    db.execute_batch(&format!(
        "PRAGMA key=\"x'{}'\"; PRAGMA cipher_compatibility=4;",
        "2a".repeat(32)
    ))
    .unwrap();
    db
}
