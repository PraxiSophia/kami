use kami_key_provider::{test_provider::TestKeyProvider, ProfileKeyId};
use kami_runtime::{Config, Engine, Error, Profile, ShutdownOutcome};
use std::{
    io::{BufRead, BufReader, Write},
    process::{Command, Stdio},
    sync::Arc,
};
fn profile(path: &std::path::Path, create: bool) -> Profile {
    let key_id = ProfileKeyId::new("aabbccddeeff00112233445566778899").unwrap();
    Profile {
        directory: path.to_owned(),
        key_id: key_id.clone(),
        key_provider: Arc::new(TestKeyProvider::seeded(key_id, [71; 32])),
        create,
        storage: Default::default(),
    }
}
#[test]
fn ownership_child() {
    let Some(path) = std::env::var_os("KAMI_RUNTIME_OWNERSHIP_CHILD") else {
        return;
    };
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async {
        let (engine, _) = Engine::start(
            profile(std::path::Path::new(&path), true),
            Config::default(),
        )
        .unwrap();
        engine.wait_running().await.unwrap();
        println!("OWNER_READY");
        std::io::stdout().flush().unwrap();
        // Test-only process stays alive until killed by the parent.
        std::future::pending::<()>().await;
    });
}
#[tokio::test]
async fn live_process_cannot_be_stolen_and_killed_owner_reopens_with_stale_metadata() {
    let base = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::current_dir().ok())
        .expect("a usable private test-directory base");
    let dir = tempfile::Builder::new()
        .prefix("kami-runtime-ownership-")
        .tempdir_in(base)
        .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "ownership_child", "--nocapture"])
        .env("KAMI_RUNTIME_OWNERSHIP_CHILD", dir.path())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .unwrap();
    struct KillOnDrop<'a>(&'a mut std::process::Child);
    impl Drop for KillOnDrop<'_> {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    let guard = KillOnDrop(&mut child);
    let reader = BufReader::new(guard.0.stdout.take().unwrap());
    let mut ready = false;
    for line in reader.lines() {
        if line.unwrap() == "OWNER_READY" {
            ready = true;
            break;
        }
    }
    assert!(ready, "child did not initialize runtime");
    let metadata = std::fs::read_to_string(dir.path().join("profile.lock")).unwrap();
    assert!(metadata.contains("profile-owner-v1"));
    assert!(metadata.contains(&format!("pid={}", guard.0.id())));
    // Bogus metadata is diagnostic only; it must never override a live lock.
    std::fs::write(
        dir.path().join("profile.lock"),
        "pid=0\nstarted=0\nversion=1\n",
    )
    .unwrap();
    let (second, _) = Engine::start(profile(dir.path(), false), Config::default()).unwrap();
    assert_eq!(second.wait_running().await, Err(Error::ProfileAlreadyOwned));
    drop(guard); // real process termination, not a simulated stale lock
    let (restarted, _) = Engine::start(profile(dir.path(), false), Config::default()).unwrap();
    restarted.wait_running().await.unwrap();
    assert_eq!(restarted.recovered().await.unwrap(), 0);
    assert_eq!(restarted.shutdown().await, ShutdownOutcome::Clean);
    restarted.wait_stopped().await;
}
