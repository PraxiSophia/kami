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
                let cap = std::sync::Arc::new(cap);
                assert!(cap.is_current());
                // This seam dials on THIS Engine; an external cap is never an input.
                let conversation = kami_core::ConversationId::from_u128(81);
                let operation =
                    crate::ingress::Operation::Claim(kami_core::MessageId::from_u128(82));
                let ingress = ea
                    .ingress_peer(*ib.device_key(), addresses.clone(), conversation, operation)
                    .await
                    .unwrap();
                let expected_local = *ia.device_key();
                let expected_remote = *ib.device_key();
                assert_eq!(
                    ea.test_ingress(ingress, move |storage, local, remote| {
                        assert_eq!(*local.as_bytes(), expected_local);
                        assert_eq!(*remote.as_bytes(), expected_remote);
                        assert!(storage.recover_sends(64).unwrap().is_empty());
                        Ok(7)
                    })
                    .await,
                    Ok(7)
                );
                for (operation, action) in [
                    (
                        crate::ingress::Operation::Receive(kami_core::MessageId::from_u128(82)),
                        crate::ingress::Action::Receive {
                            frame: vec![0; 16],
                            receipt: kami_core::MessageId::from_u128(84),
                        },
                    ),
                    (
                        crate::ingress::Operation::Receipt(kami_core::MessageId::from_u128(82)),
                        crate::ingress::Action::Receipt(vec![0; 16]),
                    ),
                ] {
                    let ingress = ea
                        .ingress_peer(*ib.device_key(), addresses.clone(), conversation, operation)
                        .await
                        .unwrap();
                    assert!(matches!(
                        ea.dispatch_ingress(ingress, action).await,
                        Err(Error::OperationRejected)
                    ));
                }
                assert!(ea
                    .ingress_peer([0; 32], addresses.clone(), conversation, operation)
                    .await
                    .is_err());
                let foreign = ea
                    .ingress_peer(*ib.device_key(), addresses.clone(), conversation, operation)
                    .await
                    .unwrap();
                assert_eq!(
                    eb.test_ingress(foreign, |_, _, _| panic!("foreign Engine authority"))
                        .await,
                    Err(Error::LiveTransportUnavailable)
                );
                let mismatched = ea
                    .ingress_peer(*ib.device_key(), addresses.clone(), conversation, operation)
                    .await
                    .unwrap();
                assert!(matches!(
                    ea.dispatch_ingress(
                        mismatched,
                        crate::ingress::Action::Claim(kami_core::MessageId::from_u128(83))
                    )
                    .await,
                    Err(Error::LiveTransportUnavailable)
                ));
                let disconnected = ea
                    .ingress_peer(*ib.device_key(), addresses.clone(), conversation, operation)
                    .await
                    .unwrap();
                let prekey_request = ea.refresh_prekeys(cap.clone()).await.unwrap();
                ea.retry_prekeys(cap.clone(), prekey_request).await.unwrap();
                ea.disconnect(*ib.device_key()).await.unwrap();
                assert!(!cap.is_current());
                assert_eq!(
                    ea.test_ingress(disconnected, |_, _, _| panic!("disconnected authority"))
                        .await,
                    Err(Error::LiveTransportUnavailable)
                );
                assert!(ea.retry_prekeys(cap.clone(), prekey_request).await.is_err());
                // Both peers explicitly revoke the old connection before TCP admission.
                eb.disconnect(*ia.device_key()).await.unwrap();
                let tcp = addresses
                    .iter()
                    .filter(|a| a.to_string().contains("/tcp/"))
                    .cloned()
                    .collect::<Vec<_>>();
                let cap = std::sync::Arc::new(
                    tokio::time::timeout(Duration::from_secs(5), async {
                        loop {
                            if let Ok(cap) = ea.connect(*ib.device_key(), tcp.clone()).await {
                                break cap;
                            }
                            tokio::time::sleep(Duration::from_millis(20)).await;
                        }
                    })
                    .await
                    .unwrap(),
                );
                // Reconnection may reconcile exact persisted bytes, never relabel them.
                ea.retry_prekeys(cap.clone(), prekey_request).await.unwrap();
                ea.refresh_prekeys(cap.clone()).await.unwrap();
                eb.replenish_prekeys(1).await.unwrap();
                ea.refresh_prekeys(cap.clone()).await.unwrap();
                let tcp_ingress = ea
                    .ingress_peer(*ib.device_key(), tcp.clone(), conversation, operation)
                    .await
                    .unwrap();
                assert_eq!(ea.test_ingress(tcp_ingress, |_, _, _| Ok(1)).await, Ok(1));
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
                ingress_security_cases(&eb, &ib).await;
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

    #[test]
    fn manual_retry_requires_exact_nonzero_lowercase_request_id() {
        let request = [0x12; 32];
        assert_eq!(manual_request_id(&public_hex(&request)).unwrap(), request);
        for invalid in [
            "".into(),
            "00".repeat(32),
            "12".repeat(31),
            "12".repeat(33),
            "AB".repeat(32),
            format!("{} ", "12".repeat(32)),
        ] {
            assert!(manual_request_id(&invalid).is_err());
        }
    }

    #[test]
    fn manual_prekey_status_is_bounded_and_not_a_durable_status_api() {
        assert_eq!(manual_prekey_status(&None), "PREKEY_STATUS session-local unavailable (no observed durable query); publisher-side/historical status unavailable");
        // Formatting-only values never enter Engine or manufacture live evidence.
        let mut observed = crate::engine::ManualPrekeyObservation {
            request: [1; 32],
            peer: "formatting-only".into(),
            device: [2; 32],
            retained: None,
            reconciled: false,
        };
        let query = manual_prekey_status(&Some(observed.clone()));
        assert!(query
            .contains("SELECTION=unobserved FRESHNESS=unknown RETENTION_BEFORE_ACK=unobserved"));
        assert!(query.contains("PUBLICATION_ACK_RESULT=unobserved-or-indeterminate"));
        assert!(!query.contains("SIGNED_REFERENCE="));
        observed.retained = Some(crate::engine::ManualPrekeySelection {
            kind: kami_identity::OlmPrekeyKind::OneTime,
            reference: vec![0xab; kami_identity::MAX_OLM_PREKEY_REFERENCE_BYTES],
            account_generation: 1,
            bundle_generation: 2,
            binding_generation: 3,
            not_before_ms: 4,
            expires_at_ms: 5,
        });
        let retained = manual_prekey_status(&Some(observed.clone()));
        assert!(retained.contains("FRESHNESS=valid-at-retention RETENTION_BEFORE_ACK=committed"));
        assert!(retained.contains("PUBLICATION_ACK_RESULT=unobserved-or-indeterminate"));
        assert!(retained.contains(&format!(
            "SIGNED_REFERENCE={}",
            "ab".repeat(kami_identity::MAX_OLM_PREKEY_REFERENCE_BYTES)
        )));
        observed.reconciled = true;
        let completed = manual_prekey_status(&Some(observed));
        assert!(completed.contains("PUBLICATION_ACK_RESULT=exact-result-reconciled"));
        assert!(completed
            .ends_with("(not current authority, publisher-side readback, or DEVICE_DELIVERED)"));
        assert!(completed.len() < 2048);
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

    fn manual_request_id(text: &str) -> Result<[u8; 32], String> {
        if text.len() != 64
            || !text
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err("request ID must be exactly 64 lowercase hex characters".into());
        }
        let mut request = [0; 32];
        for (out, pair) in request.iter_mut().zip(text.as_bytes().as_chunks::<2>().0) {
            *out = u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap();
        }
        if request == [0; 32] {
            return Err("zero request ID rejected".into());
        }
        Ok(request)
    }

    fn manual_prekey_status(observed: &Option<crate::engine::ManualPrekeyObservation>) -> String {
        let Some(observed) = observed else {
            return "PREKEY_STATUS session-local unavailable (no observed durable query); publisher-side/historical status unavailable".into();
        };
        let mut output = format!(
            "PREKEY_STATUS session-local last-observed REQUEST_ID={} AUTHENTICATED_REMOTE_PEER={} VERIFIED_REMOTE_DEVICE={} QUERY=durable",
            public_hex(&observed.request), observed.peer, public_hex(&observed.device)
        );
        if let Some(selection) = &observed.retained {
            output.push_str(&format!(
                " SIGNED_KIND={:?} SIGNED_REFERENCE={} ACCOUNT_GENERATION={} BUNDLE_GENERATION={} BINDING_GENERATION={} NOT_BEFORE_MS={} EXPIRES_AT_MS={} FRESHNESS=valid-at-retention RETENTION_BEFORE_ACK=committed",
                selection.kind, public_hex(&selection.reference), selection.account_generation,
                selection.bundle_generation, selection.binding_generation, selection.not_before_ms,
                selection.expires_at_ms
            ));
        } else {
            output.push_str(
                " SELECTION=unobserved FRESHNESS=unknown RETENTION_BEFORE_ACK=unobserved",
            );
        }
        output.push_str(if observed.reconciled {
            " PUBLICATION_ACK_RESULT=exact-result-reconciled"
        } else {
            " PUBLICATION_ACK_RESULT=unobserved-or-indeterminate"
        });
        output.push_str(" (not current authority, publisher-side readback, or DEVICE_DELIVERED)");
        output
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
        let mut capability: Option<std::sync::Arc<kami_transport::AuthenticatedConnection>> = None;
        let mut last_attempt = "none";
        loop {
            let command = prompt(
                "Command: quic | tcp | disconnect | prekey | prekey-status | retry <64-lowercase-hex-request-id> | quit (peer terminal must remain running):",
                70,
            )
            .await?;
            match command.as_str() {
                "quit" => return Ok(()),
                "prekey-status" => {
                    println!("PREKEY_LAST_ATTEMPT={last_attempt}");
                    println!(
                        "{}",
                        manual_prekey_status(&engine.manual_prekey_observation())
                    );
                }
                "prekey" => {
                    let Some(cap) = &capability else {
                        println!("PREKEY_REJECTED no locally dialed authenticated capability; connect first");
                        continue;
                    };
                    match engine.refresh_prekeys(cap.clone()).await {
                        Ok(_) => last_attempt = "completed",
                        Err(error) => {
                            last_attempt = "failed-or-indeterminate";
                            println!("PREKEY_ATTEMPT_FAILED {error:?}; no success inferred; retain request ID for exact retry");
                        }
                    }
                    println!("PREKEY_LAST_ATTEMPT={last_attempt}");
                    println!(
                        "{}",
                        manual_prekey_status(&engine.manual_prekey_observation())
                    );
                }
                retry if retry.starts_with("retry ") => {
                    let request = match manual_request_id(&retry[6..]) {
                        Ok(request) => request,
                        Err(error) => {
                            println!("PREKEY_REJECTED {error}; no network operation");
                            continue;
                        }
                    };
                    let Some(cap) = &capability else {
                        println!("PREKEY_REJECTED no locally dialed authenticated capability; reconnect first");
                        continue;
                    };
                    match engine.retry_prekeys(cap.clone(), request).await {
                        Ok(()) => last_attempt = "completed",
                        Err(error) => {
                            last_attempt = "failed-or-indeterminate";
                            println!("PREKEY_ATTEMPT_FAILED {error:?}; exact durable retry only, no new query substituted");
                        }
                    }
                    println!("PREKEY_LAST_ATTEMPT={last_attempt}");
                    println!(
                        "{}",
                        manual_prekey_status(&engine.manual_prekey_observation())
                    );
                }
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
                    capability = Some(std::sync::Arc::new(cap));
                }
                _ => return Err("unknown command".into()),
            }
        }
    }

    async fn native_endpoint(
        engine: &Engine,
        key_id: ProfileKeyId,
    ) -> (vodozemac::olm::Account, kami_e2ee::Endpoint) {
        let mut account = vodozemac::olm::Account::new();
        account.generate_one_time_keys(1);
        let invitation = engine.local_invitation().unwrap();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        let data = kami_identity::OlmBindingData {
            publisher_peer: invitation.binding().peer_id(),
            olm_ed25519: *account.ed25519_key().as_bytes(),
            olm_curve25519: *account.curve25519_key().as_bytes(),
            prekey_kind: kami_identity::OlmPrekeyKind::OneTime,
            prekey_reference: vec![1],
            prekey_curve25519: *account.one_time_keys().values().next().unwrap().as_bytes(),
            binding_generation: 1,
            bundle_generation: 1,
            not_before_ms: now - 1000,
            expires_at_ms: now + 60_000,
        };
        let (tx, rx) = tokio::sync::oneshot::channel();
        engine
            .test_work(move |storage| {
                let row = storage.enrollment_snapshot().unwrap().unwrap();
                let custody = NativeCustody::open();
                let verified =
                    kami_identity::verify_enrollment_for_transport(&custody, &key_id, &row)
                        .unwrap();
                let proof = custody
                    .create_olm_binding(&verified, &row, invitation.binding(), data.clone())
                    .unwrap();
                tx.send(kami_e2ee::Endpoint::new(
                    kami_core::DeviceId::from_bytes(*invitation.device_key()),
                    invitation.binding().clone(),
                    proof,
                    data,
                ))
                .unwrap_or_else(|_| panic!("endpoint receiver closed"));
                Ok(0)
            })
            .await
            .unwrap();
        (account, rx.await.unwrap())
    }
    async fn ingress_security_cases(server: &Engine, invitation: &Invitation) {
        use crate::ingress::{Action, Operation};
        use kami_core::{ConversationId, MessageId};
        let addresses = server.direct_addresses().await.unwrap();
        for case in 0..12 {
            let directory = tests::directory();
            let mut profile = tests::profile(&directory, None);
            profile.key_id = ProfileKeyId::new(&format!("{:032x}", 100 + case)).unwrap();
            let mut direct = direct();
            if case == 0 {
                direct.limits.capability_ttl = Duration::from_secs(1);
            }
            let (mut engine, _) = Engine::start_direct(profile, Config::default(), direct).unwrap();
            engine.wait_running().await.unwrap();
            let local = engine.local_invitation().unwrap();
            persist(&engine, trusted(invitation.clone())).await.unwrap();
            persist(server, trusted(local.clone())).await.unwrap();
            server.register_contact(*local.device_key()).await.unwrap();
            let conversation = ConversationId::from_u128(91);
            let operation = Operation::Claim(MessageId::from_u128(92));
            if case == 6 {
                assert_eq!(engine.shutdown().await, ShutdownOutcome::Clean);
                engine.wait_stopped().await;
                let mut reopened = tests::profile(&directory, None);
                reopened.key_id = ProfileKeyId::new(&format!("{:032x}", 100 + case)).unwrap();
                reopened.create = false;
                engine = Engine::start_direct(reopened, Config::default(), self::direct())
                    .unwrap()
                    .0;
                engine.wait_running().await.unwrap();
                assert_eq!(
                    engine.recovery_status().await.unwrap(),
                    kami_storage::RecoveryStatus::Disabled
                );
            }
            let peer = engine
                .ingress_peer(
                    *invitation.device_key(),
                    addresses.clone(),
                    conversation,
                    operation,
                )
                .await
                .unwrap();
            match case {
                0 => {
                    tokio::time::sleep(Duration::from_millis(1100)).await;
                    assert_eq!(
                        engine
                            .test_ingress(peer, |_, _, _| panic!("expired authority"))
                            .await,
                        Err(Error::LiveTransportUnavailable)
                    );
                }
                1 => {
                    // Durable changes are authoritative even before driver refresh.
                    let old = engine
                        .ingress_peer(
                            *invitation.device_key(),
                            addresses.clone(),
                            conversation,
                            operation,
                        )
                        .await
                        .unwrap();
                    let trust = trusted(invitation.clone());
                    let blocked = trust.transition(TrustTransition::Block).unwrap();
                    update(&engine, blocked.clone(), kami_storage::TrustState::Blocked).await;
                    assert_eq!(
                        engine
                            .test_ingress(peer, |_, _, _| panic!("blocked authority"))
                            .await,
                        Err(Error::LiveTransportUnavailable)
                    );
                    let changed = blocked.transition(TrustTransition::Unblock).unwrap();
                    update(&engine, changed.clone(), kami_storage::TrustState::Changed).await;
                    let evidence = VerificationEvidence::independent_comparison(
                        &changed,
                        changed.candidate().fingerprint(),
                        true,
                    )
                    .unwrap();
                    let verified = changed
                        .transition(TrustTransition::Verify(evidence))
                        .unwrap();
                    update(&engine, verified, kami_storage::TrustState::Verified).await;
                    assert_eq!(
                        engine
                            .test_ingress(old, |_, _, _| panic!("old generation revived"))
                            .await,
                        Err(Error::LiveTransportUnavailable)
                    );
                    let new = engine
                        .ingress_peer(
                            *invitation.device_key(),
                            addresses.clone(),
                            conversation,
                            operation,
                        )
                        .await
                        .unwrap();
                    assert_eq!(engine.test_ingress(new, |_, _, _| Ok(1)).await, Ok(1));
                    let wrong_pin = engine
                        .ingress_peer(
                            *invitation.device_key(),
                            addresses.clone(),
                            conversation,
                            operation,
                        )
                        .await
                        .unwrap();
                    let wrong = local.to_bytes();
                    let remote = *invitation.device_key();
                    engine
                        .test_work(move |storage| {
                            let row = storage.load_contact(remote).unwrap().unwrap();
                            storage
                                .compare_and_swap_contact(
                                    Some(row.generation()),
                                    kami_storage::ContactTrustRow::new(
                                        remote,
                                        kami_storage::TrustState::Verified,
                                        row.generation() + 1,
                                        Some(wrong.clone()),
                                        wrong,
                                    )
                                    .unwrap(),
                                )
                                .unwrap();
                            Ok(0)
                        })
                        .await
                        .unwrap();
                    assert_eq!(
                        engine
                            .test_ingress(wrong_pin, |_, _, _| panic!("wrong device/pin authority"))
                            .await,
                        Err(Error::LiveTransportUnavailable)
                    );
                }
                2 => {
                    let (entered, ready) = tokio::sync::oneshot::channel();
                    let (release, wait) = std::sync::mpsc::channel();
                    let e = engine.clone();
                    let blocked = tokio::spawn(async move {
                        e.test_work(move |_| {
                            entered.send(()).unwrap();
                            wait.recv().unwrap();
                            Ok(0)
                        })
                        .await
                    });
                    ready.await.unwrap();
                    let e = engine.clone();
                    let queued = tokio::spawn(async move {
                        e.test_ingress(peer, |_, _, _| panic!("queued revoked authority"))
                            .await
                    });
                    tokio::time::timeout(Duration::from_secs(5), async {
                        while engine.snapshot().active_operations < 2 {
                            tokio::task::yield_now().await;
                        }
                    })
                    .await
                    .unwrap();
                    engine.disconnect(*invitation.device_key()).await.unwrap();
                    release.send(()).unwrap();
                    blocked.await.unwrap().unwrap();
                    assert_eq!(queued.await.unwrap(), Err(Error::LiveTransportUnavailable));
                }
                3 | 4 => {
                    let (entered, ready) = tokio::sync::oneshot::channel();
                    let (release, wait) = std::sync::mpsc::channel();
                    let e = engine.clone();
                    let caller = tokio::spawn(async move {
                        e.test_ingress(peer, move |storage, _, remote| {
                            // A real durable mutation followed by loss of transport/reply.
                            let row = storage.load_contact(*remote.as_bytes()).unwrap().unwrap();
                            storage
                                .compare_and_swap_contact(
                                    Some(row.generation()),
                                    kami_storage::ContactTrustRow::new(
                                        *remote.as_bytes(),
                                        row.state(),
                                        row.generation() + 1,
                                        row.pinned().map(|p| p.to_vec()),
                                        row.candidate().to_vec(),
                                    )
                                    .unwrap(),
                                )
                                .unwrap();
                            entered.send(()).unwrap();
                            wait.recv().unwrap();
                            Ok(1)
                        })
                        .await
                    });
                    ready.await.unwrap();
                    if case == 3 {
                        engine.disconnect(*invitation.device_key()).await.unwrap();
                    } else {
                        caller.abort();
                    }
                    release.send(()).unwrap();
                    if case == 3 {
                        assert_eq!(caller.await.unwrap(), Err(Error::Indeterminate));
                    } else {
                        assert!(caller.await.unwrap_err().is_cancelled());
                    }
                    engine.wait_stopped().await;
                    let mut reopened = tests::profile(&directory, None);
                    reopened.key_id = ProfileKeyId::new(&format!("{:032x}", 100 + case)).unwrap();
                    reopened.create = false;
                    let (again, _) =
                        Engine::start_direct(reopened, Config::default(), self::direct()).unwrap();
                    again.wait_running().await.unwrap();
                    let remote = *invitation.device_key();
                    assert_eq!(
                        again
                            .test_work(move |storage| {
                                assert_eq!(
                                    storage.load_contact(remote).unwrap().unwrap().generation(),
                                    3
                                );
                                assert!(storage.recover_sends(64).unwrap().is_empty());
                                Ok(1)
                            })
                            .await,
                        Ok(1)
                    );
                    assert_eq!(again.shutdown().await, ShutdownOutcome::Clean);
                }
                5 => {
                    assert_eq!(
                        engine
                            .test_ingress(peer, |_, _, _| panic!("admitted ingress panic"))
                            .await,
                        Err(Error::Indeterminate)
                    );
                    engine.wait_stopped().await;
                }
                6 | 7 => {
                    if case == 7 {
                        let fresh = kami_identity::begin_fresh_native(
                            &NativeCustody::open(),
                            &ProfileKeyId::new("00000000000000000000000000000777").unwrap(),
                        )
                        .unwrap();
                        engine
                            .test_work(move |storage| {
                                assert!(storage.enroll_fresh(fresh).is_err());
                                assert_eq!(
                                    storage.recovery_status().unwrap(),
                                    kami_storage::RecoveryStatus::Quarantined
                                );
                                Ok(0)
                            })
                            .await
                            .unwrap();
                    }
                    assert_eq!(
                        engine
                            .test_ingress(peer, |_, _, _| panic!("disabled recovery authority"))
                            .await,
                        Err(Error::Storage(
                            kami_storage::StorageError::EnrollmentRequired
                        ))
                    );
                }
                8 => {
                    // Prepare a test-only quarantined artifact using purpose-limited
                    // native signatures, never native key export or a live session.
                    let (alice, a) = native_endpoint(
                        &engine,
                        ProfileKeyId::new(&format!("{:032x}", 100 + case)).unwrap(),
                    )
                    .await;
                    let (_, b) = native_endpoint(
                        server,
                        ProfileKeyId::new("0123456789abcdef0123456789abcdef").unwrap(),
                    )
                    .await;
                    let trust = trusted(invitation.clone());
                    let (sent, stored) = tokio::sync::oneshot::channel();
                    let local_id = kami_core::DeviceId::from_bytes(*local.device_key());
                    engine
                        .test_work(move |storage| {
                            let now = std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap()
                                .as_millis() as u64;
                            let source = kami_e2ee::QuarantinedSession::outbound(
                                alice,
                                a,
                                b,
                                conversation,
                                &trust,
                                &trust.pinned().unwrap().binding().peer_id(),
                                now,
                            )
                            .unwrap();
                            let request = kami_sender::SendRequest::new(
                                MessageId::from_u128(92),
                                conversation,
                                local_id,
                                kami_core::DeviceId::from_bytes(
                                    *trust.pinned().unwrap().device_key(),
                                ),
                                now,
                                "test-only prepared artifact".into(),
                            )
                            .unwrap();
                            let kami_sender::EnqueueOutcome::Committed { message, .. } =
                                kami_sender::Coordinator::new(storage)
                                    .enqueue(
                                        request,
                                        &source,
                                        &trust,
                                        &trust.pinned().unwrap().binding().peer_id(),
                                        now as i64,
                                    )
                                    .unwrap()
                            else {
                                panic!("fixture did not commit")
                            };
                            sent.send(message.frame().to_vec()).unwrap();
                            Ok(0)
                        })
                        .await
                        .unwrap();
                    let expected = stored.await.unwrap();
                    let outcome = engine
                        .dispatch_ingress(peer, Action::Claim(MessageId::from_u128(92)))
                        .await
                        .unwrap();
                    let crate::ingress::Outcome::Claim(Some(lease)) = outcome else {
                        panic!("real claim must return the exact prepared artifact")
                    };
                    assert_eq!(lease.message().frame(), expected);
                    assert_eq!(
                        engine
                            .test_work(move |storage| {
                                let row = storage
                                    .sender_record(
                                        kami_core::DeviceId::from_bytes(*local.device_key()),
                                        MessageId::from_u128(92),
                                    )
                                    .unwrap();
                                assert_eq!(row.frame(), expected);
                                assert_eq!(row.attempts(), 1);
                                assert_eq!(row.state(), kami_storage::SendState::InFlight);
                                Ok(1)
                            })
                            .await,
                        Ok(1)
                    );
                    engine.disconnect(*invitation.device_key()).await.unwrap();
                    // A genuine listener with the wrong secure Peer-ID is not
                    // authority for the intended, durably verified recipient.
                    assert!(engine
                        .ingress_peer(
                            *invitation.device_key(),
                            engine.direct_addresses().await.unwrap(),
                            conversation,
                            operation
                        )
                        .await
                        .is_err());
                }
                9 | 10 => {
                    let remote = *invitation.device_key();
                    let replacement = local.to_bytes();
                    engine
                        .test_work(move |storage| {
                            let row = storage.load_contact(remote).unwrap().unwrap();
                            let (state, pin, candidate) = if case == 9 {
                                (
                                    kami_storage::TrustState::Pending,
                                    None,
                                    row.candidate().to_vec(),
                                )
                            } else {
                                (
                                    kami_storage::TrustState::Changed,
                                    row.pinned().map(|p| p.to_vec()),
                                    replacement,
                                )
                            };
                            storage
                                .compare_and_swap_contact(
                                    Some(row.generation()),
                                    kami_storage::ContactTrustRow::new(
                                        remote,
                                        state,
                                        row.generation() + 1,
                                        pin,
                                        candidate,
                                    )
                                    .unwrap(),
                                )
                                .unwrap();
                            Ok(0)
                        })
                        .await
                        .unwrap();
                    assert_eq!(
                        engine
                            .test_ingress(peer, |_, _, _| panic!(
                                "unverified/replaced identity authority"
                            ))
                            .await,
                        Err(Error::LiveTransportUnavailable)
                    );
                }
                11 => {
                    let (entered, ready) = tokio::sync::oneshot::channel();
                    let (release, wait) = std::sync::mpsc::channel();
                    let e = engine.clone();
                    let blocked = tokio::spawn(async move {
                        e.test_work(move |_| {
                            entered.send(()).unwrap();
                            wait.recv().unwrap();
                            Ok(0)
                        })
                        .await
                    });
                    ready.await.unwrap();
                    let e = engine.clone();
                    let queued = tokio::spawn(async move {
                        e.test_ingress(peer, |_, _, _| panic!("queued shutdown authority"))
                            .await
                    });
                    tokio::time::timeout(Duration::from_secs(5), async {
                        while engine.snapshot().active_operations < 2 {
                            tokio::task::yield_now().await;
                        }
                    })
                    .await
                    .unwrap();
                    engine.request_shutdown();
                    release.send(()).unwrap();
                    blocked.await.unwrap().unwrap();
                    assert!(matches!(
                        queued.await.unwrap(),
                        Err(Error::LiveTransportUnavailable | Error::Indeterminate)
                    ));
                }
                _ => unreachable!(),
            }
            if !matches!(
                engine.snapshot().lifecycle,
                Lifecycle::Stopped | Lifecycle::Failed
            ) {
                let _ = engine.shutdown().await;
            }
            engine.wait_stopped().await;
            assert!(kami_storage::ProfileLease::acquire(directory.path()).is_ok());
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
