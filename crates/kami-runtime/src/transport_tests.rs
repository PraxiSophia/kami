use super::*;

#[cfg(target_os = "linux")]
mod native {
    use super::*;
    use kami_identity::{
        ContactTrust, Invitation, NativeCustody, TrustTransition, VerificationEvidence,
    };
    use kami_key_provider::{KeyError, KeyProvider, ProfileKeyId};
    use std::{
        io::Write,
        process::{Command, Stdio},
        time::{Duration, Instant},
    };
    struct Daemon(std::process::Child);
    impl Drop for Daemon {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    #[test]
    fn supervised_native_transport_preserves_lease_and_never_delivers_payloads() {
        const CHILD: &str = "KAMI_RUNTIME_PRIVATE_BUS";
        if std::env::var_os(CHILD).is_none() {
            let root = tests::directory();
            let output = Command::new("dbus-run-session").arg("--").arg(std::env::current_exe().unwrap())
                .args(["--exact", "transport_tests::native::supervised_native_transport_preserves_lease_and_never_delivers_payloads", "--nocapture"])
                .env(CHILD,"1").env("HOME",root.path()).env("XDG_DATA_HOME",root.path().join("data"))
                .env("XDG_CONFIG_HOME",root.path().join("config")).env("XDG_RUNTIME_DIR",root.path())
                .env_remove("GNOME_KEYRING_CONTROL").output().unwrap();
            assert!(
                output.status.success(),
                "{}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            return;
        }
        let mut daemon = Daemon(
            Command::new("gnome-keyring-daemon")
                .args(["--foreground", "--unlock", "--components=secrets"])
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .spawn()
                .unwrap(),
        );
        daemon.0.stdin.take().unwrap().write_all(b"\n").unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        let profile = ProfileKeyId::new("0123456789abcdef0123456789abcdef").unwrap();
        while NativeCustody::open().load_database_key(&profile).err() != Some(KeyError::Missing) {
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(20));
        }
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async {
                let a = tests::directory();
                let b = tests::directory();
                let mut pb = tests::profile(&b, None);
                pb.key_id = profile;
                let (ea, _) = Engine::start_direct(
                    tests::profile(&a, None),
                    Config {
                        shutdown_timeout: Duration::from_millis(40),
                        ..Config::default()
                    },
                    direct(),
                )
                .unwrap();
                let (eb, _) = Engine::start_direct(pb, Config::default(), direct()).unwrap();
                ea.wait_running().await.unwrap();
                eb.wait_running().await.unwrap();
                let ia = ea.local_invitation().unwrap();
                let ib = eb.local_invitation().unwrap();
                persist(&ea, trusted(ib.clone())).await.unwrap();
                persist(&eb, trusted(ia.clone())).await.unwrap();
                ea.register_contact(*ib.device_key()).await.unwrap();
                eb.register_contact(*ia.device_key()).await.unwrap();
                let addresses = tokio::time::timeout(Duration::from_secs(5), async {
                    loop {
                        let addresses = eb.direct_addresses().await.unwrap();
                        if addresses.len() == 2 {
                            break addresses;
                        }
                        tokio::time::sleep(Duration::from_millis(10)).await;
                    }
                })
                .await
                .unwrap();
                let cap = ea
                    .connect(*ib.device_key(), addresses.clone())
                    .await
                    .unwrap();
                assert!(cap.is_current());
                assert!(kami_storage::ProfileLease::acquire(a.path()).is_err());
                ea.test_work(|storage| {
                    assert!(storage.recover_sends(64).unwrap().is_empty());
                    assert!(storage.pending(64).unwrap().is_empty());
                    Ok(0)
                })
                .await
                .unwrap();
                let request = kami_sender::SendRequest::new(
                    kami_core::MessageId::from_u128(1),
                    kami_core::ConversationId::from_u128(2),
                    kami_core::DeviceId::from_bytes(*ia.device_key()),
                    kami_core::DeviceId::from_bytes(*ib.device_key()),
                    1000,
                    "must not transmit".into(),
                )
                .unwrap();
                assert_eq!(
                    ea.enqueue(request).await,
                    Err(Error::LiveTransportUnavailable)
                );
                let (entered, ready) = tokio::sync::oneshot::channel();
                let (release, wait) = std::sync::mpsc::channel();
                let e = ea.clone();
                let work = tokio::spawn(async move {
                    e.test_work(move |_| {
                        entered.send(()).unwrap();
                        wait.recv().unwrap();
                        Ok(0)
                    })
                    .await
                });
                ready.await.unwrap();
                assert_eq!(
                    tokio::time::timeout(Duration::from_millis(100), ea.health())
                        .await
                        .unwrap()
                        .unwrap(),
                    Lifecycle::Running
                );
                assert_eq!(ea.shutdown().await, ShutdownOutcome::TimedOut);
                assert!(!cap.is_current());
                assert!(kami_storage::ProfileLease::acquire(a.path()).is_err());
                release.send(()).unwrap();
                work.await.unwrap().unwrap();
                ea.wait_stopped().await;
                assert!(kami_storage::ProfileLease::acquire(a.path()).is_ok());
                let mut reopened = tests::profile(&a, None);
                reopened.create = false;
                let (ea, _) = Engine::start_direct(reopened, Config::default(), direct()).unwrap();
                ea.wait_running().await.unwrap();
                assert_eq!(ea.local_invitation().unwrap(), ia);
                let cap = ea
                    .connect(*ib.device_key(), addresses.clone())
                    .await
                    .unwrap();
                let blocked = trusted(ib.clone())
                    .transition(TrustTransition::Block)
                    .unwrap();
                update(&ea, blocked.clone(), kami_storage::TrustState::Blocked).await;
                tokio::time::timeout(Duration::from_secs(5), async {
                    while cap.is_current() {
                        tokio::time::sleep(Duration::from_millis(10)).await;
                    }
                })
                .await
                .unwrap();
                assert!(ea
                    .connect(*ib.device_key(), addresses.clone())
                    .await
                    .is_err());
                let changed = blocked.transition(TrustTransition::Unblock).unwrap();
                update(&ea, changed.clone(), kami_storage::TrustState::Changed).await;
                let evidence = VerificationEvidence::independent_comparison(
                    &changed,
                    changed.candidate().fingerprint(),
                    true,
                )
                .unwrap();
                let verified = changed
                    .transition(TrustTransition::Verify(evidence))
                    .unwrap();
                update(&ea, verified, kami_storage::TrustState::Verified).await;
                let cap = ea.connect(*ib.device_key(), addresses).await.unwrap();
                assert!(ea
                    .test_work(|_| panic!("fatal direct worker test"))
                    .await
                    .is_err());
                ea.wait_stopped().await;
                assert!(!cap.is_current());
                assert_eq!(ea.shutdown().await, ShutdownOutcome::Failed);
                assert_eq!(eb.shutdown().await, ShutdownOutcome::Clean);
            });
    }
    #[test]
    fn manual_input_is_bounded_and_never_accepts_partial_lines() {
        assert_eq!(manual_line(&b"quit\n"[..], 4).unwrap(), "quit");
        assert!(manual_line(&b"quit"[..], 4).is_err());
        assert!(manual_line(&b"12345\n"[..], 4).is_err());
        assert!(manual_line(&b"\xff\n"[..], 4).is_err());
        assert!(manual_line(&b""[..], 4).is_err());
        let mut lines = &b"quit\nnext\n"[..];
        assert_eq!(manual_line(&mut lines, 4).unwrap(), "quit");
        assert_eq!(manual_line(&mut lines, 4).unwrap(), "next");
        for malformed in [
            "".to_owned(),
            "0".into(),
            "gg".into(),
            "AA".into(),
            "00".repeat(513),
            "00".repeat(512),
        ] {
            assert!(invitation_hex(&malformed).is_err());
        }
    }

    // Owner interface: run this exact command in EACH of two terminals:
    // cargo test --locked -p kami-runtime transport_tests::native::manual_endpoint -- --exact --ignored --nocapture
    // Exchange INVITATION hex via copy/paste. Compare LOCAL_FINGERPRINT over an
    // independent trusted channel, never by copying the imported invitation's display.
    // Both endpoints must finish verification before either dials. Keep the other
    // endpoint running; dial quic, disconnect, dial tcp, disconnect, then reconnect.
    // For rejection, enter an intentionally wrong full fingerprint, or REJECT.
    // No two-endpoint wrong-peer claim: the independent three-identity test in
    // kami-transport/tests/direct.rs remains the strict socket-negative fixture.
    #[test]
    #[ignore = "owner-operated Linux native authentication; requires independent human verification"]
    fn manual_endpoint() {
        const CHILD: &str = "KAMI27_MANUAL_PRIVATE_BUS";
        if std::env::var_os(CHILD).is_none() {
            let root = tests::directory();
            let status = Command::new("dbus-run-session")
                .arg("--")
                .arg(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "transport_tests::native::manual_endpoint",
                    "--ignored",
                    "--nocapture",
                ])
                .env(CHILD, "1")
                .env("HOME", root.path())
                .env("XDG_DATA_HOME", root.path().join("data"))
                .env("XDG_CONFIG_HOME", root.path().join("config"))
                .env("XDG_RUNTIME_DIR", root.path())
                .env_remove("GNOME_KEYRING_CONTROL")
                .status()
                .expect("private D-Bus session required");
            assert!(status.success(), "manual endpoint failed closed");
            return;
        }
        // Never provision against the user's bus: only accept our private launcher.
        let home = std::path::PathBuf::from(std::env::var_os("HOME").unwrap());
        assert_eq!(
            std::env::var_os("XDG_RUNTIME_DIR"),
            Some(home.clone().into_os_string())
        );
        assert!(home
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("kami-runtime-"));
        let mut daemon = Daemon(
            Command::new("gnome-keyring-daemon")
                .args(["--foreground", "--unlock", "--components=secrets"])
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .spawn()
                .unwrap(),
        );
        daemon.0.stdin.take().unwrap().write_all(b"\n").unwrap();
        let key_id = ProfileKeyId::new(&format!(
            "{:032x}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
        .unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        while NativeCustody::open().load_database_key(&key_id).err() != Some(KeyError::Missing) {
            assert!(
                Instant::now() < deadline,
                "native Secret Service unavailable/locked"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
        let directory = tests::directory();
        println!(
            "PID={} PROFILE={} KEY_NAMESPACE={}",
            std::process::id(),
            directory.path().display(),
            key_id.as_str()
        );
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .unwrap()
            .block_on(async {
                let (engine, _) = Engine::start_direct(
                    Profile {
                        directory: directory.path().to_owned(),
                        key_id,
                        key_provider: std::sync::Arc::new(NativeCustody::open()),
                        create: true,
                        storage: Default::default(),
                    },
                    Config::default(),
                    direct(),
                )
                .unwrap();
                let result = manual_session(&engine).await;
                assert_eq!(
                    engine.shutdown().await,
                    ShutdownOutcome::Clean,
                    "shutdown failed; do not reuse profile"
                );
                tokio::time::timeout(Duration::from_secs(10), engine.wait_stopped())
                    .await
                    .expect("shutdown timeout");
                drop(
                    kami_storage::ProfileLease::acquire(directory.path())
                        .expect("profile lease not released"),
                );
                println!("CLEAN_SHUTDOWN LEASE_RELEASED");
                result.expect("manual endpoint failed closed (no verification/dial fallback)");
            });
    }

    fn manual_line(reader: impl std::io::Read, max: usize) -> Result<String, String> {
        use std::io::BufRead;
        let mut line = String::new();
        std::io::BufReader::with_capacity(1, reader.take((max + 2) as u64))
            .read_line(&mut line)
            .map_err(|_| "invalid input".to_string())?;
        if !line.ends_with('\n') || line.len() > max + 1 {
            return Err("input missing newline or exceeds bound".into());
        }
        line.pop();
        Ok(line)
    }

    async fn prompt(label: &str, max: usize) -> Result<String, String> {
        println!("{label}");
        std::io::stdout().flush().map_err(|_| "stdout closed")?;
        tokio::task::spawn_blocking(move || manual_line(std::io::stdin().lock(), max))
            .await
            .map_err(|_| "input worker failed")?
    }

    fn public_hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    fn invitation_hex(text: &str) -> Result<Invitation, String> {
        if text.len() > kami_identity::MAX_INVITATION_BYTES * 2
            || !text.len().is_multiple_of(2)
            || !text
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err("invalid bounded invitation hex".into());
        }
        let bytes: Result<Vec<_>, _> = text
            .as_bytes()
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16))
            .collect();
        Invitation::parse_bounded(&bytes.map_err(|_| "invalid hex")?)
            .map_err(|_| "invalid invitation".into())
    }

    async fn manual_session(engine: &Engine) -> Result<(), String> {
        engine
            .wait_running()
            .await
            .map_err(|e| format!("native enrollment failed: {e:?}"))?;
        let local = engine.local_invitation().map_err(|e| format!("{e:?}"))?;
        println!(
            "LOCAL_PEER={}\nLOCAL_FINGERPRINT={}\nINVITATION={}",
            local.binding().peer_id(),
            local.fingerprint(),
            public_hex(&local.to_bytes())
        );
        let addresses = tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                let addresses = engine.direct_addresses().await?;
                if addresses.len() == 2 {
                    break Ok::<_, Error>(addresses);
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .map_err(|_| "listen timeout")?
        .map_err(|e| format!("{e:?}"))?;
        for address in addresses {
            println!("LISTEN={address}");
        }
        let input = prompt(
            "Paste remote INVITATION hex (or quit without trusting):",
            1024,
        )
        .await?;
        if input == "quit" {
            return Ok(());
        }
        let remote = invitation_hex(&input)?;
        if remote.device_key() == local.device_key()
            || remote.binding().peer_id() == local.binding().peer_id()
        {
            return Err("self invitation rejected".into());
        }
        println!(
            "UNTRUSTED_REMOTE_PEER={} (not authenticated)",
            remote.binding().peer_id()
        );
        let contact = ContactTrust::pending(remote.clone());
        let compared = prompt("Enter full remote fingerprint obtained over an INDEPENDENT TRUSTED CHANNEL (or REJECT). Do not copy from the invitation:", 71).await?;
        if compared == "REJECT" {
            return Err("operator rejected; no trust persisted".into());
        }
        let fingerprint = kami_identity::DeviceFingerprint::parse_display(&compared)
            .map_err(|_| "malformed fingerprint")?;
        // Check mismatch BEFORE asking for the affirmative local action.
        if fingerprint != remote.fingerprint() {
            return Err("fingerprint mismatch; no trust persisted".into());
        }
        let action = prompt(
            "After independent full-fingerprint comparison type VERIFY locally, otherwise REJECT:",
            6,
        )
        .await?;
        let evidence =
            VerificationEvidence::independent_comparison(&contact, fingerprint, action == "VERIFY")
                .map_err(|_| "verification rejected; no trust persisted")?;
        let trust = contact
            .transition(TrustTransition::Verify(evidence))
            .map_err(|_| "trust conflict")?;
        persist(engine, trust)
            .await
            .map_err(|e| format!("trust CAS failed: {e:?}"))?;
        let device = *remote.device_key();
        engine
            .register_contact(device)
            .await
            .map_err(|e| format!("{e:?}"))?;
        println!(
            "LOCAL_VERIFIED_DURABLE: wait until BOTH terminals reach this line before dialing"
        );
        let mut capability: Option<kami_transport::AuthenticatedConnection> = None;
        loop {
            let command = prompt(
                "Command: quic | tcp | disconnect | quit (peer terminal must remain running):",
                10,
            )
            .await?;
            match command.as_str() {
                "quit" => return Ok(()),
                "disconnect" => {
                    engine
                        .disconnect(device)
                        .await
                        .map_err(|e| format!("{e:?}"))?;
                    if let Some(cap) = capability.take() {
                        tokio::time::timeout(Duration::from_secs(10), async {
                            while cap.is_current() {
                                tokio::time::sleep(Duration::from_millis(10)).await;
                            }
                        })
                        .await
                        .map_err(|_| "disconnect timeout")?;
                    }
                    println!("DISCONNECTED: old capability revoked");
                }
                "quic" | "tcp" => {
                    if capability.is_some() {
                        return Err("disconnect before selecting another transport".into());
                    }
                    let address =
                        prompt("Paste the remote LISTEN address for that transport:", 128).await?;
                    let expected = if command == "quic" { "/udp/" } else { "/tcp/" };
                    if !address.starts_with("/ip4/127.0.0.1/")
                        || !address.contains(expected)
                        || (command == "quic" && !address.ends_with("/quic-v1"))
                    {
                        return Err("only selected loopback transport is permitted".into());
                    }
                    let address = address.parse().map_err(|_| "invalid multiaddr")?;
                    let cap = engine
                        .connect(device, vec![address])
                        .await
                        .map_err(|e| format!("dial rejected: {e:?}"))?;
                    // Read fresh durable trust via runtime ownership, not a cached invitation.
                    let (tx, rx) = tokio::sync::oneshot::channel();
                    engine
                        .test_work(move |storage| {
                            let row = storage
                                .load_contact(device)
                                .map_err(Error::Storage)?
                                .ok_or(Error::OperationRejected)?;
                            if row.state() != kami_storage::TrustState::Verified {
                                return Err(Error::OperationRejected);
                            }
                            let fresh = ContactTrust::from_persisted(
                                kami_identity::ContactTrustState::Verified,
                                row.generation(),
                                row.pinned(),
                                row.candidate(),
                            )
                            .map_err(|_| Error::OperationRejected)?;
                            tx.send(fresh).map_err(|_| Error::OperationRejected)?;
                            Ok(0)
                        })
                        .await
                        .map_err(|e| format!("{e:?}"))?;
                    let fresh = rx.await.map_err(|_| "fresh trust unavailable")?;
                    let peer = cap
                        .authenticated_peer(&fresh)
                        .map_err(|e| format!("{e:?}"))?;
                    println!("AUTHENTICATED_REMOTE_PEER={peer} TRANSPORT={command}");
                    capability = Some(cap);
                }
                _ => return Err("unknown command".into()),
            }
        }
    }

    fn direct() -> DirectConfig {
        DirectConfig {
            listeners: vec![
                "/ip4/127.0.0.1/udp/0/quic-v1".parse().unwrap(),
                "/ip4/127.0.0.1/tcp/0".parse().unwrap(),
            ],
            limits: Default::default(),
        }
    }
    fn trusted(invitation: Invitation) -> ContactTrust {
        let contact = ContactTrust::pending(invitation);
        let evidence = VerificationEvidence::independent_comparison(
            &contact,
            contact.candidate().fingerprint(),
            true,
        )
        .unwrap();
        contact
            .transition(TrustTransition::Verify(evidence))
            .unwrap()
    }
    async fn persist(engine: &Engine, trust: ContactTrust) -> Result<(), Error> {
        engine
            .test_work(move |storage| {
                let device = *trust.pinned().unwrap().device_key();
                let descriptor = trust.candidate().to_bytes();
                storage
                    .compare_and_swap_contact(
                        None,
                        kami_storage::ContactTrustRow::new(
                            device,
                            kami_storage::TrustState::Pending,
                            1,
                            None,
                            descriptor.clone(),
                        )
                        .unwrap(),
                    )
                    .map_err(Error::Storage)?;
                storage
                    .compare_and_swap_contact(
                        Some(1),
                        kami_storage::ContactTrustRow::new(
                            device,
                            kami_storage::TrustState::Verified,
                            trust.generation(),
                            Some(descriptor.clone()),
                            descriptor,
                        )
                        .unwrap(),
                    )
                    .map_err(Error::Storage)?;
                Ok(0)
            })
            .await
            .map(|_| ())
    }
    async fn update(engine: &Engine, trust: ContactTrust, state: kami_storage::TrustState) {
        engine
            .test_work(move |storage| {
                storage
                    .compare_and_swap_contact(
                        Some(trust.generation() - 1),
                        kami_storage::ContactTrustRow::new(
                            *trust.pinned().unwrap().device_key(),
                            state,
                            trust.generation(),
                            trust.pinned().map(|p| p.to_bytes()),
                            trust.candidate().to_bytes(),
                        )
                        .unwrap(),
                    )
                    .map_err(Error::Storage)?;
                Ok(0)
            })
            .await
            .unwrap();
    }
}

#[tokio::test]
async fn direct_transport_never_adopts_a_marker_only_profile() {
    let dir = tests::directory();
    let (engine, _) = Engine::start(tests::profile(&dir, None), Config::default()).unwrap();
    engine.wait_running().await.unwrap();
    assert_eq!(engine.shutdown().await, ShutdownOutcome::Clean);
    let mut profile = tests::profile(&dir, None);
    profile.create = false;
    let (engine, _) = Engine::start_direct(
        profile,
        Config::default(),
        DirectConfig {
            listeners: vec![],
            limits: Default::default(),
        },
    )
    .unwrap();
    assert!(engine.wait_running().await.is_err());
    engine.wait_stopped().await;
    assert_ne!(engine.snapshot().lifecycle, Lifecycle::Running);
}
