//! These probes prove runtime API privacy, NOT global sealing of downstream crates.
use std::process::Command;
#[test]
fn production_api_has_no_capability_factory_or_bare_authority_submission() {
    let build = Command::new("cargo")
        .args([
            "build",
            "--locked",
            "--offline",
            "-p",
            "kami-runtime",
            "--message-format=json",
        ])
        .output()
        .unwrap();
    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    let artifacts: Vec<serde_json::Value> = String::from_utf8(build.stdout)
        .unwrap()
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter(|v| v["reason"] == "compiler-artifact")
        .collect();
    let lib = artifacts
        .iter()
        .find(|v| v["target"]["name"] == "kami_runtime")
        .unwrap()["filenames"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|p| p.as_str())
        .find(|p| p.ends_with(".rlib"))
        .unwrap()
        .to_owned();
    let deps = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .to_owned();
    let base = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::current_dir().ok())
        .expect("a usable private test-directory base");
    let dir = tempfile::Builder::new()
        .prefix("kami-runtime-privacy-")
        .tempdir_in(base)
        .unwrap();
    for (source, error) in [
        ("pub fn good(e: &kami_runtime::Engine) { let _ = e.snapshot(); e.request_shutdown(); }", None),
        ("use kami_runtime::ingress::AuthenticatedPeer;", Some("E0603")),
        ("pub fn bad() { let _ = kami_runtime::ingress::AuthenticatedPeer::mock(loop {}, loop {}, loop {}, loop {}, loop {}); }", Some("E0603")),
        ("pub fn bad() { let _: kami_runtime::ingress::AuthenticatedPeer = Default::default(); }", Some("E0603")),
        ("pub fn bad() { let _: kami_runtime::ingress::AuthenticatedPeer = vec![0u8].into(); }", Some("E0603")),
        ("pub fn bad(e: &kami_runtime::Engine) { e.submit_authenticated_peer(\"peer\", true); }", Some("E0599")),
        ("pub fn bad(e: &kami_runtime::Engine) { e.receive_receipt(vec![0u8], [0u8; 32]); }", Some("E0599")),
        ("pub fn bad(e: &kami_runtime::Engine) { e.test_work(|_| Ok(1)); }", Some("E0599")),
        ("impl kami_runtime::TrustedTransport for () {}", Some("E0405")),
        ("pub fn bad(e: &kami_runtime::Engine) { e.submit_receive(loop {}, loop {}); }", Some("E0599")),
        ("use kami_runtime::worker::Work;", Some("E0603")),
        ("use kami_runtime::engine::IngressConnection;", Some("E0603")),
        ("use kami_runtime::ingress::{Action, Outcome};", Some("E0603")),
        ("pub fn bad(e: &kami_runtime::Engine) { let _ = e.ingress_peer(loop {}, loop {}, loop {}, loop {}); }", Some("E0624")),
        ("pub fn bad(e: &kami_runtime::Engine) { let _ = e.dispatch_ingress(loop {}, loop {}); }", Some("E0624")),
    ] {
        let input = dir.path().join("probe.rs"); std::fs::write(&input, source).unwrap();
        let output = Command::new("rustc").args(["--edition=2021", "--crate-type=lib", "--crate-name=runtime_probe"])
            .arg(&input).arg("--out-dir").arg(dir.path())
            .arg("-L").arg(format!("dependency={}", deps.display()))
            .arg("--extern").arg(format!("kami_runtime={lib}")).output().unwrap();
        let stderr = String::from_utf8_lossy(&output.stderr);
        match error {
            Some(code) => assert!(!output.status.success() && stderr.contains(code), "{source}: {stderr}"),
            None => assert!(output.status.success(), "{stderr}"),
        }
    }
}
