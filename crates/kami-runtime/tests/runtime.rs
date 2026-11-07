use kami_key_provider::{test_provider::TestKeyProvider, ProfileKeyId};
use kami_runtime::{Config, Engine, Lifecycle, Profile, ShutdownOutcome};
use std::sync::Arc;

fn profile(dir: &tempfile::TempDir, create: bool) -> Profile {
    let id = ProfileKeyId::new("aabbccddeeff00112233445566778899").unwrap();
    Profile {
        directory: dir.path().to_owned(),
        key_id: id.clone(),
        key_provider: Arc::new(TestKeyProvider::seeded(id, [71; 32])),
        create,
        storage: Default::default(),
    }
}

#[tokio::test]
async fn starts_reconciles_drains_and_reopens_real_sqlcipher() {
    let base = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::current_dir().ok())
        .expect("a usable private test-directory base");
    let dir = tempfile::Builder::new()
        .prefix("kami-runtime-")
        .tempdir_in(base)
        .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    let (engine, mut events) = Engine::start(profile(&dir, true), Config::default()).unwrap();
    engine.wait_running().await.unwrap();
    assert_eq!(engine.snapshot().lifecycle, Lifecycle::Running);
    assert_eq!(engine.health().await.unwrap(), Lifecycle::Running);
    assert_eq!(engine.recovered().await.unwrap(), 0);
    assert_eq!(engine.shutdown().await, ShutdownOutcome::Clean);
    engine.wait_stopped().await;
    assert_eq!(engine.snapshot().lifecycle, Lifecycle::Stopped);
    let mut saw_running = false;
    while let Ok(event) = events.try_recv() {
        if event == kami_runtime::Event::Lifecycle(Lifecycle::Running) {
            saw_running = true;
        }
    }
    assert!(saw_running);
    let (again, _) = Engine::start(profile(&dir, false), Config::default()).unwrap();
    again.wait_running().await.unwrap();
    assert_eq!(again.shutdown().await, ShutdownOutcome::Clean);
    again.wait_stopped().await;
}
