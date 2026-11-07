use crate::Error;
use kami_e2ee::QuarantinedSession;
use kami_identity::ContactTrust;
use kami_storage::{Preparation, QueuedSend, SendRequest, Storage, StorageError};
use libp2p_identity::PeerId;

pub enum EnqueueOutcome {
    /// Install the proposed in-memory session only after this durable confirmation.
    Committed {
        message: QueuedSend,
        proposed_session: Box<QuarantinedSession>,
    },
    /// Exact stored bytes only. Restoring a crypto session is NOT implemented here.
    Recovered(QueuedSend),
}
/// Synchronous runtime-neutral coordinator for the quarantined authenticated seam.
/// Run on a blocking service task. Peer/trust inputs are caller evidence, not live
/// end-peer provenance; this API performs no transmission or timer scheduling.
pub struct Coordinator<'a> {
    pub(crate) storage: &'a Storage,
}
impl<'a> Coordinator<'a> {
    pub fn new(storage: &'a Storage) -> Self {
        Self { storage }
    }
    pub fn enqueue(
        &self,
        request: SendRequest,
        session: &QuarantinedSession,
        trust: &ContactTrust,
        peer: &PeerId,
        now_ms: i64,
    ) -> Result<EnqueueOutcome, Error> {
        if now_ms < 0 {
            return Err(Error::Storage(StorageError::InvalidInput));
        }
        self.storage.cleanup_prepared(now_ms)?;
        let reservation = match self.storage.prepare_send(request.clone(), now_ms)? {
            Preparation::Committed(message) => return Ok(EnqueueOutcome::Recovered(message)),
            Preparation::Prepared(p) => p,
        };
        let fresh = self.fresh_trust(request.recipient)?;
        if fresh.generation() != trust.generation() || fresh.pinned() != trust.pinned() {
            return Err(Error::IdentityHeld);
        }
        let (stage, proposed) = session
            .prepare_text(
                request.message,
                reservation.sequence(),
                request.created_at_ms,
                request.text(),
                trust,
                peer,
                now_ms as u64,
            )
            .map_err(|_| Error::CryptoRejected)?;
        let facts = stage.facts();
        if facts.sender() != request.sender
            || facts.recipient() != request.recipient
            || facts.conversation_id() != request.conversation
            || facts.message_id() != request.message
            || facts.sequence() != reservation.sequence()
        {
            return Err(Error::RequestMismatch);
        }
        let result = self
            .storage
            .commit_prepared(reservation.clone(), stage, now_ms);
        // A successful write reply is still checked against the durable immutable row.
        match self.storage.prepare_send(request, now_ms) {
            Ok(Preparation::Committed(message)) => {
                // A competing/lost reply does not prove this proposal is the one
                // persisted. Return only stored bytes, never install a guessed ratchet.
                if result.is_err() {
                    return Ok(EnqueueOutcome::Recovered(message));
                }
                if result
                    .as_ref()
                    .is_ok_and(|frame| frame.as_slice() != message.frame())
                {
                    return Err(Error::Indeterminate(StorageError::Conflict));
                }
                Ok(EnqueueOutcome::Committed {
                    message,
                    proposed_session: Box::new(proposed),
                })
            }
            Ok(Preparation::Prepared(p)) if p == reservation => Err(Error::RolledBack(
                result.err().unwrap_or(StorageError::WrongKeyOrCorrupt),
            )),
            Ok(_) => Err(Error::Indeterminate(StorageError::Conflict)),
            Err(e) => Err(Error::Indeterminate(e)),
        }
    }
}
