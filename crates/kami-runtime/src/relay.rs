//! Reservation scheduling only, not relay admission or end-peer authentication.
//! Adapter futures MUST be cancellation-safe and nonblocking; they must not own
//! profile mutations or spawn detached mutation work. No real adapter ships here.
use std::{future::Future, pin::Pin, time::Duration};
use tokio::{
    sync::{mpsc, watch},
    time::Instant,
};
#[cfg(test)]
mod tests;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Reserve,
    Renew,
    Release,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RelayError {
    Unavailable,
    Denied,
    Timeout,
    InvalidLifetime,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RelayEvent {
    Attempted(Action),
    Active,
    Failed(RelayError),
    Expired,
    Released,
    ReleaseFailed,
    TaskFailed,
}
pub type RelayFuture<'a> = Pin<Box<dyn Future<Output = Result<Duration, RelayError>> + Send + 'a>>;
/// Results report a relative reservation lifetime, never message delivery.
pub trait RelayAdapter: Send + 'static {
    fn execute(&mut self, action: Action) -> RelayFuture<'_>;
}
#[derive(Clone, Debug)]
pub struct RelayConfig {
    pub renewal_margin: Duration,
    pub operation_timeout: Duration,
    pub retry_initial: Duration,
    pub retry_max: Duration,
}
impl Default for RelayConfig {
    fn default() -> Self {
        Self {
            renewal_margin: Duration::from_secs(5),
            operation_timeout: Duration::from_secs(5),
            retry_initial: Duration::from_secs(1),
            retry_max: Duration::from_secs(30),
        }
    }
}
impl RelayConfig {
    pub(crate) fn valid(&self) -> bool {
        [
            self.renewal_margin,
            self.operation_timeout,
            self.retry_initial,
            self.retry_max,
        ]
        .iter()
        .all(|d| !d.is_zero() && *d <= Duration::from_secs(3600))
            && self.retry_initial <= self.retry_max
            && self.retry_initial >= Duration::from_millis(1)
    }
}
async fn cancelled(stop: &mut watch::Receiver<bool>) {
    loop {
        if *stop.borrow_and_update() {
            return;
        }
        if stop.changed().await.is_err() {
            return;
        }
    }
}
fn emit(events: &mpsc::Sender<RelayEvent>, value: RelayEvent) {
    // Never stall renewal/cancellation on diagnostic backpressure.
    let _ = events.try_send(value);
}
pub(crate) async fn run(
    mut adapter: Box<dyn RelayAdapter>,
    config: RelayConfig,
    mut stop: watch::Receiver<bool>,
    events: mpsc::Sender<RelayEvent>,
) {
    let mut due = Instant::now();
    let mut expiry: Option<Instant> = None;
    let mut delay = config.retry_initial;
    loop {
        tokio::select! {
            biased;
            _ = cancelled(&mut stop) => break,
            _ = tokio::time::sleep_until(due) => {}
        }
        if expiry.is_some_and(|t| Instant::now() >= t) {
            expiry = None;
            emit(&events, RelayEvent::Expired);
        }
        let action = if expiry.is_some() {
            Action::Renew
        } else {
            Action::Reserve
        };
        emit(&events, RelayEvent::Attempted(action));
        let limit = expiry.map_or(Instant::now() + config.operation_timeout, |t| {
            t.min(Instant::now() + config.operation_timeout)
        });
        let result = tokio::select! {
            biased;
            _ = cancelled(&mut stop) => break,
            result = tokio::time::timeout_at(limit, adapter.execute(action)) => result.unwrap_or(Err(RelayError::Timeout)),
        };
        match result {
            Ok(ttl)
                if ttl >= config.renewal_margin + Duration::from_millis(1)
                    && ttl <= Duration::from_secs(86400) =>
            {
                expiry = Some(Instant::now() + ttl);
                due = Instant::now() + ttl - config.renewal_margin;
                delay = config.retry_initial;
                emit(&events, RelayEvent::Active);
            }
            result => {
                if expiry.is_some_and(|t| Instant::now() >= t) {
                    emit(&events, RelayEvent::Expired);
                }
                // Failure invalidates active status even before the old TTL expires.
                expiry = None;
                emit(
                    &events,
                    RelayEvent::Failed(result.err().unwrap_or(RelayError::InvalidLifetime)),
                );
                due = Instant::now() + delay;
                delay = delay.saturating_mul(2).min(config.retry_max);
            }
        }
    }
    let released =
        tokio::time::timeout(config.operation_timeout, adapter.execute(Action::Release)).await;
    emit(
        &events,
        if matches!(released, Ok(Ok(_))) {
            RelayEvent::Released
        } else {
            RelayEvent::ReleaseFailed
        },
    );
}
