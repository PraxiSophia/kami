use super::*;
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};
use tokio::sync::{mpsc, watch};
type Steps = Arc<Mutex<VecDeque<(Action, Result<Duration, RelayError>)>>>;
struct Mock(Steps);
impl RelayAdapter for Mock {
    fn execute(&mut self, action: Action) -> RelayFuture<'_> {
        let (expected, result) = self
            .0
            .lock()
            .unwrap()
            .pop_front()
            .expect("unexpected relay operation");
        assert_eq!(action, expected);
        Box::pin(async move { result })
    }
}
#[tokio::test(start_paused = true)]
async fn renewal_failure_is_inactive_and_reacquires_with_bounded_backoff() {
    let steps = Arc::new(Mutex::new(VecDeque::from([
        (Action::Reserve, Ok(Duration::from_secs(20))),
        (Action::Renew, Err(RelayError::Unavailable)),
        (Action::Reserve, Ok(Duration::from_secs(20))),
        (Action::Release, Ok(Duration::ZERO)),
    ])));
    let (stop, rx) = watch::channel(false);
    let (events, mut received) = mpsc::channel(16);
    let task = tokio::spawn(run(
        Box::new(Mock(steps)),
        RelayConfig::default(),
        rx,
        events,
    ));
    assert_eq!(
        received.recv().await,
        Some(RelayEvent::Attempted(Action::Reserve))
    );
    assert_eq!(received.recv().await, Some(RelayEvent::Active));
    tokio::time::advance(Duration::from_secs(15)).await;
    assert_eq!(
        received.recv().await,
        Some(RelayEvent::Attempted(Action::Renew))
    );
    assert_eq!(
        received.recv().await,
        Some(RelayEvent::Failed(RelayError::Unavailable))
    );
    tokio::time::advance(Duration::from_secs(1)).await;
    assert_eq!(
        received.recv().await,
        Some(RelayEvent::Attempted(Action::Reserve))
    );
    assert_eq!(received.recv().await, Some(RelayEvent::Active));
    stop.send_replace(true);
    assert_eq!(received.recv().await, Some(RelayEvent::Released));
    task.await.unwrap();
}
#[tokio::test(start_paused = true)]
async fn delayed_renewal_expires_at_original_deadline() {
    struct Expiring;
    impl RelayAdapter for Expiring {
        fn execute(&mut self, action: Action) -> RelayFuture<'_> {
            match action {
                Action::Renew => Box::pin(std::future::pending()),
                _ => Box::pin(async { Ok(Duration::from_secs(10)) }),
            }
        }
    }
    let (stop, rx) = watch::channel(false);
    let (events, mut received) = mpsc::channel(16);
    let config = RelayConfig {
        operation_timeout: Duration::from_secs(20),
        ..Default::default()
    };
    let task = tokio::spawn(run(Box::new(Expiring), config, rx, events));
    assert_eq!(
        received.recv().await,
        Some(RelayEvent::Attempted(Action::Reserve))
    );
    assert_eq!(received.recv().await, Some(RelayEvent::Active));
    tokio::time::advance(Duration::from_secs(5)).await;
    assert_eq!(
        received.recv().await,
        Some(RelayEvent::Attempted(Action::Renew))
    );
    tokio::time::advance(Duration::from_secs(5)).await;
    assert_eq!(received.recv().await, Some(RelayEvent::Expired));
    assert_eq!(
        received.recv().await,
        Some(RelayEvent::Failed(RelayError::Timeout))
    );
    stop.send_replace(true);
    task.await.unwrap();
}

#[tokio::test(start_paused = true)]
async fn retry_delay_caps_and_never_busy_loops_when_reservation_denied() {
    let steps = Arc::new(Mutex::new(VecDeque::from([
        (Action::Reserve, Err(RelayError::Denied)),
        (Action::Reserve, Err(RelayError::Denied)),
        (Action::Reserve, Err(RelayError::Denied)),
        (Action::Reserve, Err(RelayError::Denied)),
        (Action::Release, Ok(Duration::ZERO)),
    ])));
    let (stop, rx) = watch::channel(false);
    let (events, mut received) = mpsc::channel(16);
    let task = tokio::spawn(run(
        Box::new(Mock(steps)),
        RelayConfig {
            retry_max: Duration::from_secs(2),
            ..Default::default()
        },
        rx,
        events,
    ));
    for delay in [0, 1, 2, 2] {
        tokio::time::advance(Duration::from_secs(delay)).await;
        assert_eq!(
            received.recv().await,
            Some(RelayEvent::Attempted(Action::Reserve))
        );
        assert_eq!(
            received.recv().await,
            Some(RelayEvent::Failed(RelayError::Denied))
        );
        assert!(received.try_recv().is_err());
    }
    stop.send_replace(true);
    task.await.unwrap();
}

#[tokio::test(start_paused = true)]
async fn tiny_renewal_window_is_rejected_instead_of_spinning() {
    struct Tiny;
    impl RelayAdapter for Tiny {
        fn execute(&mut self, _: Action) -> RelayFuture<'_> {
            Box::pin(async { Ok(Duration::from_secs(5) + Duration::from_nanos(1)) })
        }
    }
    let (stop, rx) = watch::channel(false);
    let (events, mut received) = mpsc::channel(16);
    let task = tokio::spawn(run(Box::new(Tiny), RelayConfig::default(), rx, events));
    assert_eq!(
        received.recv().await,
        Some(RelayEvent::Attempted(Action::Reserve))
    );
    let result = received.recv().await;
    stop.send_replace(true);
    task.await.unwrap();
    assert_eq!(
        result,
        Some(RelayEvent::Failed(RelayError::InvalidLifetime))
    );
}

struct Hang;
impl RelayAdapter for Hang {
    fn execute(&mut self, _: Action) -> RelayFuture<'_> {
        Box::pin(std::future::pending())
    }
}
#[tokio::test(start_paused = true)]
async fn delayed_reserve_times_out_and_release_is_bounded_on_cancellation() {
    let (stop, rx) = watch::channel(false);
    let (events, mut received) = mpsc::channel(16);
    let task = tokio::spawn(run(Box::new(Hang), RelayConfig::default(), rx, events));
    assert_eq!(
        received.recv().await,
        Some(RelayEvent::Attempted(Action::Reserve))
    );
    tokio::time::advance(Duration::from_secs(5)).await;
    assert_eq!(
        received.recv().await,
        Some(RelayEvent::Failed(RelayError::Timeout))
    );
    stop.send_replace(true);
    tokio::time::advance(Duration::from_secs(5)).await;
    assert_eq!(received.recv().await, Some(RelayEvent::ReleaseFailed));
    task.await.unwrap();
}
