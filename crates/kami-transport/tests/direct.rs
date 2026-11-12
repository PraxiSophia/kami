#![cfg(target_os = "linux")]
use kami_identity::{ContactTrust, NativeCustody, TrustTransition, VerificationEvidence};
use kami_key_provider::{KeyError, KeyProvider, ProfileKeyId};
use kami_transport::{Config, Driver, Error};
use std::{
    io::Write,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

#[test]
fn real_native_quic_tcp_and_fallback() {
    const CHILD: &str = "KAMI_TRANSPORT_PRIVATE_BUS";
    if std::env::var_os(CHILD).is_none() {
        let root = tempfile::tempdir().unwrap();
        let result = Command::new("dbus-run-session")
            .arg("--")
            .arg(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "real_native_quic_tcp_and_fallback",
                "--nocapture",
            ])
            .env(CHILD, "1")
            .env("HOME", root.path())
            .env("XDG_DATA_HOME", root.path().join("data"))
            .env("XDG_CONFIG_HOME", root.path().join("config"))
            .env("XDG_RUNTIME_DIR", root.path())
            .env_remove("GNOME_KEYRING_CONTROL")
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
        return;
    }
    let mut daemon = Command::new("gnome-keyring-daemon")
        .args(["--foreground", "--unlock", "--components=secrets"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .spawn()
        .unwrap();
    daemon.stdin.take().unwrap().write_all(b"\n").unwrap();
    let native = NativeCustody::open();
    let profile = ProfileKeyId::new("0123456789abcdef0123456789abcdef").unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while native.load_database_key(&profile).err() != Some(KeyError::Missing) {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(20));
    }
    let a = credentials(&native, &profile);
    let b = credentials(
        &native,
        &ProfileKeyId::new("1123456789abcdef0123456789abcdef").unwrap(),
    );
    let c = credentials(
        &native,
        &ProfileKeyId::new("2123456789abcdef0123456789abcdef").unwrap(),
    );
    let ta = trusted(a.invitation().clone());
    let tb = trusted(b.invitation().clone());
    let tc = trusted(c.invitation().clone());
    tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(async {
        let (da, ha, _) = Driver::new(a, Config { max_dials: 1, dial_timeout: Duration::from_millis(100), capability_ttl: Duration::from_millis(500), ..Config::default() }, vec!["/ip4/127.0.0.1/udp/0/quic-v1".parse().unwrap(), "/ip4/127.0.0.1/tcp/0".parse().unwrap()]).unwrap();
        let (db, hb, _) = Driver::new(b, Config::default(), vec!["/ip4/127.0.0.1/udp/0/quic-v1".parse().unwrap(), "/ip4/127.0.0.1/tcp/0".parse().unwrap()]).unwrap();
        let (dc, hc, _) = Driver::new(c, Config { max_attempts: 1, dial_timeout: Duration::from_millis(100), ..Config::default() }, vec!["/ip4/127.0.0.1/tcp/0".parse().unwrap()]).unwrap();
        let (db, mut prekey_requests) = db.with_prekeys();
        let (stop, stopping) = tokio::sync::watch::channel(false);
        let ja = tokio::spawn(da.run(stopping.clone()));
        let jb = tokio::spawn(db.run(stopping));
        let jc = tokio::spawn(dc.run(stop.subscribe()));
        ha.register(tb.clone()).await.unwrap(); hb.register(ta.clone()).await.unwrap();
        ha.register(tc.clone()).await.unwrap(); hc.register(tb.clone()).await.unwrap(); hb.register(tc.clone()).await.unwrap();
        let addresses = loop {
            let a = hb.local_addresses().await.unwrap();
            if a.len() == 2 { break a; }
            tokio::time::sleep(Duration::from_millis(10)).await;
        };
        let tcp = addresses.iter().find(|a| !a.to_string().contains("quic-v1")).unwrap().clone();
        let caddr = loop { let addresses = hc.local_addresses().await.unwrap(); if let Some(addr) = addresses.first() { break addr.clone(); } tokio::time::sleep(Duration::from_millis(10)).await; };
        // A secure wrong end-peer must terminate, never fall back to a later
        // valid address for the expected identity (also models relay substitution).
        assert!(matches!(ha.dial(*tc.pinned().unwrap().device_key(),vec![tcp.clone(),caddr]).await,Err(Error::Untrusted)));
        tokio::time::sleep(Duration::from_millis(30)).await;
        assert!(matches!(hc.dial(*tb.pinned().unwrap().device_key(),vec!["/ip4/127.0.0.1/udp/1/quic-v1".parse().unwrap(),tcp.clone()]).await,Err(Error::Unavailable)));
        assert!(matches!(ha.dial(*tb.pinned().unwrap().device_key(),vec![tcp.clone();9]).await,Err(Error::InvalidAddress)));
        assert!(matches!(ha.dial([0;32],vec![tcp.clone()]).await,Err(Error::Untrusted)));
        let mut pending = Box::pin(ha.dial(*tb.pinned().unwrap().device_key(),vec!["/ip4/127.0.0.1/udp/1/quic-v1".parse().unwrap()]));
        tokio::select! { _ = tokio::time::sleep(Duration::from_millis(20)) => {}, result = &mut pending => panic!("dial unexpectedly finished: {:?}",result.err()) }
        assert!(matches!(ha.dial(*tc.pinned().unwrap().device_key(),vec![tcp.clone()]).await,Err(Error::Busy)));
        drop(pending); // Cancellation must abort the underlying pending dial.
        tokio::time::sleep(Duration::from_millis(30)).await;
        for quic in [true, false] {
            let addr = addresses.iter().find(|a| a.to_string().contains("quic-v1") == quic).unwrap().clone();
            let cap = ha.dial(*tb.pinned().unwrap().device_key(), vec![addr]).await.unwrap();
            assert!(cap.is_current());
            assert_eq!(cap.authenticated_peer(&tb).unwrap(), tb.pinned().unwrap().binding().peer_id());
            assert_eq!(cap.authenticated_peer(&ta), Err(Error::Untrusted));
            use kami_protocol::prekey_exchange::{Exchange, Query, Failure};
            let query = Exchange::Query(Query {request:[if quic {1} else {2};32],requester:*ta.pinned().unwrap().device_key(),publisher:*tb.pinned().unwrap().device_key(),minimum_generation:1});
            let mut exchange = Box::pin(ha.exchange_prekey(&cap,query.clone()));
            let incoming = tokio::select! { value=prekey_requests.recv()=>value.unwrap(), result=&mut exchange=>panic!("unexpected completion: {:?}",result.err()) };
            assert_eq!(incoming.evidence().inspect(&ta).unwrap().1,ta.pinned().unwrap().binding().peer_id());
            assert!(matches!(ha.exchange_prekey(&cap,query.clone()).await,Err(Error::Busy)));
            incoming.respond(&ta, Exchange::Error {request:*query.request_id(),failure:Failure::Depleted}).unwrap();
            let evidence=exchange.await.unwrap();assert!(evidence.inspect(&tb).is_ok());
            ha.disconnect(*tb.pinned().unwrap().device_key()).await.unwrap();
            assert!(!evidence.is_current());
            assert!(evidence.inspect(&tb).is_err());
            assert!(!cap.is_current());
            tokio::time::sleep(Duration::from_millis(30)).await;
        }
        let tcp = addresses.iter().find(|a| !a.to_string().contains("quic-v1")).unwrap().clone();
        let cap = ha.dial(*tb.pinned().unwrap().device_key(), vec!["/ip4/127.0.0.1/udp/1/quic-v1".parse().unwrap(), tcp]).await.unwrap();
        assert!(cap.is_current());
        ha.revoke_all().await.unwrap();
        assert!(!cap.is_current());
        tokio::time::sleep(Duration::from_millis(30)).await;
        let tcp = addresses.iter().find(|a| !a.to_string().contains("quic-v1")).unwrap().clone();
        let cap = ha.dial(*tb.pinned().unwrap().device_key(), vec![tcp]).await.unwrap();
        let expiry_query=kami_protocol::prekey_exchange::Exchange::Query(kami_protocol::prekey_exchange::Query {request:[9;32],requester:*ta.pinned().unwrap().device_key(),publisher:*tb.pinned().unwrap().device_key(),minimum_generation:1});
        let mut expiring=Box::pin(ha.exchange_prekey(&cap,expiry_query));
        let stalled=tokio::select! { value=prekey_requests.recv()=>value.unwrap(),result=&mut expiring=>panic!("unexpected completion: {:?}",result.err()) };
        tokio::time::sleep(Duration::from_millis(550)).await;
        assert!(!cap.is_current(), "capability expiry must be enforced without telemetry consumption");
        assert!(expiring.await.is_err());
        assert!(!stalled.evidence().is_current());
        assert!(stalled.respond(&ta,kami_protocol::prekey_exchange::Exchange::Error {request:[9;32],failure:kami_protocol::prekey_exchange::Failure::Depleted}).is_err());
        let cap_c = hc.dial(*tb.pinned().unwrap().device_key(),vec![addresses.iter().find(|a| !a.to_string().contains("quic-v1")).unwrap().clone()]).await.unwrap();
        let blocked = tb.transition(TrustTransition::Block).unwrap();
        assert_eq!(hc.register(blocked).await,Err(Error::Untrusted)); assert!(!cap_c.is_current());
        hc.register(tb.clone()).await.unwrap();
        let mut pending = Box::pin(hc.dial(*tb.pinned().unwrap().device_key(),vec!["/ip4/127.0.0.1/udp/1/quic-v1".parse().unwrap()]));
        tokio::select! { _ = tokio::time::sleep(Duration::from_millis(20)) => {}, result = &mut pending => panic!("dial unexpectedly finished: {:?}",result.err()) }
        stop.send_replace(true);
        assert!(matches!(pending.await,Err(Error::Closed)));
        ja.await.unwrap().unwrap(); jb.await.unwrap().unwrap(); jc.await.unwrap().unwrap();
        assert!(!cap.is_current());
    });
    daemon.kill().unwrap();
    daemon.wait().unwrap();
}
fn credentials(
    native: &NativeCustody,
    profile: &ProfileKeyId,
) -> kami_identity::SealedTransportCredentials {
    let fresh = kami_identity::begin_fresh_native(native, profile).unwrap();
    let row = fresh.canonical_row().to_vec();
    kami_identity::complete_fresh_enrollment(native, fresh, &row).unwrap();
    kami_identity::verify_enrollment_for_transport(native, profile, &row)
        .unwrap()
        .materialize(native, &row)
        .unwrap()
}
fn trusted(invitation: kami_identity::Invitation) -> ContactTrust {
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
