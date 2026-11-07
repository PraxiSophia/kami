//! No production authority factory exists. KAMI-27 must co-locate its actual
//! authenticated end-peer handler here (or replace this with producer-owned
//! opaque evidence). This private seam is exercised only by in-crate mocks.
#![allow(dead_code)]
use crate::Error;
use kami_core::{ConversationId, DeviceId, MessageId};
use kami_e2ee::QuarantinedSession;
use kami_identity::ContactTrust;
use kami_storage::{Storage, TrustState};
use libp2p_identity::PeerId;
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc,
};

pub(crate) struct AuthenticatedPeer {
    end_peer: PeerId,
    local: DeviceId,
    remote: DeviceId,
    conversation: ConversationId,
    trust: ContactTrust,
    epoch: u64,
    current_epoch: Arc<AtomicU64>,
    connected: Arc<AtomicBool>,
    valid_until_ms: u64,
    operation: Operation,
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Operation {
    Enqueue(MessageId),
    Claim(MessageId),
    Receive(MessageId),
    Receipt(MessageId),
}
impl AuthenticatedPeer {
    fn admit(&self, storage: &Storage, operation: Operation, now: u64) -> Result<(), Error> {
        if self.operation != operation
            || now > i64::MAX as u64
            || now >= self.valid_until_ms
            || !self.connected.load(Ordering::Acquire)
            || self.current_epoch.load(Ordering::Acquire) != self.epoch
        {
            return Err(Error::LiveTransportUnavailable);
        }
        let fresh = storage
            .load_contact(*self.remote.as_bytes())
            .map_err(Error::Storage)?
            .ok_or(Error::LiveTransportUnavailable)?;
        if fresh.state() != TrustState::Verified
            || fresh.generation() != self.trust.generation()
            || fresh.pinned() != self.trust.pinned().map(|p| p.to_bytes()).as_deref()
            || !matches!(
                kami_identity::send_decision(
                    Some(&self.trust),
                    *self.remote.as_bytes(),
                    self.trust.pinned().map(|p| p.binding()),
                    Some(&self.end_peer)
                ),
                kami_identity::SendDecision::Allow
            )
        {
            return Err(Error::LiveTransportUnavailable);
        }
        Ok(())
    }
    pub(crate) fn enqueue(
        &self,
        storage: &Storage,
        request: kami_sender::SendRequest,
        session: &QuarantinedSession,
        now: u64,
    ) -> Result<kami_sender::EnqueueOutcome, Error> {
        self.admit(storage, Operation::Enqueue(request.message), now)?;
        if request.sender != self.local
            || request.recipient != self.remote
            || request.conversation != self.conversation
        {
            return Err(Error::LiveTransportUnavailable);
        }
        kami_sender::Coordinator::new(storage)
            .enqueue(request, session, &self.trust, &self.end_peer, now as i64)
            .map_err(sender_error)
    }
    pub(crate) fn claim(
        &self,
        storage: &Storage,
        id: MessageId,
        now: u64,
    ) -> Result<Option<kami_sender::SendLease>, Error> {
        self.admit(storage, Operation::Claim(id), now)?;
        let row = storage
            .sender_record(self.local, id)
            .map_err(Error::Storage)?;
        if row.facts().recipient() != self.remote
            || row.facts().conversation_id() != self.conversation
        {
            return Err(Error::OperationRejected);
        }
        kami_sender::Coordinator::new(storage)
            .claim(self.local, id, &self.end_peer, now as i64)
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
        self.admit(storage, Operation::Receive(outer.message_id()), now)?;
        let context = kami_receiver::ReceiveContext::new(
            self.local,
            self.remote,
            self.conversation,
            &self.trust,
            &self.end_peer,
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
        self.admit(storage, Operation::Receipt(outer.message_id()), now)?;
        if outer.sender() != self.remote
            || outer.recipient() != self.local
            || outer.conversation_id() != self.conversation
        {
            return Err(Error::OperationRejected);
        }
        kami_sender::Coordinator::new(storage)
            .receive_receipt(frame, session, &self.end_peer, now)
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
    /// Not live authority: the only factory is excluded from production builds.
    pub(crate) fn mock(
        local: DeviceId,
        remote: DeviceId,
        conversation: ConversationId,
        trust: ContactTrust,
        operation: Operation,
    ) -> Self {
        Self {
            end_peer: trust.pinned().unwrap().binding().peer_id(),
            local,
            remote,
            conversation,
            trust,
            epoch: 1,
            current_epoch: Arc::new(AtomicU64::new(1)),
            connected: Arc::new(AtomicBool::new(true)),
            valid_until_ms: 2000,
            operation,
        }
    }
    pub(crate) fn disconnect(&self) {
        self.connected.store(false, Ordering::Release);
    }
    pub(crate) fn change_epoch(&self) {
        self.current_epoch.fetch_add(1, Ordering::Release);
    }
    pub(crate) fn substitute_peer(&mut self, peer: PeerId) {
        self.end_peer = peer;
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
