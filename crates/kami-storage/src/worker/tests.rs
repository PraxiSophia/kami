use super::*;
use kami_key_provider::test_provider::TestKeyProvider;
#[test]
fn bounded_queue_rejects_without_dropping_accepted_requests() {
    let exe = std::env::current_exe().unwrap();
    let dir = tempfile::tempdir_in(exe.parent().unwrap()).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    let profile = ProfileKeyId::new("0123456789abcdef0123456789abcdef").unwrap();
    let provider = Arc::new(TestKeyProvider::seeded(profile.clone(), [42; 32]));
    let storage = Storage::create(
        ProfileLease::acquire(dir.path()).unwrap(),
        profile,
        provider,
        Config {
            queue_capacity: 1,
            ..Config::default()
        },
    )
    .unwrap();
    let (entered, wait) = mpsc::sync_channel(1);
    let (release, hold) = mpsc::sync_channel(1);
    storage
        .sender
        .as_ref()
        .unwrap()
        .try_send(Request::Pause(entered, hold))
        .unwrap();
    wait.recv().unwrap();
    let (reply, response) = mpsc::sync_channel(1);
    storage
        .sender
        .as_ref()
        .unwrap()
        .try_send(Request::Cipher(reply))
        .unwrap();
    assert_eq!(storage.cipher_version(), Err(StorageError::QueueFull));
    release.send(()).unwrap();
    assert!(response.recv().unwrap().unwrap().starts_with("4."));
    storage.shutdown().unwrap();
}
