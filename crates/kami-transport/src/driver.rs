use crate::prekey_exchange::{Behaviour, Packet, WireEvent};
use crate::{policy::direct_address, AuthenticatedConnection, Config, Error};
use kami_identity::{
    send_decision, ContactTrust, ContactTrustState, SealedTransportCredentials, SendDecision,
};
use kami_protocol::prekey_exchange::Exchange;
use libp2p::request_response as rr;
use libp2p::{
    connection_limits::ConnectionLimits,
    futures::StreamExt,
    swarm::{dial_opts::DialOpts, ConnectionId, DialError, SwarmEvent},
    Multiaddr, PeerId, Swarm,
};
use std::{
    collections::{HashMap, VecDeque},
    num::NonZeroU8,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use tokio::sync::{mpsc, oneshot, watch};

type Reply<T> = oneshot::Sender<Result<T, Error>>;
enum Command {
    Prekey(
        Box<AuthenticatedConnection>,
        Exchange,
        Reply<crate::LivePrekeyExchange>,
    ),
    Register(Box<ContactTrust>, Reply<()>),
    Addresses(Reply<Vec<Multiaddr>>),
    Dial([u8; 32], Vec<Multiaddr>, Reply<AuthenticatedConnection>),
    Disconnect([u8; 32], Reply<()>),
    Revoke(Reply<()>),
    Remove([u8; 32]),
}
#[derive(Clone)]
pub struct Handle {
    tx: mpsc::Sender<Command>,
}
impl Handle {
    pub async fn exchange_prekey(
        &self,
        cap: &AuthenticatedConnection,
        value: Exchange,
    ) -> Result<crate::LivePrekeyExchange, Error> {
        if !cap.is_current() || !matches!(value, Exchange::Query(_) | Exchange::Ack(_)) {
            return Err(Error::Untrusted);
        }
        kami_protocol::prekey_exchange::encode(&value).map_err(|_| Error::Untrusted)?;
        self.request(|r| Command::Prekey(Box::new(cap.retained()), value, r))
            .await
    }
    /// Reliable bounded control enqueue for runtime durable-trust refresh. On
    /// queue failure runtime must revoke the entire transport out-of-band.
    pub fn refresh_contact(
        &self,
        device: [u8; 32],
        contact: Option<ContactTrust>,
    ) -> Result<(), Error> {
        let command = match contact {
            Some(contact)
                if contact
                    .pinned()
                    .is_some_and(|pin| pin.device_key() == &device) =>
            {
                let (tx, _) = oneshot::channel();
                Command::Register(Box::new(contact), tx)
            }
            _ => Command::Remove(device),
        };
        self.tx.try_send(command).map_err(|_| Error::Busy)
    }
    async fn request<T>(&self, command: impl FnOnce(Reply<T>) -> Command) -> Result<T, Error> {
        let (tx, rx) = oneshot::channel();
        self.tx.try_send(command(tx)).map_err(|e| match e {
            mpsc::error::TrySendError::Full(_) => Error::Busy,
            _ => Error::Closed,
        })?;
        rx.await.map_err(|_| Error::Closed)?
    }
    /// Trusted runtime composition only: pass a freshly loaded durable contact.
    pub async fn register(&self, contact: ContactTrust) -> Result<(), Error> {
        self.request(|r| Command::Register(Box::new(contact), r))
            .await
    }
    pub async fn local_addresses(&self) -> Result<Vec<Multiaddr>, Error> {
        self.request(Command::Addresses).await
    }
    pub async fn dial(
        &self,
        device: [u8; 32],
        addresses: Vec<Multiaddr>,
    ) -> Result<AuthenticatedConnection, Error> {
        if addresses.is_empty() || addresses.len() > 8 {
            return Err(Error::InvalidAddress);
        }
        for addr in &addresses {
            direct_address(addr, false)?;
        }
        self.request(|r| Command::Dial(device, addresses, r)).await
    }
    pub async fn disconnect(&self, device: [u8; 32]) -> Result<(), Error> {
        self.request(|r| Command::Disconnect(device, r)).await
    }
    /// Epoch/profile/trust invalidation, not a telemetry event.
    pub async fn revoke_all(&self) -> Result<(), Error> {
        self.request(Command::Revoke).await
    }
}
#[derive(Clone, Debug)]
pub enum Event {
    Listening(Multiaddr),
    Connected([u8; 32]),
    Disconnected([u8; 32]),
    AttemptFailed(Error),
    Stopped,
}
struct Live {
    id: ConnectionId,
    device: [u8; 32],
    generation: u64,
    flag: Arc<AtomicBool>,
    expires: Instant,
}
impl Drop for Live {
    fn drop(&mut self) {
        self.flag.store(false, Ordering::Release);
    }
}
struct Dial {
    remaining: VecDeque<Multiaddr>,
    attempts: usize,
    id: Option<ConnectionId>,
    aborting: bool,
    started: Instant,
    reply: Reply<AuthenticatedConnection>,
}
/// One polled swarm, owned by the runtime JoinSet. No detached task, storage,
/// identity authority, application protocol, stream sender, ACK or receipt API.
pub struct Driver {
    revocation: crate::Revocation,
    swarm: Swarm<Behaviour>,
    config: Config,
    commands: mpsc::Receiver<Command>,
    events: mpsc::Sender<Event>,
    contacts: HashMap<PeerId, ContactTrust>,
    live: HashMap<PeerId, Live>,
    dials: HashMap<PeerId, Dial>,
    addresses: Vec<Multiaddr>,
    local_device: [u8; 32],
    inbound: Option<mpsc::Sender<crate::IncomingPrekeyExchange>>,
    outgoing: HashMap<rr::OutboundRequestId, Outgoing>,
    incoming: HashMap<PeerId, Incoming>,
}
struct Outgoing {
    peer: PeerId,
    cap: AuthenticatedConnection,
    request: Exchange,
    deadline: Instant,
    reply: Reply<crate::LivePrekeyExchange>,
}
struct Incoming {
    id: rr::InboundRequestId,
    cap: AuthenticatedConnection,
    request: Exchange,
    deadline: Instant,
    response: oneshot::Receiver<Exchange>,
    channel: Option<rr::ResponseChannel<Packet>>,
}
impl Driver {
    pub fn with_prekeys(mut self) -> (Self, mpsc::Receiver<crate::IncomingPrekeyExchange>) {
        let (tx, rx) = mpsc::channel(16);
        self.inbound = Some(tx);
        (self, rx)
    }
    /// Additional monotonic runtime revocation; not proof of a held profile lease.
    pub fn with_revocation(mut self, revocation: crate::Revocation) -> Self {
        self.revocation = revocation;
        self
    }
    pub fn new(
        credentials: SealedTransportCredentials,
        config: Config,
        listeners: Vec<Multiaddr>,
    ) -> Result<(Self, Handle, mpsc::Receiver<Event>), Error> {
        config.validate()?;
        if listeners.len() > 2 {
            return Err(Error::InvalidAddress);
        }
        for addr in &listeners {
            direct_address(addr, true)?;
        }
        let peer = credentials.local_peer_id();
        let local_device = *credentials.invitation().device_key();
        let limits = ConnectionLimits::default()
            .with_max_pending_incoming(Some(config.max_connections))
            .with_max_pending_outgoing(Some(config.max_dials as u32))
            .with_max_established(Some(config.max_connections))
            .with_max_established_per_peer(Some(1));
        let mut swarm = Swarm::new(
            credentials.into_transport(),
            Behaviour::new(limits),
            peer,
            libp2p::swarm::Config::with_tokio_executor()
                .with_dial_concurrency_factor(NonZeroU8::new(1).unwrap())
                .with_idle_connection_timeout(Duration::from_secs(60)),
        );
        for addr in listeners {
            swarm.listen_on(addr).map_err(|_| Error::Unavailable)?;
        }
        let (tx, commands) = mpsc::channel(64);
        let (events, rx) = mpsc::channel(32);
        Ok((
            Self {
                revocation: crate::Revocation::new(),
                swarm,
                config,
                commands,
                events,
                contacts: HashMap::new(),
                live: HashMap::new(),
                dials: HashMap::new(),
                addresses: Vec::new(),
                local_device,
                inbound: None,
                outgoing: HashMap::new(),
                incoming: HashMap::new(),
            },
            Handle { tx },
            rx,
        ))
    }
    fn emit(&self, event: Event) {
        let _ = self.events.try_send(event);
    }
    fn capability(&self, peer: PeerId) -> Result<AuthenticatedConnection, Error> {
        let c = self.live.get(&peer).ok_or(Error::Unavailable)?;
        Ok(AuthenticatedConnection {
            authority: self.revocation.clone(),
            peer,
            device: c.device,
            generation: c.generation,
            live: c.flag.clone(),
            expires: c.expires,
        })
    }
    fn revoke(&mut self, peer: PeerId, reason: Error) {
        self.incoming.remove(&peer);
        let ids: Vec<_> = self
            .outgoing
            .iter()
            .filter_map(|(id, p)| (p.peer == peer).then_some(*id))
            .collect();
        for id in ids {
            if let Some(p) = self.outgoing.remove(&id) {
                let _ = p.reply.send(Err(reason));
            }
        }
        if let Some(c) = self.live.remove(&peer) {
            self.emit(Event::Disconnected(c.device));
        }
        if let Some(dial) = self.dials.remove(&peer) {
            let _ = dial.reply.send(Err(reason));
        }
        // libp2p disconnect aborts pending as well as established connections.
        let _ = self.swarm.disconnect_peer_id(peer);
    }
    fn revoke_all_inner(&mut self, reason: Error) {
        let peers: Vec<_> = self.live.keys().chain(self.dials.keys()).copied().collect();
        for peer in peers {
            self.revoke(peer, reason);
        }
    }
    fn attempt(&mut self, peer: PeerId) {
        let Some(dial) = self.dials.get_mut(&peer) else {
            return;
        };
        if dial.attempts >= self.config.max_attempts || dial.remaining.is_empty() {
            self.revoke(peer, Error::Unavailable);
            return;
        }
        let address = dial.remaining.pop_front().unwrap();
        dial.attempts += 1;
        dial.started = Instant::now();
        dial.aborting = false;
        let opts = DialOpts::peer_id(peer).addresses(vec![address]).build();
        dial.id = Some(opts.connection_id());
        if self.swarm.dial(opts).is_err() {
            self.revoke(peer, Error::Unavailable);
        }
    }
    fn command(&mut self, command: Command) {
        match command {
            Command::Prekey(cap, value, reply) => {
                let cap = *cap;
                let peer = cap.peer;
                let valid = self
                    .live
                    .get(&peer)
                    .is_some_and(|live| Arc::ptr_eq(&live.flag, &cap.live))
                    && self
                        .contacts
                        .get(&peer)
                        .is_some_and(|t| cap.authenticated_peer(t).is_ok())
                    && request_devices(&value) == Some((self.local_device, cap.device));
                if !valid {
                    let _ = reply.send(Err(Error::Untrusted));
                    return;
                }
                if !prekey_capacity(
                    self.incoming.len() + self.outgoing.len(),
                    self.outgoing.values().any(|p| p.peer == peer),
                ) {
                    let _ = reply.send(Err(Error::Busy));
                    return;
                }
                let deadline = cap.expires.min(Instant::now() + Duration::from_secs(5));
                let id = self
                    .swarm
                    .behaviour_mut()
                    .exchange
                    .send_request(&peer, Packet::outgoing(value.clone()));
                self.outgoing.insert(
                    id,
                    Outgoing {
                        peer,
                        cap,
                        request: value,
                        deadline,
                        reply,
                    },
                );
            }
            Command::Remove(device) => {
                let peers: Vec<_> = self
                    .contacts
                    .iter()
                    .filter_map(|(p, c)| {
                        c.pinned()
                            .is_some_and(|pin| pin.device_key() == &device)
                            .then_some(*p)
                    })
                    .collect();
                for peer in peers {
                    self.revoke(peer, Error::Untrusted);
                    self.contacts.remove(&peer);
                }
            }
            Command::Addresses(r) => {
                let _ = r.send(Ok(self.addresses.clone()));
            }
            Command::Register(contact, r) => {
                let contact = *contact;
                let Some(pin) = contact.pinned() else {
                    let _ = r.send(Err(Error::Untrusted));
                    return;
                };
                let peer = pin.binding().peer_id();
                if self.contacts.len() >= self.config.max_connections as usize
                    && !self.contacts.contains_key(&peer)
                {
                    let _ = r.send(Err(Error::Busy));
                    return;
                }
                if self.contacts.get(&peer).is_some_and(|old| old != &contact) {
                    self.revoke(peer, Error::Untrusted);
                }
                // Unknown/Pending/Changed/Blocked may revoke but never authorize.
                if contact.state() != ContactTrustState::Verified {
                    self.revoke(peer, Error::Untrusted);
                    self.contacts.remove(&peer);
                    let _ = r.send(Err(Error::Untrusted));
                    return;
                }
                self.contacts.insert(peer, contact);
                let _ = r.send(Ok(()));
            }
            Command::Disconnect(device, r) => {
                let peers: Vec<_> = self
                    .contacts
                    .iter()
                    .filter_map(|(p, c)| {
                        (c.pinned().is_some_and(|pin| pin.device_key() == &device)).then_some(*p)
                    })
                    .collect();
                for peer in peers {
                    self.revoke(peer, Error::Cancelled);
                }
                let _ = r.send(Ok(()));
            }
            Command::Revoke(r) => {
                self.revoke_all_inner(Error::Expired);
                let _ = r.send(Ok(()));
            }
            Command::Dial(device, mut addresses, reply) => {
                let Some(peer) = self.contacts.iter().find_map(|(p, c)| {
                    c.pinned()
                        .is_some_and(|pin| pin.device_key() == &device)
                        .then_some(*p)
                }) else {
                    let _ = reply.send(Err(Error::Untrusted));
                    return;
                };
                if self.live.contains_key(&peer) {
                    let _ = reply.send(self.capability(peer));
                    return;
                }
                if self.dials.len() >= self.config.max_dials || self.dials.contains_key(&peer) {
                    let _ = reply.send(Err(Error::Busy));
                    return;
                }
                addresses.sort_by_key(|a| !direct_address(a, false).unwrap_or(false));
                self.dials.insert(
                    peer,
                    Dial {
                        remaining: addresses.into(),
                        attempts: 0,
                        id: None,
                        aborting: false,
                        started: Instant::now(),
                        reply,
                    },
                );
                self.attempt(peer);
            }
        }
    }
    fn tick(&mut self) {
        self.prekey_tick();
        let expired: Vec<_> = self
            .live
            .iter()
            .filter_map(|(p, c)| (Instant::now() >= c.expires).then_some(*p))
            .collect();
        for peer in expired {
            self.revoke(peer, Error::Expired);
        }
        let pending: Vec<_> = self
            .dials
            .iter()
            .filter_map(|(p, d)| {
                (d.reply.is_closed() || d.started.elapsed() >= self.config.dial_timeout)
                    .then_some((*p, d.reply.is_closed()))
            })
            .collect();
        for (peer, cancelled) in pending {
            if cancelled {
                self.revoke(peer, Error::Cancelled);
                continue;
            }
            // libp2p removes pending dials asynchronously. Wait for the exact
            // Aborted event before starting fallback, retaining concurrency caps.
            if self.dials.get(&peer).is_some_and(|d| d.aborting) {
                self.revoke(peer, Error::Timeout);
                continue;
            }
            if let Some(d) = self.dials.get_mut(&peer) {
                d.aborting = true;
                d.started = Instant::now();
            }
            let _ = self.swarm.disconnect_peer_id(peer);
            self.emit(Event::AttemptFailed(Error::Timeout));
        }
    }
    fn prekey_event(&mut self, event: WireEvent) {
        match event {
            rr::Event::Message {
                peer,
                connection_id,
                message,
            } => {
                if !self.live.get(&peer).is_some_and(|c| {
                    c.id == connection_id
                        && c.flag.load(Ordering::Acquire)
                        && Instant::now() < c.expires
                }) {
                    return;
                }
                match message {
                    rr::Message::Request {
                        request_id,
                        request,
                        channel,
                    } => {
                        let Ok(cap) = self.capability(peer) else {
                            return;
                        };
                        if self
                            .contacts
                            .get(&peer)
                            .is_none_or(|c| cap.authenticated_peer(c).is_err())
                            || request_devices(&request.value)
                                != Some((cap.device, self.local_device))
                        {
                            self.revoke(peer, Error::Untrusted);
                            return;
                        }
                        if !prekey_capacity(
                            self.incoming.len() + self.outgoing.len(),
                            self.incoming.contains_key(&peer),
                        ) {
                            let error = Exchange::Error {
                                request: *request.value.request_id(),
                                failure: kami_protocol::prekey_exchange::Failure::Busy,
                            };
                            let _ = self
                                .swarm
                                .behaviour_mut()
                                .exchange
                                .send_response(channel, Packet::outgoing(error));
                            return;
                        }
                        let deadline = cap.expires.min(Instant::now() + Duration::from_secs(5));
                        let (tx, rx) = oneshot::channel();
                        let incoming = crate::IncomingPrekeyExchange {
                            evidence: crate::LivePrekeyExchange {
                                capability: cap.retained(),
                                value: request.value.clone(),
                                deadline,
                            },
                            reply: tx,
                        };
                        if self
                            .inbound
                            .as_ref()
                            .is_none_or(|queue| queue.try_send(incoming).is_err())
                        {
                            return;
                        }
                        self.incoming.insert(
                            peer,
                            Incoming {
                                id: request_id,
                                cap,
                                request: request.value,
                                deadline,
                                response: rx,
                                channel: Some(channel),
                            },
                        );
                    }
                    rr::Message::Response {
                        request_id,
                        response,
                    } => {
                        let Some(p) = self.outgoing.remove(&request_id) else {
                            return;
                        };
                        let valid = p.peer == peer
                            && p.cap.is_current()
                            && Instant::now() < p.deadline
                            && self.live.get(&peer).is_some_and(|c| {
                                c.id == connection_id && Arc::ptr_eq(&c.flag, &p.cap.live)
                            })
                            && self
                                .contacts
                                .get(&peer)
                                .is_some_and(|c| p.cap.authenticated_peer(c).is_ok())
                            && matching_response(&p.request, &response.value);
                        let result = if valid {
                            Ok(crate::LivePrekeyExchange {
                                capability: p.cap,
                                value: response.value,
                                deadline: p.deadline,
                            })
                        } else {
                            Err(Error::Untrusted)
                        };
                        let _ = p.reply.send(result);
                    }
                }
            }
            rr::Event::OutboundFailure { request_id, .. } => {
                if let Some(p) = self.outgoing.remove(&request_id) {
                    let _ = p.reply.send(Err(Error::Unavailable));
                }
            }
            rr::Event::InboundFailure {
                peer, request_id, ..
            }
            | rr::Event::ResponseSent {
                peer, request_id, ..
            } => {
                if self.incoming.get(&peer).is_some_and(|p| p.id == request_id) {
                    self.incoming.remove(&peer);
                }
            }
        }
    }
    fn prekey_tick(&mut self) {
        let peers: Vec<_> = self
            .outgoing
            .values()
            .filter_map(|p| {
                (p.reply.is_closed() || !p.cap.is_current() || Instant::now() >= p.deadline)
                    .then_some(p.peer)
            })
            .collect();
        for peer in peers {
            self.revoke(peer, Error::Cancelled);
        }
        let peers: Vec<_> = self.incoming.keys().copied().collect();
        for peer in peers {
            let p = self.incoming.get_mut(&peer).unwrap();
            if !p.cap.is_current() || Instant::now() >= p.deadline {
                self.revoke(peer, Error::Expired);
                continue;
            }
            if p.channel.is_none() {
                continue;
            }
            match p.response.try_recv() {
                Ok(response) => {
                    if !matching_response(&p.request, &response)
                        || self
                            .contacts
                            .get(&peer)
                            .is_none_or(|c| p.cap.authenticated_peer(c).is_err())
                    {
                        self.revoke(peer, Error::Untrusted);
                        continue;
                    }
                    let channel = p.channel.take().unwrap();
                    let _ = self
                        .swarm
                        .behaviour_mut()
                        .exchange
                        .send_response(channel, Packet::outgoing(response));
                }
                Err(oneshot::error::TryRecvError::Closed) => {
                    self.incoming.remove(&peer);
                }
                Err(oneshot::error::TryRecvError::Empty) => {}
            }
        }
    }
    fn network(&mut self, event: SwarmEvent<WireEvent>) {
        match event {
            SwarmEvent::Behaviour(event) => self.prekey_event(event),
            SwarmEvent::NewListenAddr { address, .. } => {
                if self.addresses.len() < 2 {
                    self.addresses.push(address.clone());
                    self.emit(Event::Listening(address));
                }
            }
            SwarmEvent::ExpiredListenAddr { address, .. } => {
                self.addresses.retain(|a| a != &address)
            }
            SwarmEvent::ConnectionEstablished {
                peer_id,
                connection_id,
                endpoint,
                ..
            } => {
                let Some(contact) = self.contacts.get(&peer_id) else {
                    self.swarm.close_connection(connection_id);
                    return;
                };
                let Some(pin) = contact.pinned() else {
                    self.swarm.close_connection(connection_id);
                    return;
                };
                if send_decision(
                    Some(contact),
                    *pin.device_key(),
                    Some(pin.binding()),
                    Some(&peer_id),
                ) != SendDecision::Allow
                {
                    self.swarm.close_connection(connection_id);
                    self.revoke(peer_id, Error::Untrusted);
                    return;
                }
                if endpoint.is_dialer()
                    && !self
                        .dials
                        .get(&peer_id)
                        .is_some_and(|d| d.id == Some(connection_id) && !d.aborting)
                {
                    self.swarm.close_connection(connection_id);
                    return;
                }
                if self.live.contains_key(&peer_id) {
                    self.swarm.close_connection(connection_id);
                    return;
                }
                let device = *pin.device_key();
                self.live.insert(
                    peer_id,
                    Live {
                        id: connection_id,
                        device,
                        generation: contact.generation(),
                        flag: Arc::new(AtomicBool::new(true)),
                        expires: Instant::now() + self.config.capability_ttl,
                    },
                );
                if let Some(dial) = self.dials.remove(&peer_id) {
                    let _ = dial.reply.send(self.capability(peer_id));
                }
                self.emit(Event::Connected(device));
            }
            SwarmEvent::ConnectionClosed {
                peer_id,
                connection_id,
                ..
            } => {
                if self
                    .live
                    .get(&peer_id)
                    .is_some_and(|c| c.id == connection_id)
                {
                    self.revoke(peer_id, Error::Unavailable);
                }
            }
            SwarmEvent::OutgoingConnectionError {
                peer_id: Some(peer),
                connection_id,
                error,
                ..
            } => {
                if !self
                    .dials
                    .get(&peer)
                    .is_some_and(|d| d.id == Some(connection_id))
                {
                    return;
                }
                if (matches!(error, DialError::Aborted)
                    && self.dials.get(&peer).is_some_and(|d| d.aborting))
                    || network_failure(&error)
                {
                    self.emit(Event::AttemptFailed(Error::Unavailable));
                    self.attempt(peer);
                } else {
                    self.revoke(peer, Error::Untrusted);
                }
            }
            SwarmEvent::ListenerClosed { reason: Err(_), .. }
            | SwarmEvent::ListenerError { .. } => {
                self.emit(Event::AttemptFailed(Error::Unavailable))
            }
            _ => {}
        }
    }
    pub async fn run(mut self, mut stop: watch::Receiver<bool>) -> Result<(), Error> {
        let mut tick = tokio::time::interval(Duration::from_millis(10));
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            if *stop.borrow() || !self.revocation.current() {
                break;
            }
            tokio::select! {
                changed = stop.changed() => { if changed.is_err() || *stop.borrow_and_update() { break; } }
                _ = tick.tick() => self.tick(),
                command = self.commands.recv() => match command { Some(c) => self.command(c), None => break },
                event = self.swarm.select_next_some() => self.network(event),
            }
        }
        self.revoke_all_inner(Error::Closed);
        self.emit(Event::Stopped);
        Ok(())
    }
}
fn prekey_capacity(pending: usize, same_direction: bool) -> bool {
    pending < 16 && !same_direction
}
fn request_devices(value: &Exchange) -> Option<([u8; 32], [u8; 32])> {
    match value {
        Exchange::Query(q) => Some((q.requester, q.publisher)),
        Exchange::Ack(a) => Some((a.sender, a.recipient)),
        _ => None,
    }
}
fn matching_response(request: &Exchange, response: &Exchange) -> bool {
    if request.request_id() != response.request_id() {
        return false;
    }
    match (request, response) {
        (Exchange::Query(q), Exchange::Selection(s)) => {
            q.requester == s.recipient
                && q.publisher == s.publisher
                && s.bundle_generation >= q.minimum_generation
        }
        (Exchange::Ack(a), Exchange::AckResult(b)) => a == b,
        (_, Exchange::Error { .. }) => true,
        _ => false,
    }
}
fn network_failure(error: &DialError) -> bool {
    // WrongPeerId/denied/security/protocol errors never authorize downgrade.
    let DialError::Transport(errors) = error else {
        return false;
    };
    errors.iter().all(|(_, e)| {
        let mut source: Option<&(dyn std::error::Error + 'static)> = Some(e);
        for _ in 0..16 {
            let Some(e) = source else {
                break;
            };
            if let Some(io) = e.downcast_ref::<std::io::Error>() {
                if matches!(
                    io.kind(),
                    std::io::ErrorKind::ConnectionRefused
                        | std::io::ErrorKind::ConnectionReset
                        | std::io::ErrorKind::TimedOut
                        | std::io::ErrorKind::NetworkUnreachable
                        | std::io::ErrorKind::HostUnreachable
                ) {
                    return true;
                }
                source = io
                    .get_ref()
                    .map(|v| v as &(dyn std::error::Error + 'static));
            } else {
                source = e.source();
            }
        }
        false
    })
}
#[cfg(test)]
mod prekey_limit_tests {
    #[test]
    fn peer_direction_and_global_admission_are_independent_and_bounded() {
        assert!(super::prekey_capacity(0, false));
        assert!(super::prekey_capacity(15, false));
        assert!(!super::prekey_capacity(16, false));
        assert!(!super::prekey_capacity(usize::MAX, false));
        assert!(!super::prekey_capacity(0, true));
        assert!(!super::prekey_capacity(15, true));
    }
}
