//! Disposable native-custody qualification. Never touch the login user's bus/store.
#![allow(dead_code)]
use kami_key_provider::{KeyError, KeyProvider, ProfileKeyId};
use std::{
    io::Write,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

pub struct NativeFixture(std::process::Child);
impl Drop for NativeFixture {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
pub fn isolated() -> Option<NativeFixture> {
    const CHILD: &str = "KAMI_WPC_PRIVATE_NATIVE_TEST";
    if std::env::var_os(CHILD).is_none() {
        let root =
            tempfile::tempdir_in(std::env::current_exe().unwrap().parent().unwrap()).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        }
        let name = std::thread::current().name().unwrap_or("main").to_owned();
        let home = std::path::PathBuf::from(std::env::var_os("HOME").unwrap());
        let output = Command::new("dbus-run-session")
            .arg("--")
            .arg(std::env::current_exe().unwrap())
            .args(["--exact", &name, "--nocapture"])
            .env(
                "RUSTUP_HOME",
                std::env::var_os("RUSTUP_HOME")
                    .unwrap_or_else(|| home.join(".rustup").into_os_string()),
            )
            .env(
                "CARGO_HOME",
                std::env::var_os("CARGO_HOME")
                    .unwrap_or_else(|| home.join(".cargo").into_os_string()),
            )
            .env(CHILD, "1")
            .env("HOME", root.path())
            .env("XDG_DATA_HOME", root.path().join("data"))
            .env("XDG_CONFIG_HOME", root.path().join("config"))
            .env("XDG_RUNTIME_DIR", root.path())
            .env_remove("GNOME_KEYRING_CONTROL")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return None;
    }
    let mut daemon = NativeFixture(
        Command::new("gnome-keyring-daemon")
            .args(["--foreground", "--unlock", "--components=secrets"])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .spawn()
            .unwrap(),
    );
    daemon.0.stdin.take().unwrap().write_all(b"\n").unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    let profile = ProfileKeyId::new("00000000000000000000000000000000").unwrap();
    while kami_identity::NativeCustody::open()
        .load_database_key(&profile)
        .err()
        != Some(KeyError::Missing)
    {
        assert!(
            Instant::now() < deadline,
            "private native store did not become ready"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    Some(daemon)
}
pub fn new_profile(path: &std::path::Path) -> ProfileKeyId {
    assert!(std::env::var_os("KAMI_WPC_PRIVATE_NATIVE_TEST").is_some());
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos()
        ^ (u128::from(std::process::id()) << 64)
        ^ u128::from(NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed));
    let value = format!("{n:032x}");
    std::fs::write(path.join("fixture-profile"), &value).unwrap();
    ProfileKeyId::new(&value).unwrap()
}
pub fn profile(path: &std::path::Path) -> ProfileKeyId {
    assert!(std::env::var_os("KAMI_WPC_PRIVATE_NATIVE_TEST").is_some());
    ProfileKeyId::new(&std::fs::read_to_string(path.join("fixture-profile")).unwrap()).unwrap()
}
