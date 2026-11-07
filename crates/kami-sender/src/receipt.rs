//! Receipt-only authenticated receive path; never text inbox finalization.
use crate::{Coordinator, Error};
use kami_core::MessageFacts;
use kami_e2ee::{QuarantinedSession, ReplayPermit};
use kami_protocol::{decode_frame, ExpectedContext, MessageKind};
use libp2p_identity::PeerId;

pub enum ReceiptOutcome {
    Delivered {
        facts: MessageFacts,
        proposed_session: Box<QuarantinedSession>,
    },
    Duplicate(MessageFacts),
}
impl Coordinator<'_> {
    pub fn receive_receipt(
        &self,
        frame: &[u8],
        session: Option<&QuarantinedSession>,
        end_peer: &PeerId,
        now: u64,
    ) -> Result<ReceiptOutcome, Error> {
        let outer = decode_frame(frame).map_err(|_| Error::CryptoRejected)?;
        if outer.kind() != MessageKind::DeviceDelivered {
            return Err(Error::CryptoRejected);
        }
        let id = outer.receipt_target().ok_or(Error::CryptoRejected)?;
        let target = self.storage.sender_record(outer.recipient(), id)?;
        let facts = target.facts();
        if outer.sender() != facts.recipient() || outer.conversation_id() != facts.conversation_id()
        {
            return Err(Error::RequestMismatch);
        }
        let trust = self.fresh_trust(facts.recipient())?;
        let permit = || {
            ReplayPermit::new(
                &trust,
                end_peer,
                facts.recipient(),
                facts.sender(),
                facts.conversation_id(),
            )
            .map_err(|_| Error::IdentityHeld)
        };
        if let Some(facts) = self
            .storage
            .recover_delivery_receipt(permit()?, frame.to_vec())?
        {
            return Ok(ReceiptOutcome::Duplicate(facts));
        }
        let expected = ExpectedContext {
            message_id: outer.message_id(),
            conversation_id: facts.conversation_id(),
            sender: facts.recipient(),
            recipient: facts.sender(),
            receipt_target: Some(id),
        };
        let session = session.ok_or(Error::CryptoRejected)?;
        let (stage, proposed) = session
            .prepare_receipt(
                frame,
                &expected,
                facts,
                target.frame(),
                &trust,
                end_peer,
                now,
            )
            .map_err(|_| Error::CryptoRejected)?;
        let result = self.storage.commit_delivery_receipt(stage);
        match self
            .storage
            .recover_delivery_receipt(permit()?, frame.to_vec())
            .map_err(Error::Indeterminate)?
        {
            Some(f) if matches!(result, Ok(kami_storage::Acceptance::New)) => {
                Ok(ReceiptOutcome::Delivered {
                    facts: f,
                    proposed_session: Box::new(proposed),
                })
            }
            Some(f) => Ok(ReceiptOutcome::Duplicate(f)),
            None => Err(Error::RolledBack(
                result.err().unwrap_or(kami_storage::StorageError::Conflict),
            )),
        }
    }
}
