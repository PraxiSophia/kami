use crate::{
    worker::{self, Completion, Work},
    Config, Error, Event, Lifecycle, Profile, ShutdownOutcome, Snapshot,
};
use kami_core::{DeviceId, MessageId};
use std::collections::BTreeMap;
use tokio::sync::{mpsc, oneshot, watch};

type Reply = oneshot::Sender<Result<usize, Error>>;
struct Connectivity {
    revocation: kami_transport::Revocation,
    relay: Option<(
        crate::relay::RelayConfig,
        Box<dyn crate::relay::RelayAdapter>,
    )>,
    direct: Option<crate::DirectConfig>,
    network: watch::Sender<Option<(kami_transport::Handle, kami_identity::Invitation)>>,
}
enum Command {
    Health(oneshot::Sender<Result<Lifecycle, Error>>),
    Work(Work, Reply),
}
#[derive(Clone)]
pub struct Engine {
    revocation: kami_transport::Revocation,
    network: watch::Receiver<Option<(kami_transport::Handle, kami_identity::Invitation)>>,
    commands: mpsc::Sender<Command>,
    stop: watch::Sender<bool>,
    state: watch::Receiver<Snapshot>,
}
impl Engine {
    /// Call inside Tokio. Initialization is isolated and fail-closed; wait_running
    /// must succeed before profile operations can be admitted.
    pub fn start(profile: Profile, config: Config) -> Result<(Self, mpsc::Receiver<Event>), Error> {
        Self::start_inner(profile, config, None, None)
    }
    pub fn start_with_relay(
        profile: Profile,
        config: Config,
        relay: crate::relay::RelayConfig,
        adapter: Box<dyn crate::relay::RelayAdapter>,
    ) -> Result<(Self, mpsc::Receiver<Event>), Error> {
        if !relay.valid() {
            return Err(Error::InvalidConfig);
        }
        Self::start_inner(profile, config, Some((relay, adapter)), None)
    }
    /// Explicit native-only direct connectivity; application delivery stays off.
    /// Profile.key_provider is ignored by this native production composition.
    pub fn start_direct(
        profile: Profile,
        config: Config,
        direct: crate::DirectConfig,
    ) -> Result<(Self, mpsc::Receiver<Event>), Error> {
        direct.limits.validate().map_err(Error::Transport)?;
        if direct.listeners.len() > 2 {
            return Err(Error::InvalidConfig);
        }
        Self::start_inner(profile, config, None, Some(direct))
    }
    fn start_inner(
        profile: Profile,
        config: Config,
        relay: Option<(
            crate::relay::RelayConfig,
            Box<dyn crate::relay::RelayAdapter>,
        )>,
        direct: Option<crate::DirectConfig>,
    ) -> Result<(Self, mpsc::Receiver<Event>), Error> {
        if [
            config.command_capacity,
            config.event_capacity,
            config.blocking_capacity,
        ]
        .iter()
        .any(|n| *n == 0 || *n > 1024)
            || config.shutdown_timeout.is_zero()
            || config.shutdown_timeout > std::time::Duration::from_secs(3600)
            || config.retry_scan_interval < std::time::Duration::from_millis(10)
            || config.retry_scan_interval > std::time::Duration::from_secs(3600)
            || tokio::runtime::Handle::try_current().is_err()
        {
            return Err(Error::InvalidConfig);
        }
        let (commands, incoming) = mpsc::channel(config.command_capacity);
        let (events, receiver) = mpsc::channel(config.event_capacity);
        let (stop, stopping) = watch::channel(false);
        let (state, snapshot) = watch::channel(Snapshot {
            lifecycle: Lifecycle::Starting,
            active_operations: 0,
            dropped_events: 0,
            recovered: 0,
            failure: None,
            shutdown: None,
        });
        let (network, network_rx) = watch::channel(None);
        let revocation = kami_transport::Revocation::new();
        tokio::spawn(run(
            profile,
            config,
            incoming,
            events,
            stopping,
            state,
            Connectivity {
                relay,
                direct,
                network,
                revocation: revocation.clone(),
            },
        ));
        Ok((
            Self {
                revocation,
                network: network_rx,
                commands,
                stop,
                state: snapshot,
            },
            receiver,
        ))
    }
    pub fn snapshot(&self) -> Snapshot {
        *self.state.borrow()
    }
    fn network(&self) -> Result<kami_transport::Handle, Error> {
        if self.snapshot().lifecycle != Lifecycle::Running {
            return Err(Error::NotRunning);
        }
        self.network
            .borrow()
            .as_ref()
            .map(|(handle, _)| handle.clone())
            .ok_or(Error::LiveTransportUnavailable)
    }
    pub fn local_invitation(&self) -> Result<kami_identity::Invitation, Error> {
        self.network()?;
        self.network
            .borrow()
            .as_ref()
            .map(|(_, invitation)| invitation.clone())
            .ok_or(Error::LiveTransportUnavailable)
    }
    async fn contact(&self, device: [u8; 32]) -> Result<kami_identity::ContactTrust, Error> {
        let (tx, rx) = oneshot::channel();
        self.work(Work::Contact(device, tx)).await?;
        rx.await.map_err(|_| Error::WorkerFailed)?
    }
    pub async fn register_contact(&self, device: [u8; 32]) -> Result<(), Error> {
        let network = self.network()?;
        network
            .register(self.contact(device).await?)
            .await
            .map_err(Error::Transport)
    }
    pub async fn direct_addresses(&self) -> Result<Vec<kami_transport::Multiaddr>, Error> {
        self.network()?
            .local_addresses()
            .await
            .map_err(Error::Transport)
    }
    pub async fn connect(
        &self,
        device: [u8; 32],
        addresses: Vec<kami_transport::Multiaddr>,
    ) -> Result<kami_transport::AuthenticatedConnection, Error> {
        let network = self.network()?;
        network
            .register(self.contact(device).await?)
            .await
            .map_err(Error::Transport)?;
        let cap = network
            .dial(device, addresses)
            .await
            .map_err(Error::Transport)?;
        let fresh = self.contact(device).await?;
        cap.authenticated_peer(&fresh).map_err(Error::Transport)?;
        Ok(cap)
    }
    pub async fn disconnect(&self, device: [u8; 32]) -> Result<(), Error> {
        self.network()?
            .disconnect(device)
            .await
            .map_err(Error::Transport)
    }
    pub async fn wait_running(&self) -> Result<(), Error> {
        let mut state = self.state.clone();
        loop {
            let value = *state.borrow_and_update();
            match value.lifecycle {
                Lifecycle::Running => return Ok(()),
                Lifecycle::Failed => return Err(value.failure.unwrap_or(Error::WorkerFailed)),
                Lifecycle::Draining | Lifecycle::Stopped => return Err(Error::Closed),
                _ => {}
            }
            state.changed().await.map_err(|_| Error::Closed)?;
        }
    }
    pub async fn health(&self) -> Result<Lifecycle, Error> {
        let (tx, rx) = oneshot::channel();
        self.send(Command::Health(tx))?;
        rx.await.map_err(|_| Error::Closed)?
    }
    fn send(&self, command: Command) -> Result<(), Error> {
        self.commands.try_send(command).map_err(|e| match e {
            mpsc::error::TrySendError::Full(_) => Error::Busy,
            mpsc::error::TrySendError::Closed(_) => Error::Closed,
        })
    }
    async fn work(&self, work: Work) -> Result<usize, Error> {
        let (tx, rx) = oneshot::channel();
        self.send(Command::Work(work, tx))?;
        rx.await.map_err(|_| Error::Closed)?
    }
    pub async fn recovered(&self) -> Result<usize, Error> {
        self.work(Work::Recover).await
    }
    pub async fn cancel(&self, sender: DeviceId, id: MessageId) -> Result<(), Error> {
        self.work(Work::Cancel(sender, id)).await.map(|_| ())
    }
    /// Request data is not authority. No production connection producer exists
    /// until KAMI-27; fail closed rather than accepting an asserted PeerId.
    pub async fn enqueue(&self, _request: kami_sender::SendRequest) -> Result<(), Error> {
        Err(Error::LiveTransportUnavailable)
    }
    /// Out-of-band cancellation cannot be starved by a full command queue.
    pub fn request_shutdown(&self) {
        self.revocation.revoke();
        self.stop.send_replace(true);
    }
    pub async fn shutdown(&self) -> ShutdownOutcome {
        self.request_shutdown();
        let mut state = self.state.clone();
        loop {
            if let Some(outcome) = state.borrow_and_update().shutdown {
                return outcome;
            }
            if state.changed().await.is_err() {
                return ShutdownOutcome::Failed;
            }
        }
    }
    /// Unlike shutdown(), may wait indefinitely for a blocked OS operation.
    pub async fn wait_stopped(&self) {
        let mut state = self.state.clone();
        loop {
            if matches!(
                state.borrow_and_update().lifecycle,
                Lifecycle::Stopped | Lifecycle::Failed
            ) {
                return;
            }
            if state.changed().await.is_err() {
                return;
            }
        }
    }
    #[cfg(test)]
    pub(crate) async fn test_work(
        &self,
        f: impl FnOnce(&kami_storage::Storage) -> Result<usize, Error> + Send + 'static,
    ) -> Result<usize, Error> {
        self.work(Work::Test(Box::new(f))).await
    }
}
fn event(events: &mpsc::Sender<Event>, state: &watch::Sender<Snapshot>, value: Event) {
    if events.try_send(value).is_err() {
        state.send_modify(|s| s.dropped_events = s.dropped_events.saturating_add(1));
    }
}
fn lifecycle(events: &mpsc::Sender<Event>, state: &watch::Sender<Snapshot>, value: Lifecycle) {
    state.send_modify(|s| s.lifecycle = value);
    event(events, state, Event::Lifecycle(value));
}
async fn run(
    profile: Profile,
    config: Config,
    mut commands: mpsc::Receiver<Command>,
    events: mpsc::Sender<Event>,
    mut stop: watch::Receiver<bool>,
    state: watch::Sender<Snapshot>,
    connectivity: Connectivity,
) {
    let Connectivity {
        mut relay,
        mut direct,
        network,
        revocation,
    } = connectivity;
    let (jobs_tx, jobs) = mpsc::channel(config.blocking_capacity);
    let (done, mut completions) = mpsc::channel(config.blocking_capacity + 1);
    let direct_mode = direct.is_some();
    let (authority, alive) = watch::channel(false);
    let worker_revocation = revocation.clone();
    let mut worker = tokio::task::spawn_blocking(move || {
        worker::run(
            profile,
            jobs,
            done,
            direct_mode,
            worker::Authority(authority, worker_revocation),
        )
    });
    let mut worker_done = false;
    let mut children = tokio::task::JoinSet::new();
    let mut transport_events: Option<mpsc::Receiver<kami_transport::Event>> = None;
    let (relay_stop, relay_stopping) = watch::channel(false);
    let (relay_events, mut relay_incoming) = mpsc::channel(config.event_capacity);
    let mut jobs_tx = Some(jobs_tx);
    let mut pending = BTreeMap::<u64, Reply>::new();
    let mut id = 0u64;
    let mut draining = false;
    let mut deadline = None;
    let mut retry_tick = tokio::time::interval_at(
        tokio::time::Instant::now() + config.retry_scan_interval,
        config.retry_scan_interval,
    );
    retry_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    lifecycle(&events, &state, Lifecycle::Reconciling);
    loop {
        tokio::select! {
            biased;
            changed = stop.changed(), if !draining => {
                if changed.is_err() || *stop.borrow_and_update() {
                    draining = true;
                    commands.close();
                    while let Ok(command) = commands.try_recv() {
                        match command {
                            Command::Health(reply) => { let _ = reply.send(Err(Error::Closed)); }
                            Command::Work(_, reply) => { let _ = reply.send(Err(Error::Closed)); }
                        }
                    }
                    if children.is_empty() { jobs_tx.take(); }
                    deadline = Some(tokio::time::Instant::now() + config.shutdown_timeout);
                    relay.take();
                    relay_stop.send_replace(true);
                    lifecycle(&events, &state, Lifecycle::Draining);
                }
            }
            _ = async { match deadline { Some(t) => tokio::time::sleep_until(t).await, None => std::future::pending().await } } => {
                deadline = None;
                state.send_modify(|s| s.shutdown = Some(ShutdownOutcome::TimedOut));
                event(&events, &state, Event::ShutdownTimedOut);
            }
            completion = completions.recv(), if !worker_done => {
                match completion {
                    Some(Completion::Contacts(updates)) => {
                        if !draining {
                            let handle = network.borrow().as_ref().map(|(h,_)| h.clone());
                            if let Some(handle) = handle {
                                for (device,trust) in updates {
                                    if let Err(error) = handle.refresh_contact(device,trust) {
                                        state.send_modify(|s| s.failure = Some(Error::Transport(error)));
                                        let (tx,rx) = watch::channel(true); drop(tx); stop = rx;
                                        relay_stop.send_replace(true);
                                        break;
                                    }
                                }
                            }
                        }
                    }
                    Some(Completion::Ready(Ok((count, credentials)))) => {
                        state.send_modify(|s| s.recovered = count);
                        event(&events, &state, Event::Reconciled(count));
                        if !draining {
                            if let (Some(config), Some(credentials)) = (direct.take(), credentials) {
                                let invitation = credentials.invitation().clone();
                                match kami_transport::Driver::new(*credentials, config.limits, config.listeners) {
                                    Ok((driver, handle, diagnostics)) => {
                                        let driver = driver.with_revocation(revocation.clone());
                                        transport_events = Some(diagnostics);
                                        network.send_replace(Some((handle, invitation)));
                                        let stopping = relay_stopping.clone();
                                        let mut validity = alive.clone();
                                        let valid = *validity.borrow_and_update();
                                        children.spawn(async move {
                                            if !valid { return Err(Error::LiveTransportUnavailable); }
                                            tokio::select! {
                                                biased;
                                                _ = validity.changed() => Err(Error::LiveTransportUnavailable),
                                                result = driver.run(stopping) => result.map_err(Error::Transport),
                                            }
                                        });
                                    }
                                    Err(error) => {
                                        state.send_modify(|s| s.failure = Some(Error::Transport(error)));
                                        jobs_tx.take();
                                        continue;
                                    }
                                }
                            }
                            lifecycle(&events, &state, Lifecycle::Running);
                            if let Some((config, adapter)) = relay.take() {
                                let stopping = relay_stopping.clone(); let events = relay_events.clone();
                                children.spawn(async move { crate::relay::run(adapter, config, stopping, events).await; Ok(()) });
                            }
                        }
                    }
                    Some(Completion::Ready(Err(e))) => {
                        state.send_modify(|s| s.failure = Some(e));
                        event(&events, &state, Event::OperationFailed(e));
                        jobs_tx.take();
                    }
                    Some(Completion::Done(id, result)) => {
                        if let Some(reply) = pending.remove(&id) { let _ = reply.send(result); }
                        state.send_modify(|s| s.active_operations = pending.len());
                        event(&events, &state, match result { Ok(count) if id == 0 => Event::RetryReady(count), Ok(_) => Event::OperationCompleted, Err(e) => Event::OperationFailed(e) });
                    }
                    Some(Completion::Closed(result)) => {
                        if let Err(e) = result {
                            state.send_modify(|s| s.failure = Some(e));
                            event(&events, &state, Event::OperationFailed(e));
                        }
                    }
                    None => {
                        let result = (&mut worker).await;
                        let failed = result.is_err() || !pending.is_empty() || state.borrow().failure.is_some();
                        state.send_modify(|s| {
                            s.active_operations = 0;
                            if failed { s.failure = s.failure.or(Some(Error::WorkerFailed)); }
                            if s.shutdown.is_none() && children.is_empty() { s.shutdown = Some(if failed { ShutdownOutcome::Failed } else { ShutdownOutcome::Clean }); }
                        });
                        worker_done = true;
                        relay_stop.send_replace(true);
                        if children.is_empty() {
                            lifecycle(&events, &state, if failed { Lifecycle::Failed } else { Lifecycle::Stopped });
                            for (_, reply) in std::mem::take(&mut pending) { let _ = reply.send(Err(Error::WorkerFailed)); }
                            return;
                        }
                        if !draining {
                            draining = true;
                            commands.close();
                            while let Ok(command) = commands.try_recv() {
                                match command {
                                    Command::Health(reply) => { let _ = reply.send(Err(Error::Closed)); }
                                    Command::Work(_, reply) => { let _ = reply.send(Err(Error::Closed)); }
                                }
                            }
                            jobs_tx.take();
                            deadline = Some(tokio::time::Instant::now() + config.shutdown_timeout);
                            lifecycle(&events, &state, Lifecycle::Draining);
                        }
                        for (_, reply) in std::mem::take(&mut pending) { let _ = reply.send(Err(Error::WorkerFailed)); }
                    }
                }
            }
            result = children.join_next(), if !children.is_empty() => {
                let child_error = match result { Some(Err(_)) => Some(Error::WorkerFailed), Some(Ok(Err(e))) => Some(e), _ => None };
                if child_error.is_some() {
                    event(&events, &state, Event::Relay(crate::relay::RelayEvent::TaskFailed));
                }
                if direct_mode {
                    network.send_replace(None);
                    if !draining {
                        state.send_modify(|s| s.failure = Some(child_error.unwrap_or(Error::LiveTransportUnavailable)));
                        let (tx, rx) = watch::channel(true); drop(tx); stop = rx;
                    }
                }
                if draining && children.is_empty() { jobs_tx.take(); }
                if worker_done {
                    let failed = state.borrow().failure.is_some();
                    state.send_modify(|s| { if s.shutdown.is_none() { s.shutdown = Some(if failed { ShutdownOutcome::Failed } else { ShutdownOutcome::Clean }); } });
                    lifecycle(&events, &state, if failed { Lifecycle::Failed } else { Lifecycle::Stopped });
                    return;
                }
            }
            Some(value) = relay_incoming.recv() => { event(&events, &state, Event::Relay(value)); }
            value = async { match transport_events.as_mut() { Some(rx) => rx.recv().await, None => std::future::pending().await } } => {
                match value {
                    None => { transport_events.take(); }
                    Some(value) => {
                        use crate::TransportEvent as T;
                        use kami_transport::Event as N;
                        let value = match value { N::Listening(_) => T::Listening, N::Connected(_) => T::Connected, N::Disconnected(_) => T::Disconnected, N::AttemptFailed(error) => T::AttemptFailed(error), N::Stopped => T::Stopped };
                        event(&events, &state, Event::Transport(value));
                    }
                }
            }
            _ = retry_tick.tick(), if !draining && state.borrow().lifecycle == Lifecycle::Running => {
                // One coalesced scan, sharing the same admission bound as user work.
                // A wakeup is never authentication, a lease, or permission to send.
                if pending.len() < config.blocking_capacity && !pending.contains_key(&0)
                    && jobs_tx.as_ref().is_some_and(|tx| tx.try_send((0, Work::RetryScan)).is_ok()) {
                    let (reply, _) = oneshot::channel();
                    pending.insert(0, reply);
                    state.send_modify(|s| s.active_operations = pending.len());
                }
            }
            command = commands.recv(), if !draining => {
                match command {
                    None => { let (tx, rx) = watch::channel(true); drop(tx); stop = rx; }
                    Some(Command::Health(reply)) => { let _ = reply.send(Ok(state.borrow().lifecycle)); }
                    Some(Command::Work(work, reply)) => {
                        if state.borrow().lifecycle != Lifecycle::Running { let _ = reply.send(Err(Error::NotRunning)); continue; }
                        if pending.len() >= config.blocking_capacity { let _ = reply.send(Err(Error::Busy)); continue; }
                        let Some(next) = id.checked_add(1) else { let _ = reply.send(Err(Error::Closed)); continue; };
                        id = next;
                        if jobs_tx.as_ref().is_some_and(|tx| tx.try_send((id, work)).is_ok()) {
                            pending.insert(id, reply);
                            state.send_modify(|s| s.active_operations = pending.len());
                        } else { let _ = reply.send(Err(Error::Busy)); }
                    }
                }
            }
        }
    }
}
