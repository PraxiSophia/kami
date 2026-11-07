use crate::{Error, ReceiveContext};
use kami_core::MessageId;
use kami_e2ee::{AuthenticatedDelivery, Endpoint, QuarantinedSession};
use kami_protocol::{decode_frame, ExpectedContext, MessageKind};
use kami_storage::{Acceptance, DurableDelivery, Storage, TrustState};
use vodozemac::olm::Account;

pub enum ReceiveOutcome {
    New {
        delivery: DurableDelivery,
        proposed_session: Box<QuarantinedSession>,
    },
    Duplicate {
        delivery: DurableDelivery,
    },
}
/// All methods are synchronous and use the single bounded storage worker.
/// A proposed session is installable ONLY after an unambiguous New result.
pub struct Receiver<'a> {
    storage: &'a Storage,
}
impl<'a> Receiver<'a> {
    pub fn new(storage: &'a Storage) -> Self {
        Self { storage }
    }
    fn admit(&self, frame: &[u8], ctx: &ReceiveContext<'_>) -> Result<ExpectedContext, Error> {
        let outer = decode_frame(frame).map_err(|_| Error::InvalidInput)?;
        if outer.kind() != MessageKind::Text
            || outer.sender() != ctx.remote
            || outer.recipient() != ctx.local
            || outer.conversation_id() != ctx.conversation
        {
            return Err(Error::Unauthorized);
        }
        let fresh = self
            .storage
            .load_contact(*ctx.remote.as_bytes())?
            .ok_or(Error::Unauthorized)?;
        if fresh.state() != TrustState::Verified
            || fresh.generation() != ctx.trust.generation()
            || fresh.pinned() != ctx.trust.pinned().map(|p| p.to_bytes()).as_deref()
        {
            return Err(Error::Unauthorized);
        }
        Ok(ExpectedContext {
            message_id: outer.message_id(),
            conversation_id: ctx.conversation,
            sender: ctx.remote,
            recipient: ctx.local,
            receipt_target: None,
        })
    }
    pub fn receive(
        &self,
        frame: &[u8],
        ctx: ReceiveContext<'_>,
        session: Option<&QuarantinedSession>,
        receipt_id: MessageId,
    ) -> Result<ReceiveOutcome, Error> {
        let expected = self.admit(frame, &ctx)?;
        if let Some(delivery) = self
            .storage
            .recover_delivery(ctx.permit()?, frame.to_vec())?
        {
            return Ok(ReceiveOutcome::Duplicate { delivery });
        }
        let session = session.ok_or(Error::SessionUnavailable)?;
        let (stage, proposed) = session
            .prepare_delivery(
                frame, &expected, receipt_id, ctx.trust, ctx.peer, ctx.now_ms,
            )
            .map_err(|_| Error::CryptoRejected)?;
        self.commit(frame, &ctx, stage, proposed)
    }
    #[allow(clippy::too_many_arguments)]
    pub fn receive_first(
        &self,
        frame: &[u8],
        ctx: ReceiveContext<'_>,
        account: Account,
        local: Endpoint,
        remote: Endpoint,
        receipt_id: MessageId,
    ) -> Result<ReceiveOutcome, Error> {
        let expected = self.admit(frame, &ctx)?;
        if let Some(delivery) = self
            .storage
            .recover_delivery(ctx.permit()?, frame.to_vec())?
        {
            return Ok(ReceiveOutcome::Duplicate { delivery });
        }
        let (stage, proposed) = QuarantinedSession::inbound_delivery(
            account,
            local,
            remote,
            ctx.conversation,
            frame,
            &expected,
            receipt_id,
            ctx.trust,
            ctx.peer,
            ctx.now_ms,
        )
        .map_err(|_| Error::CryptoRejected)?;
        self.commit(frame, &ctx, stage, proposed)
    }
    fn commit(
        &self,
        frame: &[u8],
        ctx: &ReceiveContext<'_>,
        stage: AuthenticatedDelivery,
        proposed: QuarantinedSession,
    ) -> Result<ReceiveOutcome, Error> {
        let result = self.storage.commit_delivery(stage);
        // Always read back exact durable evidence, including an ambiguous reply.
        let durable = self
            .storage
            .recover_delivery(ctx.permit()?, frame.to_vec())
            .map_err(Error::Indeterminate)?;
        match (result, durable) {
            (Ok(committed), Some(delivery)) if committed.receipt() == delivery.receipt() => {
                if committed.acceptance() == Acceptance::New {
                    Ok(ReceiveOutcome::New {
                        delivery,
                        proposed_session: Box::new(proposed),
                    })
                } else {
                    Ok(ReceiveOutcome::Duplicate { delivery })
                }
            }
            (Err(_), Some(delivery)) => Ok(ReceiveOutcome::Duplicate { delivery }),
            (Err(e), None) => Err(e.into()),
            _ => Err(Error::Indeterminate(kami_storage::StorageError::Conflict)),
        }
    }
}
