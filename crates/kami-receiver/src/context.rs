use crate::Error;
use kami_core::{ConversationId, DeviceId};
use kami_e2ee::ReplayPermit;
use kami_identity::ContactTrust;
use libp2p_identity::PeerId;

/// The trusted caller supplies the actual authenticated END peer, not a relay.
/// This is admission context, not proof that any incoming frame is authentic.
pub struct ReceiveContext<'a> {
    pub(crate) local: DeviceId,
    pub(crate) remote: DeviceId,
    pub(crate) conversation: ConversationId,
    pub(crate) trust: &'a ContactTrust,
    pub(crate) peer: &'a PeerId,
    pub(crate) now_ms: u64,
}
impl<'a> ReceiveContext<'a> {
    pub fn new(
        local: DeviceId,
        remote: DeviceId,
        conversation: ConversationId,
        trust: &'a ContactTrust,
        end_peer: &'a PeerId,
        now_ms: u64,
    ) -> Result<Self, Error> {
        ReplayPermit::new(trust, end_peer, remote, local, conversation)
            .map_err(|_| Error::Unauthorized)?;
        if now_ms > i64::MAX as u64 {
            return Err(Error::InvalidInput);
        }
        Ok(Self {
            local,
            remote,
            conversation,
            trust,
            peer: end_peer,
            now_ms,
        })
    }
    pub(crate) fn permit(&self) -> Result<ReplayPermit, Error> {
        ReplayPermit::new(
            self.trust,
            self.peer,
            self.remote,
            self.local,
            self.conversation,
        )
        .map_err(|_| Error::Unauthorized)
    }
}
