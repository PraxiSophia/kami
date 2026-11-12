use crate::Error;
use kami_identity::{send_decision, ContactTrust, SendDecision};
use libp2p::PeerId;
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Instant,
};

/// Monotonic invalidation only, NOT identity, enrollment or lease evidence.
/// A new handle cannot mint a connection or revive any revoked capability.
#[derive(Clone)]
pub struct Revocation(Arc<AtomicBool>);
impl Revocation {
    pub fn new() -> Self {
        Self(Arc::new(AtomicBool::new(true)))
    }
    pub fn revoke(&self) {
        self.0.store(false, Ordering::Release);
    }
    pub(crate) fn current(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}
impl Default for Revocation {
    fn default() -> Self {
        Self::new()
    }
}

/// Only a real secure Swarm connection can create this capability. Public
/// descriptors, invitation fields and relay identities are not constructors.
/// ```compile_fail
/// use kami_transport::AuthenticatedConnection;
/// let cap = AuthenticatedConnection::default();
/// ```
/// ```compile_fail
/// use kami_transport::AuthenticatedConnection;
/// fn duplicate(c: AuthenticatedConnection) { let _ = c.clone(); }
/// ```
pub struct AuthenticatedConnection {
    pub(crate) authority: Revocation,
    pub(crate) peer: PeerId,
    pub(crate) device: [u8; 32],
    pub(crate) generation: u64,
    pub(crate) live: Arc<AtomicBool>,
    pub(crate) expires: Instant,
}
impl AuthenticatedConnection {
    pub(crate) fn retained(&self) -> Self {
        Self {
            authority: self.authority.clone(),
            peer: self.peer,
            device: self.device,
            generation: self.generation,
            live: self.live.clone(),
            expires: self.expires,
        }
    }
    pub fn remote_device(&self) -> [u8; 32] {
        self.device
    }
    pub fn is_current(&self) -> bool {
        self.authority.current()
            && self.live.load(Ordering::Acquire)
            && Instant::now() < self.expires
    }
    /// Fresh durable trust is mandatory at every consequential operation. This
    /// returns only the actual secure end-peer, never a caller-asserted PeerId.
    pub fn authenticated_peer(&self, fresh: &ContactTrust) -> Result<PeerId, Error> {
        if !self.is_current() {
            return Err(Error::Expired);
        }
        if fresh.generation() != self.generation {
            return Err(Error::Untrusted);
        }
        let proof = fresh.pinned().map(|pin| pin.binding());
        if send_decision(Some(fresh), self.device, proof, Some(&self.peer)) != SendDecision::Allow {
            return Err(Error::Untrusted);
        }
        Ok(self.peer)
    }
}
