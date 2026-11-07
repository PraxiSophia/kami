//! Dedicated receipt stages in the existing bidirectional Olm session.
//! A delivery consumes two session generations; receipt consumption consumes one.
//! Neither type can pass through text finalization or the legacy inbox APIs.
use super::*;
use kami_core::{MessageFacts, MessageId};

const DOMAIN: &[u8] = b"KAMI-DEVICE-DELIVERED-V1\0";
pub struct ReplayPermit {
    remote: DeviceId,
    local: DeviceId,
    conversation: ConversationId,
    generation: u64,
    pin: Vec<u8>,
}
impl ReplayPermit {
    /// Only authorizes lookup of EXACT previously authenticated durable material.
    /// Caller supplies actual authenticated end-peer evidence, never relay metadata.
    pub fn new(
        trust: &ContactTrust,
        end_peer: &PeerId,
        remote: DeviceId,
        local: DeviceId,
        conversation: ConversationId,
    ) -> Result<Self, Error> {
        let pin = trust.pinned().ok_or(Error::AuthenticationRejected)?;
        if remote == local
            || conversation.as_u128() == 0
            || !matches!(
                kami_identity::send_decision(
                    Some(trust),
                    *remote.as_bytes(),
                    Some(pin.binding()),
                    Some(end_peer)
                ),
                kami_identity::SendDecision::Allow
            )
        {
            return Err(Error::AuthenticationRejected);
        }
        Ok(Self {
            remote,
            local,
            conversation,
            generation: trust.generation(),
            pin: pin.to_bytes(),
        })
    }
    pub fn into_context(self) -> (DeviceId, DeviceId, ConversationId, u64, Vec<u8>) {
        (
            self.remote,
            self.local,
            self.conversation,
            self.generation,
            self.pin,
        )
    }
}
pub struct AuthenticatedDelivery {
    inbound: AuthenticatedCommit,
    receipt: AuthenticatedReceiptCommit,
}
pub struct AuthenticatedReceiptCommit {
    material: CommitMaterial,
    target_frame_digest: [u8; 32],
}
impl AuthenticatedDelivery {
    pub fn facts(&self) -> MessageFacts {
        self.inbound.facts()
    }
    pub fn receipt_frame(&self) -> &[u8] {
        self.receipt.material.frame()
    }
    pub fn into_parts(self) -> (AuthenticatedCommit, AuthenticatedReceiptCommit) {
        (self.inbound, self.receipt)
    }
}
impl AuthenticatedReceiptCommit {
    pub fn facts(&self) -> MessageFacts {
        self.material.facts()
    }
    pub fn target_frame_digest(&self) -> [u8; 32] {
        self.target_frame_digest
    }
    /// Consuming custodian transfer, never a neutral reconstruction API.
    pub fn into_material(self) -> CommitMaterial {
        self.material
    }
}
fn binding(f: MessageFacts, digest: [u8; 32], generation: u64) -> Vec<u8> {
    let mut bytes = DOMAIN.to_vec();
    bytes.extend(f.message_id().as_u128().to_be_bytes());
    bytes.extend(f.conversation_id().as_u128().to_be_bytes());
    bytes.extend(f.sender().as_bytes());
    bytes.extend(f.recipient().as_bytes());
    bytes.extend(f.sequence().value().to_be_bytes());
    bytes.extend(f.content_binding().as_bytes());
    bytes.extend(digest);
    bytes.extend(generation.to_be_bytes());
    bytes
}
/// Recognize only the bounded domain/version-specific receipt payload.
/// Empty historical qualification receipts remain parseable but cannot finalize.
pub(crate) fn valid_payload(bytes: &[u8]) -> bool {
    bytes.len() == DOMAIN.len() + 176
        && bytes.starts_with(DOMAIN)
        && bytes[bytes.len() - 8..] != [0; 8]
}
impl QuarantinedSession {
    #[allow(clippy::too_many_arguments)]
    pub fn inbound_delivery(
        account: Account,
        local: Endpoint,
        remote: Endpoint,
        conversation: ConversationId,
        frame: &[u8],
        expected: &ExpectedContext,
        receipt_id: MessageId,
        trust: &ContactTrust,
        end_peer: &PeerId,
        now: u64,
    ) -> Result<(AuthenticatedDelivery, Self), Error> {
        let (inbound, proposed) = Self::inbound(
            account,
            local,
            remote,
            conversation,
            frame,
            expected,
            trust,
            end_peer,
            now,
        )?;
        proposed.finish_delivery(inbound, receipt_id, trust, end_peer, now)
    }
    #[allow(clippy::too_many_arguments)]
    pub fn prepare_delivery(
        &self,
        frame: &[u8],
        expected: &ExpectedContext,
        receipt_id: MessageId,
        trust: &ContactTrust,
        end_peer: &PeerId,
        now: u64,
    ) -> Result<(AuthenticatedDelivery, Self), Error> {
        let (inbound, proposed) = self.prepare_receive(frame, expected, trust, end_peer, now)?;
        proposed.finish_delivery(inbound, receipt_id, trust, end_peer, now)
    }
    fn finish_delivery(
        self,
        inbound: AuthenticatedCommit,
        receipt_id: MessageId,
        trust: &ContactTrust,
        end_peer: &PeerId,
        now: u64,
    ) -> Result<(AuthenticatedDelivery, Self), Error> {
        let target = inbound.facts();
        let digest: [u8; 32] = Sha256::digest(inbound.material.frame()).into();
        let a = &self.bound.anchor;
        if !inbound.material.incoming
            || inbound.material.next_digest()
                != self
                    .bound
                    .snapshot(self.session_generation, self.account_generation)?
                    .digest()
        {
            return Err(Error::StateEncoding);
        }
        let inner = InnerEnvelopeV1 {
            context: crate::ApplicationContext {
                conversation: a.conversation,
                message: receipt_id,
                sender: a.local.device,
                recipient: a.remote.device,
            },
            kind: kami_protocol::MessageKind::DeviceDelivered,
            sequence: None,
            receipt_target: Some(target.message_id()),
            created_at_ms: now,
            session_id: self.bound.session_id(),
            direction: if self.bound.initiator {
                Direction::InitiatorToResponder
            } else {
                Direction::ResponderToInitiator
            },
            sender_peer: a.local.selected.publisher_peer,
            recipient_peer: a.remote.selected.publisher_peer,
            sender_binding: a.local.digest(),
            recipient_binding: a.remote.digest(),
            payload: binding(target, digest, a.trust_generation),
        };
        let sent = self
            .bound
            .stage_send(&inner, Some(trust), Some(end_peer), now)?;
        let receipt = self.receipt_commit(
            &sent.session,
            &sent.inner,
            sent.frame,
            target,
            digest,
            false,
            trust,
            end_peer,
            now,
        )?;
        let mut proposed = self.proposed(sent.session)?;
        // Receipt evidence must never be recoverable as an ordinary text commit.
        proposed.retry = None;
        Ok((AuthenticatedDelivery { inbound, receipt }, proposed))
    }
    #[allow(clippy::too_many_arguments)]
    pub fn prepare_receipt(
        &self,
        frame: &[u8],
        expected: &ExpectedContext,
        target: MessageFacts,
        target_frame: &[u8],
        trust: &ContactTrust,
        end_peer: &PeerId,
        now: u64,
    ) -> Result<(AuthenticatedReceiptCommit, Self), Error> {
        if target_frame.is_empty() || target_frame.len() > kami_protocol::MAX_ENCODED_FRAME_SIZE {
            return Err(Error::AuthenticationRejected);
        }
        let stage = self
            .bound
            .stage_receive(frame, expected, Some(trust), Some(end_peer), now)?;
        let inner = &stage.inner;
        let digest = Sha256::digest(target_frame).into();
        if inner.kind != kami_protocol::MessageKind::DeviceDelivered
            || inner.receipt_target != Some(target.message_id())
            || inner.context.conversation != target.conversation_id()
            || inner.context.sender != target.recipient()
            || inner.context.recipient != target.sender()
            || !valid_payload(&inner.payload)
        {
            return Err(Error::AuthenticationRejected);
        }
        let issuer_generation = u64::from_be_bytes(
            inner.payload[inner.payload.len() - 8..]
                .try_into()
                .map_err(|_| Error::AuthenticationRejected)?,
        );
        // Trust generations are local counters, not synchronized peer counters.
        if inner.payload != binding(target, digest, issuer_generation) {
            return Err(Error::AuthenticationRejected);
        }
        let receipt = self.receipt_commit(
            &stage.session,
            inner,
            frame.to_vec(),
            target,
            digest,
            true,
            trust,
            end_peer,
            now,
        )?;
        Ok((receipt, self.proposed(stage.session)?))
    }
    #[allow(clippy::too_many_arguments)]
    fn receipt_commit(
        &self,
        next: &BoundSession,
        inner: &InnerEnvelopeV1,
        frame: Vec<u8>,
        facts: MessageFacts,
        target_frame_digest: [u8; 32],
        incoming: bool,
        trust: &ContactTrust,
        end_peer: &PeerId,
        now: u64,
    ) -> Result<AuthenticatedReceiptCommit, Error> {
        self.bound.anchor.verify(Some(trust), Some(end_peer), now)?;
        next.anchor
            .matches_inner(inner, &next.session, next.initiator, !incoming)?;
        inner.matches_frame(&decode_frame(&frame).map_err(|_| Error::AuthenticationRejected)?)?;
        if inner.kind != kami_protocol::MessageKind::DeviceDelivered
            || !valid_payload(&inner.payload)
            || self.session_generation == 0
        {
            return Err(Error::AuthenticationRejected);
        }
        let generation = next_generation(self.session_generation)?;
        let a = &self.bound.anchor;
        let material = CommitMaterial {
            account: crate::persistence::AccountSnapshot::capture(
                &self.account,
                &a.local,
                self.account_generation,
            )?,
            next_account: None,
            previous: Some(
                self.bound
                    .snapshot(self.session_generation, self.account_generation)?,
            ),
            next: next.snapshot(generation, self.account_generation)?,
            id: Sha256::digest(self.bound.session_id().as_bytes()).into(),
            contact: *a.remote.device.as_bytes(),
            trust_generation: a.trust_generation,
            pin: trust
                .pinned()
                .ok_or(Error::AuthenticationRejected)?
                .to_bytes(),
            context: BoundSession::persistence_context(a),
            facts,
            frame,
            payload: Zeroizing::new(inner.payload.clone()),
            incoming,
            account_generation: self.account_generation,
            session_generation: generation,
            received_at_ms: i64::try_from(now).map_err(|_| Error::StateEncoding)?,
        };
        Ok(AuthenticatedReceiptCommit {
            material,
            target_frame_digest,
        })
    }
}
