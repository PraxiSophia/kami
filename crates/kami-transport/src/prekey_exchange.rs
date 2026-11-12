//! Fixed prekey-only framing and behaviour; no application payload API.
use kami_protocol::prekey_exchange::{decode, encode, Exchange, MAX_BODY_BYTES};
use libp2p::{
    futures::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    request_response as rr,
};
use std::{
    io,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};

fn invalid() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, "invalid prekey frame")
}
async fn read_frame<T: AsyncRead + Unpin>(io: &mut T) -> io::Result<Exchange> {
    let mut prefix = [0; 4];
    io.read_exact(&mut prefix).await?;
    let size = u32::from_be_bytes(prefix) as usize;
    if size == 0 || size > MAX_BODY_BYTES {
        return Err(invalid());
    }
    let mut bytes = vec![0; size];
    io.read_exact(&mut bytes).await?;
    // Request/response substreams are half-closed after the one fixed frame.
    let mut trailing = [0; 1];
    if io.read(&mut trailing).await? != 0 {
        return Err(invalid());
    }
    decode(&bytes).map_err(|_| invalid())
}
async fn write_frame<T: AsyncWrite + Unpin>(io: &mut T, value: Exchange) -> io::Result<()> {
    let bytes = encode(&value).map_err(|_| invalid())?;
    io.write_all(&(bytes.len() as u32).to_be_bytes()).await?;
    io.write_all(&bytes).await?;
    io.close().await
}
struct Permit(Arc<AtomicUsize>);
impl Drop for Permit {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}
pub(crate) struct Packet {
    pub value: Exchange,
    _permit: Option<Permit>,
}
impl Packet {
    pub fn outgoing(value: Exchange) -> Self {
        Self {
            value,
            _permit: None,
        }
    }
}
#[derive(Clone, Default)]
pub(crate) struct Codec(Arc<AtomicUsize>);
impl Codec {
    async fn read<T: AsyncRead + Unpin>(&self, io: &mut T) -> io::Result<Packet> {
        self.0
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| {
                (n < 16).then_some(n + 1)
            })
            .map_err(|_| invalid())?;
        let permit = Permit(self.0.clone());
        let value = read_frame(io).await?;
        Ok(Packet {
            value,
            _permit: Some(permit),
        })
    }
}
impl rr::Codec for Codec {
    type Protocol = libp2p::StreamProtocol;
    type Request = Packet;
    type Response = Packet;
    async fn read_request<T: AsyncRead + Unpin + Send>(
        &mut self,
        _: &Self::Protocol,
        io: &mut T,
    ) -> io::Result<Packet> {
        let p = self.read(io).await?;
        if !matches!(p.value, Exchange::Query(_) | Exchange::Ack(_)) {
            return Err(invalid());
        }
        Ok(p)
    }
    async fn read_response<T: AsyncRead + Unpin + Send>(
        &mut self,
        _: &Self::Protocol,
        io: &mut T,
    ) -> io::Result<Packet> {
        let p = self.read(io).await?;
        if !matches!(
            p.value,
            Exchange::Selection(_) | Exchange::AckResult(_) | Exchange::Error { .. }
        ) {
            return Err(invalid());
        }
        Ok(p)
    }
    async fn write_request<T: AsyncWrite + Unpin + Send>(
        &mut self,
        _: &Self::Protocol,
        io: &mut T,
        p: Packet,
    ) -> io::Result<()> {
        write_frame(io, p.value).await
    }
    async fn write_response<T: AsyncWrite + Unpin + Send>(
        &mut self,
        _: &Self::Protocol,
        io: &mut T,
        p: Packet,
    ) -> io::Result<()> {
        write_frame(io, p.value).await
    }
}
pub(crate) type WireEvent = rr::Event<Packet, Packet>;
pub(crate) struct Behaviour {
    limits: libp2p::connection_limits::Behaviour,
    pub exchange: rr::Behaviour<Codec>,
}
impl Behaviour {
    pub fn new(limits: libp2p::connection_limits::ConnectionLimits) -> Self {
        Self {
            limits: libp2p::connection_limits::Behaviour::new(limits),
            exchange: rr::Behaviour::with_codec(
                Codec::default(),
                [(
                    libp2p::StreamProtocol::new("/kami/prekey-exchange/1"),
                    rr::ProtocolSupport::Full,
                )],
                rr::Config::default()
                    .with_request_timeout(Duration::from_secs(5))
                    .with_max_concurrent_streams(1),
            ),
        }
    }
}
// The limits behaviour has a dummy handler and always polls Pending. Delegate
// admission/events to it while retaining only request-response's real handler.
impl libp2p::swarm::NetworkBehaviour for Behaviour {
    type ConnectionHandler =
        <rr::Behaviour<Codec> as libp2p::swarm::NetworkBehaviour>::ConnectionHandler;
    type ToSwarm = WireEvent;
    fn handle_pending_inbound_connection(
        &mut self,
        id: libp2p::swarm::ConnectionId,
        local: &libp2p::Multiaddr,
        remote: &libp2p::Multiaddr,
    ) -> Result<(), libp2p::swarm::ConnectionDenied> {
        self.limits
            .handle_pending_inbound_connection(id, local, remote)?;
        self.exchange
            .handle_pending_inbound_connection(id, local, remote)
    }
    fn handle_pending_outbound_connection(
        &mut self,
        id: libp2p::swarm::ConnectionId,
        peer: Option<libp2p::PeerId>,
        addresses: &[libp2p::Multiaddr],
        role: libp2p::core::Endpoint,
    ) -> Result<Vec<libp2p::Multiaddr>, libp2p::swarm::ConnectionDenied> {
        self.limits
            .handle_pending_outbound_connection(id, peer, addresses, role)?;
        self.exchange
            .handle_pending_outbound_connection(id, peer, addresses, role)
    }
    fn handle_established_inbound_connection(
        &mut self,
        id: libp2p::swarm::ConnectionId,
        peer: libp2p::PeerId,
        local: &libp2p::Multiaddr,
        remote: &libp2p::Multiaddr,
    ) -> Result<libp2p::swarm::THandler<Self>, libp2p::swarm::ConnectionDenied> {
        self.limits
            .handle_established_inbound_connection(id, peer, local, remote)?;
        self.exchange
            .handle_established_inbound_connection(id, peer, local, remote)
    }
    fn handle_established_outbound_connection(
        &mut self,
        id: libp2p::swarm::ConnectionId,
        peer: libp2p::PeerId,
        address: &libp2p::Multiaddr,
        role: libp2p::core::Endpoint,
        port: libp2p::core::transport::PortUse,
    ) -> Result<libp2p::swarm::THandler<Self>, libp2p::swarm::ConnectionDenied> {
        self.limits
            .handle_established_outbound_connection(id, peer, address, role, port)?;
        self.exchange
            .handle_established_outbound_connection(id, peer, address, role, port)
    }
    fn on_swarm_event(&mut self, event: libp2p::swarm::FromSwarm) {
        self.limits.on_swarm_event(event);
        self.exchange.on_swarm_event(event);
    }
    fn on_connection_handler_event(
        &mut self,
        peer: libp2p::PeerId,
        id: libp2p::swarm::ConnectionId,
        event: libp2p::swarm::THandlerOutEvent<Self>,
    ) {
        self.exchange.on_connection_handler_event(peer, id, event);
    }
    fn poll(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<libp2p::swarm::ToSwarm<Self::ToSwarm, libp2p::swarm::THandlerInEvent<Self>>>
    {
        self.exchange.poll(cx)
    }
}

/// Nonconstructible evidence of these exact typed bytes on an admitted connection.
/// A detached signed proof or caller PeerId cannot construct this value.
pub struct LivePrekeyExchange {
    pub(crate) capability: crate::AuthenticatedConnection,
    pub(crate) value: Exchange,
    pub(crate) deadline: std::time::Instant,
}
impl LivePrekeyExchange {
    pub fn remote_device(&self) -> [u8; 32] {
        self.capability.device
    }
    pub fn inspect(
        &self,
        fresh: &kami_identity::ContactTrust,
    ) -> Result<(&Exchange, libp2p::PeerId), crate::Error> {
        if std::time::Instant::now() >= self.deadline {
            return Err(crate::Error::Expired);
        }
        Ok((&self.value, self.capability.authenticated_peer(fresh)?))
    }
    pub fn is_current(&self) -> bool {
        self.capability.is_current() && std::time::Instant::now() < self.deadline
    }
}
/// Reliable bounded request handoff, not a lossy diagnostic event.
pub struct IncomingPrekeyExchange {
    pub(crate) evidence: LivePrekeyExchange,
    pub(crate) reply: tokio::sync::oneshot::Sender<Exchange>,
}
impl IncomingPrekeyExchange {
    pub fn reject(self, failure: kami_protocol::prekey_exchange::Failure) {
        let value = Exchange::Error {
            request: *self.evidence.value.request_id(),
            failure,
        };
        let _ = self.reply.send(value);
    }
    pub fn evidence(&self) -> &LivePrekeyExchange {
        &self.evidence
    }
    pub fn respond(
        self,
        fresh: &kami_identity::ContactTrust,
        value: Exchange,
    ) -> Result<(), crate::Error> {
        self.evidence.inspect(fresh)?;
        if value.request_id() != self.evidence.value.request_id() {
            return Err(crate::Error::Untrusted);
        }
        self.reply.send(value).map_err(|_| crate::Error::Cancelled)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use libp2p::futures::{executor::block_on, io::Cursor};
    #[test]
    fn length_is_rejected_before_payload_read() {
        let mut io = Cursor::new((MAX_BODY_BYTES as u32 + 1).to_be_bytes());
        assert_eq!(
            block_on(read_frame(&mut io)).unwrap_err().kind(),
            std::io::ErrorKind::InvalidData
        );
    }
    #[test]
    fn framing_roundtrip_and_trailing_rejection() {
        let value = Exchange::Query(kami_protocol::prekey_exchange::Query {
            request: [1; 32],
            requester: [2; 32],
            publisher: [3; 32],
            minimum_generation: 1,
        });
        let mut io = Cursor::new(Vec::new());
        block_on(write_frame(&mut io, value.clone())).unwrap();
        io.set_position(0);
        assert_eq!(block_on(read_frame(&mut io)).unwrap(), value);
        io.get_mut().push(0);
        io.set_position(0);
        assert!(block_on(read_frame(&mut io)).is_err());
    }
}
