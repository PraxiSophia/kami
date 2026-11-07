use super::*;
use kami_key_provider::{test_provider::TestKeyProvider, ProfileKeyId};
use std::{
    sync::{mpsc as std_mpsc, Arc},
    time::Duration,
};

pub(crate) fn directory() -> tempfile::TempDir {
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
    dir
}
pub(crate) fn profile(
    dir: &tempfile::TempDir,
    provider: Option<Arc<dyn kami_key_provider::KeyProvider>>,
) -> Profile {
    let key_id = ProfileKeyId::new("aabbccddeeff00112233445566778899").unwrap();
    Profile {
        directory: dir.path().to_owned(),
        key_id: key_id.clone(),
        key_provider: provider
            .unwrap_or_else(|| Arc::new(TestKeyProvider::seeded(key_id, [71; 32]))),
        create: true,
        storage: Default::default(),
    }
}
#[tokio::test]
async fn blocked_storage_is_bounded_and_health_and_shutdown_stay_responsive() {
    let dir = directory();
    let config = Config {
        blocking_capacity: 1,
        shutdown_timeout: Duration::from_millis(30),
        ..Config::default()
    };
    struct MockNetwork;
    impl crate::relay::RelayAdapter for MockNetwork {
        fn execute(&mut self, _: crate::relay::Action) -> crate::relay::RelayFuture<'_> {
            Box::pin(async {
                tokio::time::sleep(Duration::from_millis(1)).await;
                Ok(Duration::from_millis(20))
            })
        }
    }
    let relay = crate::relay::RelayConfig {
        renewal_margin: Duration::from_millis(5),
        operation_timeout: Duration::from_millis(10),
        ..Default::default()
    };
    let (engine, mut events) =
        Engine::start_with_relay(profile(&dir, None), config, relay, Box::new(MockNetwork))
            .unwrap();
    engine.wait_running().await.unwrap();
    let (entered, ready) = tokio::sync::oneshot::channel();
    let (release, wait) = std_mpsc::channel();
    let e = engine.clone();
    let work = tokio::spawn(async move {
        e.test_work(move |storage| {
            entered.send(()).unwrap();
            wait.recv().unwrap();
            storage.cipher_version().map_err(Error::Storage)?;
            Ok(17)
        })
        .await
    });
    ready.await.unwrap();
    assert_eq!(engine.recovered().await, Err(Error::Busy));
    assert_eq!(
        tokio::time::timeout(Duration::from_millis(100), engine.health())
            .await
            .unwrap()
            .unwrap(),
        Lifecycle::Running
    );
    assert_eq!(engine.snapshot().active_operations, 1);
    tokio::time::timeout(Duration::from_millis(250), async {
        while events.recv().await
            != Some(Event::Relay(crate::relay::RelayEvent::Attempted(
                crate::relay::Action::Renew,
            )))
        {}
    })
    .await
    .unwrap();
    assert_eq!(engine.shutdown().await, ShutdownOutcome::TimedOut);
    assert_eq!(engine.snapshot().lifecycle, Lifecycle::Draining);
    assert!(kami_storage::ProfileLease::acquire(dir.path()).is_err());
    release.send(()).unwrap();
    assert_eq!(work.await.unwrap(), Ok(17));
    engine.wait_stopped().await;
    assert!(kami_storage::ProfileLease::acquire(dir.path()).is_ok());
    assert_eq!(engine.snapshot().shutdown, Some(ShutdownOutcome::TimedOut));
}
#[tokio::test]
async fn cancelled_caller_does_not_cancel_mutation_or_release_profile() {
    let dir = directory();
    let (engine, _) = Engine::start(profile(&dir, None), Config::default()).unwrap();
    engine.wait_running().await.unwrap();
    let (entered, ready) = tokio::sync::oneshot::channel();
    let (release, wait) = std_mpsc::channel();
    let e = engine.clone();
    let caller = tokio::spawn(async move {
        e.test_work(move |_| {
            entered.send(()).unwrap();
            wait.recv().unwrap();
            Ok(1)
        })
        .await
    });
    ready.await.unwrap();
    caller.abort();
    assert_eq!(engine.snapshot().active_operations, 1);
    assert!(kami_storage::ProfileLease::acquire(dir.path()).is_err());
    release.send(()).unwrap();
    assert_eq!(engine.shutdown().await, ShutdownOutcome::Clean);
    engine.wait_stopped().await;
}
#[tokio::test]
async fn worker_panic_is_fatal_and_never_reports_clean_shutdown() {
    let dir = directory();
    let (engine, _) = Engine::start(profile(&dir, None), Config::default()).unwrap();
    engine.wait_running().await.unwrap();
    assert_eq!(
        engine.test_work(|_| panic!("injected worker fault")).await,
        Err(Error::WorkerFailed)
    );
    engine.wait_stopped().await;
    assert_eq!(engine.snapshot().lifecycle, Lifecycle::Failed);
    assert_eq!(engine.shutdown().await, ShutdownOutcome::Failed);
    assert!(kami_storage::ProfileLease::acquire(dir.path()).is_ok());
}
struct DelayedKey {
    entered: std::sync::Mutex<Option<tokio::sync::oneshot::Sender<()>>>,
    release: std::sync::Mutex<std_mpsc::Receiver<()>>,
    inner: TestKeyProvider,
}
impl kami_key_provider::KeyProvider for DelayedKey {
    fn load_database_key(
        &self,
        id: &ProfileKeyId,
    ) -> Result<kami_key_provider::DatabaseKey, kami_key_provider::KeyError> {
        self.entered
            .lock()
            .unwrap()
            .take()
            .unwrap()
            .send(())
            .unwrap();
        self.release.lock().unwrap().recv().unwrap();
        self.inner.load_database_key(id)
    }
}
#[tokio::test]
async fn delayed_key_initialization_does_not_block_control_or_shutdown() {
    let dir = directory();
    let (entered, ready) = tokio::sync::oneshot::channel();
    let (release, wait) = std_mpsc::channel();
    let provider = DelayedKey {
        entered: std::sync::Mutex::new(Some(entered)),
        release: std::sync::Mutex::new(wait),
        inner: TestKeyProvider::seeded(
            ProfileKeyId::new("aabbccddeeff00112233445566778899").unwrap(),
            [71; 32],
        ),
    };
    let (engine, _) = Engine::start(
        profile(&dir, Some(Arc::new(provider))),
        Config {
            shutdown_timeout: Duration::from_millis(30),
            ..Config::default()
        },
    )
    .unwrap();
    ready.await.unwrap();
    assert_eq!(engine.health().await.unwrap(), Lifecycle::Reconciling);
    assert_eq!(engine.recovered().await, Err(Error::NotRunning));
    assert_eq!(engine.shutdown().await, ShutdownOutcome::TimedOut);
    assert!(kami_storage::ProfileLease::acquire(dir.path()).is_err());
    release.send(()).unwrap();
    engine.wait_stopped().await;
    assert_eq!(engine.snapshot().lifecycle, Lifecycle::Stopped);
}
#[tokio::test]
async fn shutdown_waits_for_supervised_relay_release_before_reporting_clean() {
    struct Relay;
    impl crate::relay::RelayAdapter for Relay {
        fn execute(&mut self, action: crate::relay::Action) -> crate::relay::RelayFuture<'_> {
            Box::pin(async move {
                if action == crate::relay::Action::Release {
                    tokio::time::sleep(Duration::from_millis(80)).await;
                }
                Ok(Duration::from_secs(20))
            })
        }
    }
    let dir = directory();
    let (engine, mut events) = Engine::start_with_relay(
        profile(&dir, None),
        Config {
            shutdown_timeout: Duration::from_millis(20),
            ..Config::default()
        },
        crate::relay::RelayConfig::default(),
        Box::new(Relay),
    )
    .unwrap();
    engine.wait_running().await.unwrap();
    while events.recv().await != Some(Event::Relay(crate::relay::RelayEvent::Active)) {}
    assert_eq!(engine.shutdown().await, ShutdownOutcome::TimedOut);
    assert_eq!(engine.snapshot().lifecycle, Lifecycle::Draining);
    engine.wait_stopped().await;
}

#[tokio::test]
async fn worker_failure_stops_admission_while_relay_child_finishes() {
    struct Relay;
    impl crate::relay::RelayAdapter for Relay {
        fn execute(&mut self, action: crate::relay::Action) -> crate::relay::RelayFuture<'_> {
            Box::pin(async move {
                if action == crate::relay::Action::Release {
                    tokio::time::sleep(Duration::from_millis(40)).await;
                }
                Ok(Duration::from_secs(20))
            })
        }
    }
    let dir = directory();
    let (engine, mut events) = Engine::start_with_relay(
        profile(&dir, None),
        Config::default(),
        crate::relay::RelayConfig::default(),
        Box::new(Relay),
    )
    .unwrap();
    engine.wait_running().await.unwrap();
    while events.recv().await != Some(Event::Relay(crate::relay::RelayEvent::Active)) {}
    assert_eq!(
        engine.test_work(|_| panic!("injected fatal failure")).await,
        Err(Error::WorkerFailed)
    );
    assert_eq!(engine.snapshot().lifecycle, Lifecycle::Draining);
    assert!(matches!(engine.recovered().await, Err(Error::Closed)));
    assert_eq!(engine.shutdown().await, ShutdownOutcome::Failed);
    engine.wait_stopped().await;
}

#[tokio::test(start_paused = true)]
async fn retry_scan_timer_runs_without_claiming_or_creating_delivery() {
    let dir = directory();
    let (engine, mut events) = Engine::start(profile(&dir, None), Config::default()).unwrap();
    engine.wait_running().await.unwrap();
    tokio::time::advance(Duration::from_secs(1)).await;
    loop {
        if events.recv().await == Some(Event::RetryReady(0)) {
            break;
        }
    }
    assert_eq!(engine.recovered().await, Ok(0));
    tokio::time::resume();
    assert_eq!(engine.shutdown().await, ShutdownOutcome::Clean);
}

#[tokio::test]
async fn command_queue_backpressure_cannot_starve_out_of_band_shutdown() {
    use std::{future::Future, task::Poll};
    let dir = directory();
    let (engine, _) = Engine::start(
        profile(&dir, None),
        Config {
            command_capacity: 1,
            ..Config::default()
        },
    )
    .unwrap();
    let mut first = std::pin::pin!(engine.health());
    std::future::poll_fn(|cx| {
        assert!(first.as_mut().poll(cx).is_pending());
        Poll::Ready(())
    })
    .await;
    assert_eq!(engine.health().await, Err(Error::Busy));
    assert_eq!(engine.shutdown().await, ShutdownOutcome::Clean);
    assert_eq!(first.await, Err(Error::Closed));
    engine.wait_stopped().await;
}

#[tokio::test]
async fn full_diagnostic_queue_never_blocks_operations_and_reports_loss() {
    let dir = directory();
    let (engine, _events) = Engine::start(
        profile(&dir, None),
        Config {
            event_capacity: 1,
            ..Config::default()
        },
    )
    .unwrap();
    engine.wait_running().await.unwrap();
    for _ in 0..20 {
        assert_eq!(engine.recovered().await, Ok(0));
    }
    assert!(engine.snapshot().dropped_events > 0);
    assert_eq!(engine.shutdown().await, ShutdownOutcome::Clean);
}
