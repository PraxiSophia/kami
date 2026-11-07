//! Internal qualification format, not an enabled public encryption interface.
//! Fixed order/version, no extensions or optional tags. The outer hash is only
//! a correspondence commitment; authenticity comes from the verified Olm session.
use crate::{ApplicationContext, Error};
use kami_core::{ConversationId, DeviceId, MessageId, SenderSequence};
use kami_protocol::{FrameInput, MessageKind, StructurallyValidFrame};
use libp2p_identity::PeerId;
use sha2::{Digest, Sha256};
use std::fmt;
use zeroize::{Zeroize, Zeroizing};

const DOMAIN: &[u8] = b"KAMI-INNER-V1\0";
const COMMITMENT_DOMAIN: &[u8] = b"KAMI-INNER-CONTEXT-V1\0";
pub(crate) const MAX_INNER_BYTES: usize = 65_536;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum Direction {
    InitiatorToResponder = 1,
    ResponderToInitiator = 2,
}

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct InnerEnvelopeV1 {
    pub context: ApplicationContext,
    pub kind: MessageKind,
    pub sequence: Option<SenderSequence>,
    pub receipt_target: Option<MessageId>,
    pub created_at_ms: u64,
    pub session_id: String,
    pub direction: Direction,
    pub sender_peer: PeerId,
    pub recipient_peer: PeerId,
    /// SHA-256 of exact independently signed WP2-A1 proof bytes, both parties.
    pub sender_binding: [u8; 32],
    pub recipient_binding: [u8; 32],
    pub payload: Vec<u8>,
}
impl fmt::Debug for InnerEnvelopeV1 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("InnerEnvelopeV1([REDACTED])")
    }
}
impl Drop for InnerEnvelopeV1 {
    fn drop(&mut self) {
        self.payload.zeroize();
    }
}
impl InnerEnvelopeV1 {
    fn validate(&self) -> Result<(), Error> {
        self.context.validate()?;
        if self.context.sender == self.context.recipient
            || self.created_at_ms > 253_402_300_799_999
            || self.sender_peer.to_bytes().len() > 128
            || self.recipient_peer.to_bytes().len() > 128
            || self.session_id.len() != 43
            || !self
                .session_id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'+' || b == b'/')
            || !b"AEIMQUYcgkosw048".contains(&self.session_id.as_bytes()[42])
            || self.sender_binding == [0; 32]
            || self.recipient_binding == [0; 32]
            || self.payload.len() > MAX_INNER_BYTES
        {
            return Err(Error::InvalidContext);
        }
        match self.kind {
            MessageKind::Text
                if self.sequence.is_some_and(|s| s.value() > 0)
                    && self.receipt_target.is_none()
                    && !self.payload.is_empty()
                    && std::str::from_utf8(&self.payload).is_ok() =>
            {
                Ok(())
            }
            MessageKind::DeviceDelivered
                if self.sequence.is_none()
                    && self.receipt_target.is_some_and(|id| id.as_u128() != 0)
                    && (self.payload.is_empty()
                        || crate::authenticated::receipts::valid_payload(&self.payload)) =>
            {
                Ok(())
            }
            _ => Err(Error::InvalidContext),
        }
    }
    /// domain || envelope-version:u8 || Olm-profile:u16be || kind:u8 ||
    /// direction:u8 || conversation[16] || message[16] || devices[32]*2 ||
    /// session-id:canonical unpadded base64[43] || sequence:u64be (text) OR
    /// receipt-target[16] (receipt) || timestamp:u64be ||
    /// peers:u16be-length-prefixed*2 || proof-digests[32]*2 || payload:u32be+bytes.
    pub(crate) fn encode(&self) -> Result<Vec<u8>, Error> {
        self.validate()?;
        let mut out = Vec::new();
        out.extend(DOMAIN);
        out.extend([1, 0, 1]);
        out.push(match self.kind {
            MessageKind::Text => 1,
            MessageKind::DeviceDelivered => 2,
        });
        out.push(self.direction as u8);
        out.extend(self.context.conversation.as_u128().to_be_bytes());
        out.extend(self.context.message.as_u128().to_be_bytes());
        out.extend(self.context.sender.as_bytes());
        out.extend(self.context.recipient.as_bytes());
        out.extend(self.session_id.as_bytes());
        match self.kind {
            MessageKind::Text => out.extend(
                self.sequence
                    .ok_or(Error::InvalidContext)?
                    .value()
                    .to_be_bytes(),
            ),
            MessageKind::DeviceDelivered => out.extend(
                self.receipt_target
                    .ok_or(Error::InvalidContext)?
                    .as_u128()
                    .to_be_bytes(),
            ),
        }
        out.extend(self.created_at_ms.to_be_bytes());
        for peer in [self.sender_peer, self.recipient_peer] {
            let bytes = peer.to_bytes();
            out.extend((bytes.len() as u16).to_be_bytes());
            out.extend(bytes);
        }
        out.extend(self.sender_binding);
        out.extend(self.recipient_binding);
        out.extend((self.payload.len() as u32).to_be_bytes());
        out.extend(&self.payload);
        if out.len() > MAX_INNER_BYTES {
            out.zeroize();
            return Err(Error::InvalidContext);
        }
        Ok(out)
    }
    pub(crate) fn decode(bytes: &[u8]) -> Result<Self, Error> {
        if bytes.len() > MAX_INNER_BYTES {
            return Err(Error::InvalidContext);
        }
        let mut r = Reader(bytes);
        if r.take(DOMAIN.len())? != DOMAIN || r.take(3)? != [1, 0, 1] {
            return Err(Error::InvalidContext);
        }
        let kind = match r.fixed::<1>()? {
            [1] => MessageKind::Text,
            [2] => MessageKind::DeviceDelivered,
            _ => return Err(Error::InvalidContext),
        };
        let direction = match r.fixed::<1>()? {
            [1] => Direction::InitiatorToResponder,
            [2] => Direction::ResponderToInitiator,
            _ => return Err(Error::InvalidContext),
        };
        let context = ApplicationContext {
            conversation: ConversationId::from_u128(u128::from_be_bytes(r.fixed()?)),
            message: MessageId::from_u128(u128::from_be_bytes(r.fixed()?)),
            sender: DeviceId::from_bytes(r.fixed()?),
            recipient: DeviceId::from_bytes(r.fixed()?),
        };
        let session_id = std::str::from_utf8(r.take(43)?)
            .map_err(|_| Error::InvalidContext)?
            .to_owned();
        let (sequence, receipt_target) = match kind {
            MessageKind::Text => (
                Some(SenderSequence::new(u64::from_be_bytes(r.fixed()?))),
                None,
            ),
            MessageKind::DeviceDelivered => (
                None,
                Some(MessageId::from_u128(u128::from_be_bytes(r.fixed()?))),
            ),
        };
        let created_at_ms = u64::from_be_bytes(r.fixed()?);
        let sender_peer = r.peer()?;
        let recipient_peer = r.peer()?;
        let sender_binding = r.fixed()?;
        let recipient_binding = r.fixed()?;
        let len = u32::from_be_bytes(r.fixed()?) as usize;
        let payload = r.take(len)?;
        if !r.0.is_empty() {
            return Err(Error::InvalidContext);
        }
        let payload = payload.to_vec();
        let inner = Self {
            context,
            kind,
            sequence,
            receipt_target,
            created_at_ms,
            session_id,
            direction,
            sender_peer,
            recipient_peer,
            sender_binding,
            recipient_binding,
            payload,
        };
        inner.validate()?;
        if Zeroizing::new(inner.encode()?).as_slice() != bytes {
            return Err(Error::InvalidContext);
        }
        Ok(inner)
    }
    pub(crate) fn commitment(&self) -> Result<[u8; 32], Error> {
        let mut hash = Sha256::new();
        hash.update(COMMITMENT_DOMAIN);
        hash.update(&*Zeroizing::new(self.encode()?));
        Ok(hash.finalize().into())
    }
    /// Uses the existing protocol encoder for the ACTUAL complete-frame budget.
    pub(crate) fn frame(&self, ciphertext: &[u8]) -> Result<StructurallyValidFrame, Error> {
        StructurallyValidFrame::new(FrameInput {
            message_id: self.context.message,
            conversation_id: self.context.conversation,
            sender: self.context.sender,
            recipient: self.context.recipient,
            kind: self.kind,
            sequence: self.sequence,
            receipt_target: self.receipt_target,
            created_at_ms: self.created_at_ms,
            protected_payload: ciphertext,
            authentication_context: &self.commitment()?,
        })
        .map_err(|_| Error::InvalidContext)
    }
    pub(crate) fn matches_frame(&self, frame: &StructurallyValidFrame) -> Result<(), Error> {
        if frame.message_id() != self.context.message
            || frame.conversation_id() != self.context.conversation
            || frame.sender() != self.context.sender
            || frame.recipient() != self.context.recipient
            || frame.kind() != self.kind
            || frame.sequence() != self.sequence
            || frame.receipt_target() != self.receipt_target
            || frame.created_at_ms() != self.created_at_ms
            || frame.authentication_context() != self.commitment()?
        {
            return Err(Error::InvalidContext);
        }
        Ok(())
    }
}
struct Reader<'a>(&'a [u8]);
impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], Error> {
        let value = self.0.get(..n).ok_or(Error::InvalidContext)?;
        self.0 = &self.0[n..];
        Ok(value)
    }
    fn fixed<const N: usize>(&mut self) -> Result<[u8; N], Error> {
        self.take(N)?.try_into().map_err(|_| Error::InvalidContext)
    }
    fn peer(&mut self) -> Result<PeerId, Error> {
        let n = u16::from_be_bytes(self.fixed()?) as usize;
        if n > 128 {
            return Err(Error::InvalidContext);
        }
        let bytes = self.take(n)?;
        let peer = PeerId::from_bytes(bytes).map_err(|_| Error::InvalidContext)?;
        if peer.to_bytes() != bytes {
            return Err(Error::InvalidContext);
        }
        Ok(peer)
    }
}
