//! Safe external Rust cannot fabricate or accidentally print a trusted candidate.
use std::{path::PathBuf, process::Command};

#[test]
fn opaque_commit_privacy_has_positive_and_diagnostic_negative_probes() {
    let output = Command::new("cargo")
        .args([
            "build",
            "--locked",
            "--offline",
            "-p",
            "kami-e2ee",
            "--message-format=json",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let library = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .find(|v| v["target"]["name"] == "kami_e2ee" && v["reason"] == "compiler-artifact")
        .unwrap()["filenames"][0]
        .as_str()
        .unwrap()
        .to_owned();
    let dir = std::env::temp_dir().join(format!("kami-e2ee-privacy-{}", std::process::id()));
    std::fs::create_dir(&dir).unwrap();
    struct Cleanup(PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let _cleanup = Cleanup(dir.clone());
    let deps = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
    for (source, code) in [
        ("pub fn valid(_: kami_e2ee::AuthenticatedCommit) {}", None),
        ("pub fn valid(_: kami_e2ee::AuthenticatedDelivery, _: kami_e2ee::AuthenticatedReceiptCommit, _: kami_e2ee::ReplayPermit) {}", None),
        ("pub fn bad() -> kami_e2ee::AuthenticatedDelivery { kami_e2ee::AuthenticatedDelivery { inbound: loop {}, receipt: loop {} } }", Some("E0451")),
        ("pub fn bad() -> kami_e2ee::AuthenticatedReceiptCommit { kami_e2ee::AuthenticatedReceiptCommit { material: loop {}, target_frame_digest: [0; 32] } }", Some("E0451")),
        ("pub fn bad() -> kami_e2ee::ReplayPermit { kami_e2ee::ReplayPermit { remote: loop {}, local: loop {}, conversation: loop {}, generation: 1, pin: vec![] } }", Some("E0451")),
        ("pub fn bad(v: kami_e2ee::AuthenticatedReceiptCommit) { let _: kami_e2ee::AuthenticatedCommit = v.into(); }", Some("E0277")),
        ("pub fn bad(v: kami_e2ee::AuthenticatedDelivery) { let _ = v.clone(); }", Some("E0599")),
        ("use kami_e2ee::authenticated::SendStage;", Some("E0603")),
        ("pub fn bad() -> kami_e2ee::AuthenticatedCommit { kami_e2ee::AuthenticatedCommit { material: loop {} } }", Some("E0451")),
        ("pub fn bad() { let _: kami_e2ee::AuthenticatedCommit = Default::default(); }", Some("E0277")),
        ("pub fn bad(v: kami_e2ee::AuthenticatedCommit) { let _ = v.clone(); }", Some("E0599")),
        ("pub fn bad(v: kami_e2ee::AuthenticatedCommit) { let _ = format!(\"{:?}\", v); }", Some("E0277")),
        ("pub fn bad(v: kami_e2ee::CommitMaterial) { let _ = format!(\"{:?}\", v); }", Some("E0277")),
        ("pub fn bad(v: kami_e2ee::SnapshotTransfer) { let _ = format!(\"{:?}\", v); }", Some("E0277")),
        ("pub fn bad(v: kami_e2ee::CommitMaterial) { let _ = kami_e2ee::AuthenticatedCommit::from(v); }", Some("E0308")),
        ("pub fn bad() { let _ = kami_e2ee::AuthenticatedCommit::new(); }", Some("E0599")),
        ("pub fn bad() { let _: kami_e2ee::AuthenticatedCommit = Vec::<u8>::new().into(); }", Some("E0277")),
        ("pub fn bad<T: serde::Serialize>() {} pub fn check() { bad::<kami_e2ee::AuthenticatedCommit>(); }", Some("E0277")),
        ("pub fn bad<T: serde::de::DeserializeOwned>() {} pub fn check() { bad::<kami_e2ee::AuthenticatedCommit>(); }", Some("E0277")),
    ] {
        let input = dir.join("probe.rs"); std::fs::write(&input, source).unwrap();
        let mut command = Command::new("rustc");
        command.args(["--edition=2021", "--crate-type=lib"]).arg(&input).arg("--out-dir").arg(&dir)
            .arg("-L").arg(format!("dependency={}", deps.display())).arg("--extern").arg(format!("kami_e2ee={library}"));
        // serde is already a direct dependency; use Cargo's coherent artifact.
        if source.contains("serde::") {
            let serde = Command::new("cargo").args(["build", "--locked", "--offline", "-p", "kami-e2ee", "--message-format=json"]).output().unwrap();
            let artifact = String::from_utf8(serde.stdout).unwrap().lines().filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
                .find(|v| v["target"]["name"] == "serde" && v["reason"] == "compiler-artifact").unwrap();
            command.arg("--extern").arg(format!("serde={}", artifact["filenames"][0].as_str().unwrap()));
        }
        let result = command.output().unwrap(); let stderr = String::from_utf8_lossy(&result.stderr);
        if let Some(code) = code { assert!(!result.status.success() && stderr.contains(code), "{source}: {stderr}"); }
        else { assert!(result.status.success(), "{stderr}"); }
    }
}
