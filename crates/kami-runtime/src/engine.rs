use crate::{
    worker::{self, Completion, Work},
    Config, Error, Event, Lifecycle, Profile, ShutdownOutcome, Snapshot,
};
use kami_core::{DeviceId, MessageId};
use std::collections::BTreeMap;
use tokio::sync::{mpsc, oneshot, watch};

type Reply = oneshot::Sender<Result<usize, Error>>;
/// Sealed origin: only this module can wrap the result of its own direct dial.
/// Neither an external AuthenticatedConnection nor a trust snapshot is accepted.
#[allow(dead_code)]
pub(crate) struct IngressConnection {
    cap: kami_transport::AuthenticatedConnection,
    local: DeviceId,
    owner: mpsc::Sender<Command>,
    stop: watch::Sender<bool>,
}
#[allow(dead_code)]
impl IngressConnection {
    pub(crate) fn local(&self) -> DeviceId {
        self.local
    }
    pub(crate) fn remote(&self) -> [u8; 32] {
        self.cap.remote_device()
    }
    pub(crate) fn current(&self) -> bool {
        !*self.stop.borrow() && self.cap.is_current()
    }
    pub(crate) fn belongs_to(&self, engine: &Engine) -> bool {
        self.owner.same_channel(&engine.commands)
    }
    pub(crate) fn authenticated_peer(
        &self,
        trust: &kami_identity::ContactTrust,
    ) -> Result<libp2p_identity::PeerId, Error> {
        if !self.current() {
            return Err(Error::LiveTransportUnavailable);
        }
        self.cap
            .authenticated_peer(trust)
            .map_err(|_| Error::LiveTransportUnavailable)
    }
}
// Dropping an admitted future is not a rollback oracle. Stop further admission
// and invalidate every live cap even if the blocking operation has already begun.
struct IngressCancellation<'a> {
    engine: &'a Engine,
    armed: bool,
}
impl Drop for IngressCancellation<'_> {
    fn drop(&mut self) {
        if self.armed {
            self.engine.request_shutdown();
        }
    }
}
// Only the Linux ignored manual endpoint consumes this last-observed public view.
// It is not a storage status API, provenance constructor, or production diagnostic.
#[cfg(all(test, target_os = "linux"))]
#[derive(Clone)]
pub(crate) struct ManualPrekeyObservation {
    pub request: [u8; 32],
    pub peer: String,
    pub device: [u8; 32],
    pub retained: Option<ManualPrekeySelection>,
    pub reconciled: bool,
}
#[cfg(all(test, target_os = "linux"))]
#[derive(Clone)]
pub(crate) struct ManualPrekeySelection {
    pub kind: kami_identity::OlmPrekeyKind,
    pub reference: Vec<u8>,
    pub account_generation: u64,
    pub bundle_generation: u64,
    pub binding_generation: u64,
    pub not_before_ms: u64,
    pub expires_at_ms: u64,
}
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
    Work(Box<Work>, Reply),
}
#[derive(Clone)]
pub struct Engine {
    #[cfg(all(test, target_os = "linux"))]
    manual_prekey: watch::Sender<Option<ManualPrekeyObservation>>,
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
                #[cfg(all(test, target_os = "linux"))]
                manual_prekey: watch::channel(None).0,
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
    /// Artifact recovery is not readiness for a new cryptographic transition.
    pub async fn recovery_status(&self) -> Result<kami_storage::RecoveryStatus, Error> {
        let (reply, status) = oneshot::channel();
        self.work(Work::RecoveryStatus(reply)).await?;
        status.await.map_err(|_| Error::NotRunning)?
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
    pub async fn replenish_prekeys(&self, count: usize) -> Result<(), Error> {
        self.network()?;
        self.work(Work::MaintainPrekey(Some(count)))
            .await
            .map(|_| ())
    }
    pub async fn rotate_prekey_fallback(&self) -> Result<(), Error> {
        self.network()?;
        self.work(Work::MaintainPrekey(None)).await.map(|_| ())
    }
    /// Durable monotonic revocation/retirement, local inventory when device=None.
    pub async fn invalidate_prekeys(
        &self,
        device: Option<[u8; 32]>,
        through: u64,
        retire: bool,
    ) -> Result<(), Error> {
        self.network()?;
        self.work(Work::InvalidatePrekey(device, through, retire))
            .await
            .map(|_| ())
    }
    /// Prekey-only live exchange; no message/session activation or Delivered state.
    pub async fn refresh_prekeys(
        &self,
        cap: std::sync::Arc<kami_transport::AuthenticatedConnection>,
    ) -> Result<[u8; 32], Error> {
        self.prekeys(cap, None).await
    }
    /// Resume only an exact durable query for this authenticated peer/device.
    pub async fn retry_prekeys(
        &self,
        cap: std::sync::Arc<kami_transport::AuthenticatedConnection>,
        request: [u8; 32],
    ) -> Result<(), Error> {
        self.prekeys(cap, Some(request)).await.map(|_| ())
    }
    async fn prekeys(
        &self,
        cap: std::sync::Arc<kami_transport::AuthenticatedConnection>,
        request: Option<[u8; 32]>,
    ) -> Result<[u8; 32], Error> {
        #[cfg(all(test, target_os = "linux"))]
        self.manual_prekey.send_replace(None);
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            let network = self.network()?;
            let (tx, rx) = oneshot::channel();
            self.work(Work::BeginPrekey(cap.clone(), request, tx))
                .await?;
            let query = rx.await.map_err(|_| Error::WorkerFailed)??;
            let request = *query.request_id();
            #[cfg(all(test, target_os = "linux"))]
            let mut observed = {
                let fresh = self.contact(cap.remote_device()).await?;
                let peer = cap.authenticated_peer(&fresh).map_err(Error::Transport)?;
                let observed = ManualPrekeyObservation {
                    request,
                    peer: peer.to_string(),
                    device: cap.remote_device(),
                    retained: None,
                    reconciled: false,
                };
                self.manual_prekey.send_replace(Some(observed.clone()));
                observed
            };
            let evidence = network
                .exchange_prekey(&cap, query)
                .await
                .map_err(Error::Transport)?;
            #[cfg(all(test, target_os = "linux"))]
            let selection = {
                let fresh = self.contact(evidence.remote_device()).await?;
                let (value, _) = evidence.inspect(&fresh).map_err(Error::Transport)?;
                if let kami_protocol::prekey_exchange::Exchange::Selection(selection) = value {
                    let proof = kami_identity::OlmBindingProof::parse_bounded(&selection.proof)
                        .map_err(|_| Error::OperationRejected)?;
                    let data = proof.data();
                    Some(ManualPrekeySelection {
                        kind: data.prekey_kind,
                        reference: data.prekey_reference.clone(),
                        account_generation: selection.account_generation,
                        bundle_generation: data.bundle_generation,
                        binding_generation: data.binding_generation,
                        not_before_ms: data.not_before_ms,
                        expires_at_ms: data.expires_at_ms,
                    })
                } else {
                    None
                }
            };
            let (tx, rx) = oneshot::channel();
            self.work(Work::RetainPrekey(evidence, tx)).await?;
            let ack = rx.await.map_err(|_| Error::WorkerFailed)??;
            // Publish selection fields only AFTER the normal worker's signature,
            // freshness, exact-request, durable-retention and live-fence checks.
            #[cfg(all(test, target_os = "linux"))]
            {
                observed.retained = selection;
                self.manual_prekey.send_replace(Some(observed.clone()));
            }
            let evidence = network
                .exchange_prekey(&cap, ack)
                .await
                .map_err(Error::Transport)?;
            self.work(Work::FinishPrekey(evidence)).await?;
            #[cfg(all(test, target_os = "linux"))]
            {
                observed.reconciled = true;
                self.manual_prekey.send_replace(Some(observed));
            }
            Ok(request)
        })
        .await
        .map_err(|_| Error::Transport(kami_transport::Error::Timeout))?
    }
    #[cfg(all(test, target_os = "linux"))]
    pub(crate) fn manual_prekey_observation(&self) -> Option<ManualPrekeyObservation> {
        self.manual_prekey.borrow().clone()
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
        self.send(Command::Work(Box::new(work), tx))?;
        rx.await.map_err(|_| Error::Closed)?
    }
    pub async fn recovered(&self) -> Result<usize, Error> {
        self.work(Work::Recover).await
    }
    pub async fn cancel(&self, sender: DeviceId, id: MessageId) -> Result<(), Error> {
        self.work(Work::Cancel(sender, id)).await.map(|_| ())
    }
    /// Private foundation only: not wired to CLI, application wire or public enqueue.
    #[allow(dead_code)]
    pub(crate) async fn ingress_peer(
        &self,
        device: [u8; 32],
        addresses: Vec<kami_transport::Multiaddr>,
        conversation: kami_core::ConversationId,
        operation: crate::ingress::Operation,
    ) -> Result<crate::ingress::AuthenticatedPeer, Error> {
        if *self.stop.borrow() {
            return Err(Error::NotRunning);
        }
        let local = DeviceId::from_bytes(*self.local_invitation()?.device_key());
        let cap = self.connect(device, addresses).await?;
        if *self.stop.borrow() || !cap.is_current() {
            return Err(Error::LiveTransportUnavailable);
        }
        Ok(crate::ingress::AuthenticatedPeer::from_runtime(
            IngressConnection {
                cap,
                local,
                owner: self.commands.clone(),
                stop: self.stop.clone(),
            },
            conversation,
            operation,
        ))
    }
    #[allow(dead_code)]
    pub(crate) async fn dispatch_ingress(
        &self,
        peer: crate::ingress::AuthenticatedPeer,
        action: crate::ingress::Action,
    ) -> Result<crate::ingress::Outcome, Error> {
        if !peer.owned_by(self) || !peer.live() {
            return Err(Error::LiveTransportUnavailable);
        }
        let monitor = peer.monitor().ok_or(Error::LiveTransportUnavailable)?;
        let (reply, response) = oneshot::channel();
        let (done, completed) = oneshot::channel();
        self.send(Command::Work(
            Box::new(Work::Ingress(Box::new(peer), action, reply)),
            done,
        ))?;
        let mut cancellation = IngressCancellation {
            engine: self,
            armed: true,
        };
        // Done only accounts for queue occupancy. The typed worker reply is the
        // sole operation result; failure after submission is conservatively unknown.
        let result = match completed.await {
            Ok(Ok(_)) => response.await.unwrap_or(Err(Error::Indeterminate)),
            _ => Err(Error::Indeterminate),
        };
        let result = if result.is_ok() && !monitor.current() {
            Err(Error::Indeterminate)
        } else {
            result
        };
        if matches!(result, Err(Error::Indeterminate)) {
            self.request_shutdown();
        }
        cancellation.armed = false;
        result
    }
    #[cfg(test)]
    pub(crate) async fn test_ingress(
        &self,
        peer: crate::ingress::AuthenticatedPeer,
        f: impl FnOnce(&kami_storage::Storage, DeviceId, DeviceId) -> Result<usize, Error>
            + Send
            + 'static,
    ) -> Result<usize, Error> {
        match self
            .dispatch_ingress(peer, crate::ingress::Action::Probe(Box::new(f)))
            .await?
        {
            crate::ingress::Outcome::Probe(n) => Ok(n),
            _ => Err(Error::OperationRejected),
        }
    }
    /// Request data is not authority. Application delivery is still disabled;
    /// the private provenance foundation does not activate public messaging.
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
    let mut prekey_requests: Option<mpsc::Receiver<kami_transport::IncomingPrekeyExchange>> = None;
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
                                        let (driver, requests) = driver.with_prekeys();
                                        prekey_requests = Some(requests);
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
            request = async { match prekey_requests.as_mut() { Some(rx) => rx.recv().await, None => std::future::pending().await } }, if !draining => {
                if let Some(request) = request {
                    if state.borrow().lifecycle != Lifecycle::Running || pending.len() >= config.blocking_capacity {
                        request.reject(kami_protocol::prekey_exchange::Failure::Busy); continue;
                    }
                    let Some(next) = id.checked_add(1) else { request.reject(kami_protocol::prekey_exchange::Failure::Cancelled); continue; };
                    id = next;
                    if let Some(tx) = &jobs_tx {
                        match tx.try_send((id, Work::IncomingPrekey(request))) {
                            Ok(()) => { let (reply, _) = oneshot::channel(); pending.insert(id, reply); state.send_modify(|s| s.active_operations = pending.len()); }
                            Err(error) => { if let (_, Work::IncomingPrekey(request)) = error.into_inner() { request.reject(kami_protocol::prekey_exchange::Failure::Busy); } }
                        }
                    }
                } else { prekey_requests.take(); }
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
                        if jobs_tx.as_ref().is_some_and(|tx| tx.try_send((id, *work)).is_ok()) {
                            pending.insert(id, reply);
                            state.send_modify(|s| s.active_operations = pending.len());
                        } else { let _ = reply.send(Err(Error::Busy)); }
                    }
                }
            }
        }
    }
}
