//! Deterministic eligibility and fresh identity checks, never live provenance.
use crate::{Coordinator, Error};
use kami_core::{DeviceId, MessageId};
use kami_identity::{send_decision, ContactTrust, ContactTrustState, SendDecision};
pub use kami_storage::{backoff_ms, ATTEMPT_LEASE_MS, MAX_ATTEMPTS};
use kami_storage::{SendLease, TrustState};
use libp2p_identity::PeerId;
impl Coordinator<'_> {
    pub(crate) fn fresh_trust(&self, recipient: DeviceId) -> Result<ContactTrust, Error> {
        let row = self
            .storage
            .load_contact(*recipient.as_bytes())?
            .ok_or(Error::IdentityHeld)?;
        let state = match row.state() {
            TrustState::Pending => ContactTrustState::Pending,
            TrustState::Verified => ContactTrustState::Verified,
            TrustState::Changed => ContactTrustState::Changed,
            TrustState::Blocked => ContactTrustState::Blocked,
        };
        ContactTrust::from_persisted(state, row.generation(), row.pinned(), row.candidate())
            .map_err(|_| Error::IdentityHeld)
    }
    fn retry_trust(
        &self,
        sender: DeviceId,
        id: MessageId,
        peer: &PeerId,
    ) -> Result<ContactTrust, Error> {
        let record = self.storage.sender_record(sender, id)?;
        let trust = self.fresh_trust(record.facts().recipient());
        let allowed = trust.as_ref().is_ok_and(|t| {
            matches!(
                send_decision(
                    Some(t),
                    *record.facts().recipient().as_bytes(),
                    t.pinned().map(|p| p.binding()),
                    Some(peer)
                ),
                SendDecision::Allow
            )
        });
        if !allowed {
            self.storage.hold_send(sender, id)?;
            return Err(Error::IdentityHeld);
        }
        trust
    }
    /// PeerId is caller evidence, not live provenance. Runtime serializes send/revocation.
    pub fn claim(
        &self,
        sender: DeviceId,
        id: MessageId,
        peer: &PeerId,
        now: i64,
    ) -> Result<Option<SendLease>, Error> {
        self.retry_trust(sender, id, peer)?;
        self.storage
            .claim_send(sender, id, now)
            .map_err(Error::from)
    }
    /// Explicit original-identity reverification, never replacement adoption.
    pub fn resume(
        &self,
        sender: DeviceId,
        id: MessageId,
        peer: &PeerId,
        now: i64,
    ) -> Result<(), Error> {
        let trust = self.retry_trust(sender, id, peer)?;
        self.storage
            .resume_send(sender, id, trust.generation(), now)
            .map_err(Error::from)
    }
}
