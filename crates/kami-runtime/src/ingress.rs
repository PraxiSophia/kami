//! Private, non-activated application-operation boundary. Live evidence can only
//! be minted by the owning Engine's direct dial path; it is never a public API.
#![allow(dead_code)]
use crate::{engine::IngressConnection, Error};
use kami_core::{ConversationId, DeviceId, MessageId};
use kami_e2ee::QuarantinedSession;
use kami_identity::ContactTrust;
use kami_storage::Storage;
#[cfg(test)]
use kami_storage::TrustState;
use libp2p_identity::PeerId;
#[cfg(test)]
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

pub(crate) struct AuthenticatedPeer {
    evidence: Evidence,
    local: DeviceId,
    remote: DeviceId,
    conversation: ConversationId,
    operation: Operation,
}
enum Evidence {
    Live(Arc<IngressConnection>),
    #[cfg(test)]
    Mock {
        end_peer: PeerId,
        trust: Box<ContactTrust>,
        epoch: u64,
        current_epoch: Arc<AtomicU64>,
        connected: Arc<AtomicBool>,
        valid_until_ms: u64,
    },
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Operation {
    Enqueue(MessageId),
    Claim(MessageId),
    Receive(MessageId),
    Receipt(MessageId),
}
/// No session or plaintext input: this slice cannot establish sessions or encrypt
/// new application messages. Existing exact artifacts use the existing contracts.
pub(crate) enum Action {
    Claim(MessageId),
    Receive {
        frame: Vec<u8>,
        receipt: MessageId,
    },
    Receipt(Vec<u8>),
    #[cfg(test)]
    Probe(TestProbe),
}
#[cfg(test)]
pub(crate) type TestProbe =
    Box<dyn FnOnce(&Storage, DeviceId, DeviceId) -> Result<usize, Error> + Send>;
pub(crate) enum Outcome {
    Claim(Option<kami_sender::SendLease>),
    Receive(kami_receiver::ReceiveOutcome),
    Receipt(kami_sender::ReceiptOutcome),
    #[cfg(test)]
    Probe(usize),
}
impl AuthenticatedPeer {
    // IngressConnection's fields and constructor are private to engine.rs.
    pub(crate) fn from_runtime(
        connection: IngressConnection,
        conversation: ConversationId,
        operation: Operation,
    ) -> Self {
        Self {
            local: connection.local(),
            remote: DeviceId::from_bytes(connection.remote()),
            evidence: Evidence::Live(Arc::new(connection)),
            conversation,
            operation,
        }
    }
    pub(crate) fn local(&self) -> DeviceId {
        self.local
    }
    pub(crate) fn remote(&self) -> DeviceId {
        self.remote
    }
    pub(crate) fn monitor(&self) -> Option<Arc<IngressConnection>> {
        match &self.evidence {
            Evidence::Live(connection) => Some(connection.clone()),
            #[cfg(test)]
            Evidence::Mock { .. } => None,
        }
    }
    pub(crate) fn owned_by(&self, engine: &crate::Engine) -> bool {
        matches!(&self.evidence, Evidence::Live(connection) if connection.belongs_to(engine))
    }
    pub(crate) fn live(&self) -> bool {
        matches!(&self.evidence, Evidence::Live(connection) if connection.current())
    }
    pub(crate) fn admit(
        &self,
        storage: &Storage,
        operation: Operation,
        now: u64,
    ) -> Result<(ContactTrust, PeerId), Error> {
        if self.operation != operation || now > i64::MAX as u64 {
            return Err(Error::LiveTransportUnavailable);
        }
        match &self.evidence {
            Evidence::Live(connection) => {
                if !connection.current() || connection.remote() != *self.remote.as_bytes() {
                    return Err(Error::LiveTransportUnavailable);
                }
                let fresh = crate::worker::contact(storage, *self.remote.as_bytes())?;
                let peer = connection.authenticated_peer(&fresh)?;
                Ok((fresh, peer))
            }
            #[cfg(test)]
            Evidence::Mock {
                end_peer,
                trust,
                epoch,
                current_epoch,
                connected,
                valid_until_ms,
            } => {
                if now >= *valid_until_ms
                    || !connected.load(Ordering::Acquire)
                    || current_epoch.load(Ordering::Acquire) != *epoch
                {
                    return Err(Error::LiveTransportUnavailable);
                }
                let fresh = storage
                    .load_contact(*self.remote.as_bytes())
                    .map_err(Error::Storage)?
                    .ok_or(Error::LiveTransportUnavailable)?;
                if fresh.state() != TrustState::Verified
                    || fresh.generation() != trust.generation()
                    || fresh.pinned() != trust.pinned().map(|p| p.to_bytes()).as_deref()
                    || !matches!(
                        kami_identity::send_decision(
                            Some(trust),
                            *self.remote.as_bytes(),
                            trust.pinned().map(|p| p.binding()),
                            Some(end_peer)
                        ),
                        kami_identity::SendDecision::Allow
                    )
                {
                    return Err(Error::LiveTransportUnavailable);
                }
                Ok((trust.as_ref().clone(), *end_peer))
            }
        }
    }
    pub(crate) fn check(&self, storage: &Storage, now: u64) -> Result<(), Error> {
        self.admit(storage, self.operation, now).map(|_| ())
    }
    pub(crate) fn execute(
        &self,
        storage: &Storage,
        action: Action,
        now: u64,
    ) -> Result<Outcome, Error> {
        match action {
            Action::Claim(id) => self.claim(storage, id, now).map(Outcome::Claim),
            Action::Receive { frame, receipt } => self
                .receive(storage, &frame, None, receipt, now)
                .map(Outcome::Receive),
            Action::Receipt(frame) => self
                .receipt(storage, &frame, None, now)
                .map(Outcome::Receipt),
            #[cfg(test)]
            Action::Probe(probe) => probe(storage, self.local, self.remote).map(Outcome::Probe),
        }
    }
    #[cfg(test)]
    pub(crate) fn enqueue(
        &self,
        storage: &Storage,
        request: kami_sender::SendRequest,
        session: &QuarantinedSession,
        now: u64,
    ) -> Result<kami_sender::EnqueueOutcome, Error> {
        let (trust, peer) = self.admit(storage, Operation::Enqueue(request.message), now)?;
        if request.sender != self.local
            || request.recipient != self.remote
            || request.conversation != self.conversation
        {
            return Err(Error::LiveTransportUnavailable);
        }
        kami_sender::Coordinator::new(storage)
            .enqueue(request, session, &trust, &peer, now as i64)
            .map_err(sender_error)
    }
    pub(crate) fn claim(
        &self,
        storage: &Storage,
        id: MessageId,
        now: u64,
    ) -> Result<Option<kami_sender::SendLease>, Error> {
        let (_, peer) = self.admit(storage, Operation::Claim(id), now)?;
        let row = storage
            .sender_record(self.local, id)
            .map_err(Error::Storage)?;
        if row.facts().recipient() != self.remote
            || row.facts().conversation_id() != self.conversation
        {
            return Err(Error::OperationRejected);
        }
        kami_sender::Coordinator::new(storage)
            .claim(self.local, id, &peer, now as i64)
            .map_err(sender_error)
    }
    pub(crate) fn receive(
        &self,
        storage: &Storage,
        frame: &[u8],
        session: Option<&QuarantinedSession>,
        receipt: MessageId,
        now: u64,
    ) -> Result<kami_receiver::ReceiveOutcome, Error> {
        let outer = kami_protocol::decode_frame(frame).map_err(|_| Error::OperationRejected)?;
        let (trust, peer) = self.admit(storage, Operation::Receive(outer.message_id()), now)?;
        let context = kami_receiver::ReceiveContext::new(
            self.local,
            self.remote,
            self.conversation,
            &trust,
            &peer,
            now,
        )
        .map_err(|_| Error::OperationRejected)?;
        kami_receiver::Receiver::new(storage)
            .receive(frame, context, session, receipt)
            .map_err(receiver_error)
    }
    pub(crate) fn receipt(
        &self,
        storage: &Storage,
        frame: &[u8],
        session: Option<&QuarantinedSession>,
        now: u64,
    ) -> Result<kami_sender::ReceiptOutcome, Error> {
        let outer = kami_protocol::decode_frame(frame).map_err(|_| Error::OperationRejected)?;
        let (_, peer) = self.admit(storage, Operation::Receipt(outer.message_id()), now)?;
        if outer.sender() != self.remote
            || outer.recipient() != self.local
            || outer.conversation_id() != self.conversation
        {
            return Err(Error::OperationRejected);
        }
        kami_sender::Coordinator::new(storage)
            .receive_receipt(frame, session, &peer, now)
            .map_err(sender_error)
    }
}
fn sender_error(error: kami_sender::Error) -> Error {
    match error {
        kami_sender::Error::Indeterminate(_) => Error::Indeterminate,
        kami_sender::Error::RolledBack(_) => Error::RolledBack,
        kami_sender::Error::Storage(e) => Error::Storage(e),
        _ => Error::OperationRejected,
    }
}
fn receiver_error(error: kami_receiver::Error) -> Error {
    match error {
        kami_receiver::Error::Indeterminate(_) => Error::Indeterminate,
        kami_receiver::Error::Storage(e) => Error::Storage(e),
        _ => Error::OperationRejected,
    }
}
#[cfg(test)]
impl AuthenticatedPeer {
    pub(crate) fn mock(
        local: DeviceId,
        remote: DeviceId,
        conversation: ConversationId,
        trust: ContactTrust,
        operation: Operation,
    ) -> Self {
        Self {
            evidence: Evidence::Mock {
                end_peer: trust.pinned().unwrap().binding().peer_id(),
                trust: Box::new(trust),
                epoch: 1,
                current_epoch: Arc::new(AtomicU64::new(1)),
                connected: Arc::new(AtomicBool::new(true)),
                valid_until_ms: 2000,
            },
            local,
            remote,
            conversation,
            operation,
        }
    }
    pub(crate) fn disconnect(&self) {
        if let Evidence::Mock { connected, .. } = &self.evidence {
            connected.store(false, Ordering::Release);
        }
    }
    pub(crate) fn change_epoch(&self) {
        if let Evidence::Mock { current_epoch, .. } = &self.evidence {
            current_epoch.fetch_add(1, Ordering::Release);
        }
    }
    pub(crate) fn substitute_peer(&mut self, peer: PeerId) {
        if let Evidence::Mock { end_peer, .. } = &mut self.evidence {
            *end_peer = peer;
        }
    }
}
#[cfg(test)]
mod error_tests {
    use super::*;
    #[test]
    fn ambiguous_outcome_is_not_reported_as_rejection_or_rollback() {
        assert_eq!(
            sender_error(kami_sender::Error::Indeterminate(
                kami_storage::StorageError::Closed
            )),
            Error::Indeterminate
        );
        assert_eq!(
            sender_error(kami_sender::Error::RolledBack(
                kami_storage::StorageError::Full
            )),
            Error::RolledBack
        );
        assert_eq!(
            receiver_error(kami_receiver::Error::Indeterminate(
                kami_storage::StorageError::Closed
            )),
            Error::Indeterminate
        );
    }
}
