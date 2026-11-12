//! Quarantined authenticated-envelope and bounded WP2-B custodian boundary.
//!
//! No public messaging activation, runtime import, or prekey publication.
//! Candidates alone never grant ACK eligibility; the storage transaction does.
//! `ContactTrust`, `PeerId`, `ExpectedContext` and `now` are supplied assertions,
//! not independently proven fresh trust, a live END peer, or a trusted clock.
//! Checking their consistency does not establish runtime provenance. A relay
//! identity or indirectly obtained signed prekey is never live authentication.
//! Production needs producer-owned, nonforgeable KAMI-27 end-peer evidence and
//! serialized durable trust/connection freshness checks at finalization/retry.
//! No such evidence can be minted here, and no production constructor is added.
//!
//! Olm v1 only: 8-byte truncated MAC, upstream bounded skipped-key retention.
//! This is not Signal/Matrix interoperability or production qualification.
use crate::{
    envelope::{Direction, InnerEnvelopeV1},
    Error,
};
use kami_core::{ConversationId, DeviceId};
use kami_identity::{
    verify_olm_binding, ContactTrust, OlmBindingData, OlmBindingProof, OlmPrekeyKind,
    PeerBindingProof,
};
use kami_protocol::{
    decode_frame, encode_frame, validate_for_context, ExpectedContext, StructurallyValidFrame,
};
use libp2p_identity::PeerId;
use sha2::{Digest, Sha256};
use vodozemac::{
    olm::{Account, OlmMessage, Session, SessionConfig},
    Curve25519PublicKey,
};
use zeroize::Zeroizing;

pub(crate) mod receipts;
pub use receipts::{AuthenticatedDelivery, AuthenticatedReceiptCommit, ReplayPermit};

#[derive(Clone)]
pub struct Endpoint {
    pub(crate) device: DeviceId,
    pub(crate) peer_binding: PeerBindingProof,
    pub(crate) binding: OlmBindingProof,
    /// Retained trusted selection; never inferred merely from received proof.
    pub(crate) selected: OlmBindingData,
}
impl Endpoint {
    /// Untrusted inputs, verified at every stage/finalization. Not a trust tag.
    pub fn new(
        device: DeviceId,
        peer_binding: PeerBindingProof,
        binding: OlmBindingProof,
        selected: OlmBindingData,
    ) -> Self {
        Self {
            device,
            peer_binding,
            binding,
            selected,
        }
    }
    pub(crate) fn account_context(&self) -> [u8; 32] {
        crate::prekeys::account_context(
            *self.device.as_bytes(),
            &self.selected.publisher_peer,
            self.selected.olm_ed25519,
            self.selected.olm_curve25519,
        )
    }
    pub(crate) fn digest(&self) -> [u8; 32] {
        Sha256::digest(self.binding.to_bytes()).into()
    }
    fn verify(&self, now: u64) -> Result<(), Error> {
        self.binding
            .verify(
                *self.device.as_bytes(),
                &self.peer_binding,
                &self.selected,
                now,
            )
            .map_err(|_| Error::AuthenticationRejected)
    }
    pub(crate) fn matches_account(&self, account: &Account) -> Result<(), Error> {
        if account.ed25519_key().as_bytes() != &self.selected.olm_ed25519
            || account.curve25519_key().as_bytes() != &self.selected.olm_curve25519
        {
            return Err(Error::AuthenticationRejected);
        }
        Ok(())
    }
}
#[derive(Clone)]
pub(crate) struct Anchor {
    pub local: Endpoint,
    pub remote: Endpoint,
    pub conversation: ConversationId,
    /// Recipient-local snapshot generation, NEVER encoded as a shared counter.
    pub trust_generation: u64,
}
impl Anchor {
    fn verify(
        &self,
        trust: Option<&ContactTrust>,
        end_peer: Option<&PeerId>,
        now: u64,
    ) -> Result<(), Error> {
        if self.conversation.as_u128() == 0
            || self.local.device == self.remote.device
            || trust.is_none_or(|t| t.generation() != self.trust_generation)
        {
            return Err(Error::AuthenticationRejected);
        }
        self.local.verify(now)?;
        self.remote.verify(now)?;
        verify_olm_binding(
            trust,
            *self.remote.device.as_bytes(),
            &self.remote.binding,
            &self.remote.selected,
            end_peer,
            now,
        )
        .map_err(|_| Error::AuthenticationRejected)
    }
    fn matches_inner(
        &self,
        inner: &InnerEnvelopeV1,
        session: &Session,
        initiator: bool,
        sending: bool,
    ) -> Result<(), Error> {
        let (sender, recipient) = if sending {
            (&self.local, &self.remote)
        } else {
            (&self.remote, &self.local)
        };
        let direction = if initiator == sending {
            Direction::InitiatorToResponder
        } else {
            Direction::ResponderToInitiator
        };
        if session.session_config() != SessionConfig::version_1()
            || inner.context.conversation != self.conversation
            || inner.context.sender != sender.device
            || inner.context.recipient != recipient.device
            || inner.sender_peer != sender.selected.publisher_peer
            || inner.recipient_peer != recipient.selected.publisher_peer
            || inner.sender_binding != sender.digest()
            || inner.recipient_binding != recipient.digest()
            || inner.session_id != session.session_id()
            || inner.direction != direction
        {
            return Err(Error::AuthenticationRejected);
        }
        Ok(())
    }
}

/// Restricted application-bound session contract, separate from native AAD.
/// Only canonical `InnerEnvelopeV1` plaintext is encrypted. Decryption must pass
/// Olm authentication, canonical parsing, anchored inner checks and complete
/// outer correspondence before returning a stage. A stage never mutates its
/// source and must recheck supplied trust/peer/validity at finalization.
///
/// This crate-private interface is NOT a runtime-authority capability. Its
/// supplied arguments can be fabricated; do not expose it as live provenance.
pub(crate) struct BoundSession {
    session: Session,
    anchor: Anchor,
    initiator: bool,
    committed_generation: u64,
    committed_account_generation: u64,
}
/// Candidate only, no durable evidence or implicit commit. No Debug/serialization.
pub(crate) struct SendStage {
    pub session: BoundSession,
    pub frame: Vec<u8>,
    inner: InnerEnvelopeV1,
    source_digest: [u8; 32],
}
pub(crate) struct ReceiveStage {
    pub session: BoundSession,
    pub inner: InnerEnvelopeV1,
    frame: Vec<u8>,
    source_digest: [u8; 32],
}
pub(crate) struct InboundStage {
    pub account: Account,
    pub session: BoundSession,
    pub inner: InnerEnvelopeV1,
    frame: Vec<u8>,
    source_account_digest: [u8; 32],
    source_account_generation: u64,
}
impl BoundSession {
    fn persistence_context(anchor: &Anchor) -> [u8; 32] {
        let mut hash = Sha256::new();
        hash.update(b"kami-wp3-session-context-v1\0");
        hash.update(anchor.local.device.as_bytes());
        hash.update(anchor.remote.device.as_bytes());
        hash.update(anchor.conversation.as_u128().to_be_bytes());
        hash.update(anchor.trust_generation.to_be_bytes());
        hash.update(anchor.local.digest());
        hash.update(anchor.remote.digest());
        hash.finalize().into()
    }
    pub(crate) fn snapshot(
        &self,
        generation: u64,
        account_generation: u64,
    ) -> Result<crate::persistence::SessionSnapshot, Error> {
        crate::persistence::SessionSnapshot::capture(
            &self.session,
            generation,
            if generation == self.committed_generation {
                self.committed_account_generation
            } else {
                account_generation
            },
            Self::persistence_context(&self.anchor),
            self.initiator,
        )
    }
    // Private qualification only; no automatic DB/profile-copy restore path.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn restore(
        snapshot: &crate::persistence::SessionSnapshot,
        anchor: Anchor,
        account: &Account,
        generation: u64,
        account_generation: u64,
        session_id: &str,
        authority: crate::persistence::RestoreAuthority,
        trust: Option<&ContactTrust>,
        end_peer: Option<&PeerId>,
        now: u64,
    ) -> Result<Self, Error> {
        anchor.verify(trust, end_peer, now)?;
        anchor.local.matches_account(account)?;
        let (session, initiator) = snapshot.restore(
            generation,
            account_generation,
            Self::persistence_context(&anchor),
            authority,
        )?;
        if session.session_id() != session_id {
            return Err(Error::StateEncoding);
        }
        Ok(Self {
            session,
            anchor,
            initiator,
            committed_generation: generation,
            committed_account_generation: account_generation,
        })
    }
    pub(crate) fn outbound(
        account: &Account,
        anchor: Anchor,
        trust: Option<&ContactTrust>,
        end_peer: Option<&PeerId>,
        now: u64,
    ) -> Result<Self, Error> {
        anchor.verify(trust, end_peer, now)?;
        anchor.local.matches_account(account)?;
        let session = account
            .create_outbound_session(
                SessionConfig::version_1(),
                Curve25519PublicKey::from_bytes(anchor.remote.selected.olm_curve25519),
                Curve25519PublicKey::from_bytes(anchor.remote.selected.prekey_curve25519),
            )
            .map_err(|_| Error::AuthenticationRejected)?;
        Ok(Self {
            session,
            anchor,
            initiator: true,
            committed_generation: 0,
            committed_account_generation: 1,
        })
    }
    pub(crate) fn session_id(&self) -> String {
        self.session.session_id()
    }
    pub(crate) fn stage_send(
        &self,
        inner: &InnerEnvelopeV1,
        trust: Option<&ContactTrust>,
        end_peer: Option<&PeerId>,
        now: u64,
    ) -> Result<SendStage, Error> {
        self.anchor.verify(trust, end_peer, now)?;
        self.anchor
            .matches_inner(inner, &self.session, self.initiator, true)?;
        let plaintext = Zeroizing::new(inner.encode().map_err(|_| Error::AuthenticationRejected)?);
        let mut session = Session::from_pickle(self.session.pickle());
        let message = session
            .encrypt(&*plaintext)
            .map_err(|_| Error::AuthenticationRejected)?;
        let ciphertext = encode_olm(&message);
        let frame = inner
            .frame(&ciphertext)
            .and_then(|f| encode_frame(&f).map_err(|_| Error::AuthenticationRejected))
            .map_err(|_| Error::AuthenticationRejected)?;
        Ok(SendStage {
            session: Self {
                session,
                anchor: self.anchor.clone(),
                initiator: self.initiator,
                committed_generation: self.committed_generation,
                committed_account_generation: self.committed_account_generation,
            },
            frame,
            inner: inner.clone(),
            source_digest: self.snapshot(1, 1)?.digest(),
        })
    }
    pub(crate) fn stage_receive(
        &self,
        bytes: &[u8],
        expected: &ExpectedContext,
        trust: Option<&ContactTrust>,
        end_peer: Option<&PeerId>,
        now: u64,
    ) -> Result<ReceiveStage, Error> {
        self.anchor.verify(trust, end_peer, now)?;
        let (frame, message) = parse_frame(bytes, expected)?;
        // Session::decrypt ignores the prekey wrapper's routing keys itself.
        // Repeated PreKey MUST route to this exact anchored session, not account.
        if let OlmMessage::PreKey(prekey) = &message {
            if prekey.session_keys() != self.session.session_keys() {
                return Err(Error::AuthenticationRejected);
            }
        }
        let mut session = Session::from_pickle(self.session.pickle());
        let plaintext = Zeroizing::new(
            session
                .decrypt(&message)
                .map_err(|_| Error::AuthenticationRejected)?,
        );
        let inner = self.accept_inner(&plaintext, &frame, &session)?;
        Ok(ReceiveStage {
            session: Self {
                session,
                anchor: self.anchor.clone(),
                initiator: self.initiator,
                committed_generation: self.committed_generation,
                committed_account_generation: self.committed_account_generation,
            },
            inner,
            frame: bytes.to_vec(),
            source_digest: self.snapshot(1, 1)?.digest(),
        })
    }
    fn accept_inner(
        &self,
        plaintext: &[u8],
        frame: &StructurallyValidFrame,
        session: &Session,
    ) -> Result<InnerEnvelopeV1, Error> {
        let inner =
            InnerEnvelopeV1::decode(plaintext).map_err(|_| Error::AuthenticationRejected)?;
        self.anchor
            .matches_inner(&inner, session, self.initiator, false)?;
        inner
            .matches_frame(frame)
            .map_err(|_| Error::AuthenticationRejected)?;
        Ok(inner)
    }
    /// First-message account, session and prekey consumption are staged together.
    /// Invalid context drops ALL candidates. No mutation of the source account.
    pub(crate) fn stage_inbound(
        account: &Account,
        anchor: Anchor,
        bytes: &[u8],
        expected: &ExpectedContext,
        trust: Option<&ContactTrust>,
        end_peer: Option<&PeerId>,
        now: u64,
    ) -> Result<InboundStage, Error> {
        Self::stage_inbound_generation(account, anchor, bytes, expected, trust, end_peer, now, 1)
    }
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn stage_inbound_generation(
        account: &Account,
        anchor: Anchor,
        bytes: &[u8],
        expected: &ExpectedContext,
        trust: Option<&ContactTrust>,
        end_peer: Option<&PeerId>,
        now: u64,
        account_generation: u64,
    ) -> Result<InboundStage, Error> {
        anchor.verify(trust, end_peer, now)?;
        anchor.local.matches_account(account)?;
        let (frame, message) = parse_frame(bytes, expected)?;
        let OlmMessage::PreKey(prekey) = message else {
            return Err(Error::AuthenticationRejected);
        };
        if prekey.identity_key().as_bytes() != &anchor.remote.selected.olm_curve25519
            || prekey.one_time_key().as_bytes() != &anchor.local.selected.prekey_curve25519
        {
            return Err(Error::AuthenticationRejected);
        }
        let mut staged = Account::from_pickle(account.pickle());
        let result = staged
            .create_inbound_session(
                SessionConfig::version_1(),
                Curve25519PublicKey::from_bytes(anchor.remote.selected.olm_curve25519),
                &prekey,
            )
            .map_err(|_| Error::AuthenticationRejected)?;
        let plaintext = Zeroizing::new(result.plaintext);
        // Upstream removes only an OTK, never a fallback. Check selection kind
        // against the actual staged effect, including already-published keys.
        let removed = account
            .stored_one_time_key_count()
            .checked_sub(staged.stored_one_time_key_count());
        if removed
            != Some(match anchor.local.selected.prekey_kind {
                OlmPrekeyKind::OneTime => 1,
                OlmPrekeyKind::Fallback => 0,
            })
        {
            return Err(Error::AuthenticationRejected);
        }
        let source_account_digest = crate::persistence::AccountSnapshot::capture(
            account,
            &anchor.local,
            account_generation,
        )?
        .digest();
        let bound = Self {
            session: result.session,
            anchor,
            initiator: false,
            committed_generation: 0,
            committed_account_generation: account_generation,
        };
        let inner = bound.accept_inner(&plaintext, &frame, &bound.session)?;
        Ok(InboundStage {
            account: staged,
            session: bound,
            inner,
            frame: bytes.to_vec(),
            source_account_digest,
            source_account_generation: account_generation,
        })
    }
}
/// Consumed authenticated candidate. No raw constructor, cloning, Debug or serde.
/// A candidate is NOT durable evidence, a receipt, or live network provenance.
pub struct AuthenticatedCommit {
    material: CommitMaterial,
    lifecycle: Option<crate::prekeys::PrekeyUse>,
}
impl AuthenticatedCommit {
    pub fn prekey_use(&self) -> Option<&crate::prekeys::PrekeyUse> {
        self.lifecycle.as_ref()
    }
    pub fn facts(&self) -> kami_core::MessageFacts {
        self.material.facts
    }
    /// Explicit trusted in-process custodian transfer. Full provider pickles may
    /// contain private prekey material. Never send this to IPC, logs or adapters.
    pub fn into_material(self) -> CommitMaterial {
        self.material
    }
}
/// Opaque, bounded SQLCipher custodian transfer; not accepted by storage as proof.
/// Only consuming a valid AuthenticatedCommit produces this value.
pub struct CommitMaterial {
    account: crate::persistence::AccountSnapshot,
    next_account: Option<crate::persistence::AccountSnapshot>,
    previous: Option<crate::persistence::SessionSnapshot>,
    next: crate::persistence::SessionSnapshot,
    id: [u8; 32],
    contact: [u8; 32],
    trust_generation: u64,
    pin: Vec<u8>,
    context: [u8; 32],
    facts: kami_core::MessageFacts,
    frame: Vec<u8>,
    payload: Zeroizing<Vec<u8>>,
    incoming: bool,
    account_generation: u64,
    session_generation: u64,
    received_at_ms: i64,
}
/// Consuming full-snapshot transfer, including provider private material.
/// Bounds and zeroizing ownership do not protect against a malicious custodian.
pub struct SnapshotTransfer {
    account: Zeroizing<Vec<u8>>,
    next_account: Option<Zeroizing<Vec<u8>>>,
    previous: Option<Zeroizing<Vec<u8>>>,
    next: Zeroizing<Vec<u8>>,
}
impl SnapshotTransfer {
    /// Transfer full bounded provider snapshots to an explicitly trusted custodian.
    /// This is consuming export, not an individual-key getter or a storage proof.
    pub fn consume<T>(
        self,
        custodian: impl FnOnce(
            Zeroizing<Vec<u8>>,
            Option<Zeroizing<Vec<u8>>>,
            Option<Zeroizing<Vec<u8>>>,
            Zeroizing<Vec<u8>>,
        ) -> T,
    ) -> T {
        custodian(self.account, self.next_account, self.previous, self.next)
    }
}
impl CommitMaterial {
    // Only stage-owned evidence is retained; no public Clone/neutral reconstruction.
    fn retained(&self) -> Result<Self, Error> {
        use crate::persistence::{AccountSnapshot, SessionSnapshot};
        Ok(Self {
            account: AccountSnapshot::parse(self.account.bytes())?,
            next_account: self
                .next_account
                .as_ref()
                .map(|s| AccountSnapshot::parse(s.bytes()))
                .transpose()?,
            previous: self
                .previous
                .as_ref()
                .map(|s| SessionSnapshot::parse(s.bytes()))
                .transpose()?,
            next: SessionSnapshot::parse(self.next.bytes())?,
            id: self.id,
            contact: self.contact,
            trust_generation: self.trust_generation,
            pin: self.pin.clone(),
            context: self.context,
            facts: self.facts,
            frame: self.frame.clone(),
            payload: self.payload.clone(),
            incoming: self.incoming,
            account_generation: self.account_generation,
            session_generation: self.session_generation,
            received_at_ms: self.received_at_ms,
        })
    }
    pub fn session_id(&self) -> [u8; 32] {
        self.id
    }
    pub fn contact(&self) -> [u8; 32] {
        self.contact
    }
    pub fn trust_generation(&self) -> u64 {
        self.trust_generation
    }
    pub fn pin(&self) -> &[u8] {
        &self.pin
    }
    pub fn context(&self) -> [u8; 32] {
        self.context
    }
    pub fn facts(&self) -> kami_core::MessageFacts {
        self.facts
    }
    pub fn frame(&self) -> &[u8] {
        &self.frame
    }
    pub fn payload(&self) -> &[u8] {
        &self.payload
    }
    pub fn incoming(&self) -> bool {
        self.incoming
    }
    pub fn account_generation(&self) -> u64 {
        self.account_generation
    }
    pub fn session_generation(&self) -> u64 {
        self.session_generation
    }
    pub fn advances_account(&self) -> bool {
        self.next_account.is_some()
    }
    pub fn received_at_ms(&self) -> i64 {
        self.received_at_ms
    }
    pub fn account_digest(&self) -> [u8; 32] {
        self.account.digest()
    }
    pub fn next_account_digest(&self) -> Option<[u8; 32]> {
        self.next_account.as_ref().map(|s| s.digest())
    }
    pub fn previous_digest(&self) -> Option<[u8; 32]> {
        self.previous.as_ref().map(|s| s.digest())
    }
    pub fn next_digest(&self) -> [u8; 32] {
        self.next.digest()
    }
    pub fn into_snapshots(self) -> SnapshotTransfer {
        SnapshotTransfer {
            account: self.account.into_bytes(),
            next_account: self.next_account.map(|s| s.into_bytes()),
            previous: self.previous.map(|s| s.into_bytes()),
            next: self.next.into_bytes(),
        }
    }
}
fn next_generation(n: u64) -> Result<u64, Error> {
    n.checked_add(1)
        .filter(|v| *v <= i64::MAX as u64)
        .ok_or(Error::StateEncoding)
}
#[allow(clippy::too_many_arguments)]
fn finalize_stage(
    session: &BoundSession,
    inner: &InnerEnvelopeV1,
    frame: Vec<u8>,
    account: &Account,
    previous: Option<&BoundSession>,
    next_account: Option<&Account>,
    account_generation: u64,
    session_generation: u64,
    incoming: bool,
    trust: Option<&ContactTrust>,
    end_peer: Option<&PeerId>,
    now: u64,
) -> Result<AuthenticatedCommit, Error> {
    use crate::persistence::AccountSnapshot;
    session.anchor.verify(trust, end_peer, now)?;
    session.anchor.local.matches_account(account)?;
    session
        .anchor
        .matches_inner(inner, &session.session, session.initiator, !incoming)?;
    let outer = decode_frame(&frame).map_err(|_| Error::AuthenticationRejected)?;
    inner.matches_frame(&outer)?;
    // Receipts use the dedicated typed path, never ordinary text finalization.
    if inner.kind != kami_protocol::MessageKind::Text {
        return Err(Error::AuthenticationRejected);
    }
    let sequence = inner.sequence.ok_or(Error::AuthenticationRejected)?;
    if sequence.value() > i64::MAX as u64 {
        return Err(Error::StateEncoding);
    }
    let next_session_generation = next_generation(session_generation)?;
    let next_account_generation = if next_account.is_some() {
        next_generation(account_generation)?
    } else {
        account_generation
    };
    let account = AccountSnapshot::capture(account, &session.anchor.local, account_generation)?;
    let next_account = next_account
        .map(|a| AccountSnapshot::capture(a, &session.anchor.local, next_account_generation))
        .transpose()?;
    let previous = if session_generation == 0 {
        None
    } else {
        Some(
            previous
                .ok_or(Error::StateEncoding)?
                .snapshot(session_generation, account_generation)?,
        )
    };
    let next = session.snapshot(next_session_generation, next_account_generation)?;
    // Authenticated immutable inner content, NOT an unauthenticated outer digest.
    let facts = kami_core::MessageFacts::new(
        inner.context.message,
        inner.context.conversation,
        inner.context.sender,
        inner.context.recipient,
        sequence,
        kami_core::ContentBinding::from_bytes(inner.commitment()?),
    );
    let pin = trust
        .and_then(|t| t.pinned())
        .ok_or(Error::AuthenticationRejected)?
        .to_bytes();
    let payload = Zeroizing::new(if incoming {
        inner.payload.clone()
    } else {
        frame.clone()
    });
    Ok(AuthenticatedCommit {
        lifecycle: None,
        material: CommitMaterial {
            account,
            next_account,
            previous,
            next,
            id: Sha256::digest(session.session_id().as_bytes()).into(),
            contact: *session.anchor.remote.device.as_bytes(),
            trust_generation: session.anchor.trust_generation,
            pin,
            context: BoundSession::persistence_context(&session.anchor),
            facts,
            frame,
            payload,
            incoming,
            account_generation,
            session_generation: next_session_generation,
            received_at_ms: i64::try_from(now).map_err(|_| Error::StateEncoding)?,
        },
    })
}
impl SendStage {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn finalize(
        self,
        account: &Account,
        source: &BoundSession,
        account_generation: u64,
        session_generation: u64,
        trust: Option<&ContactTrust>,
        end_peer: Option<&PeerId>,
        now: u64,
    ) -> Result<AuthenticatedCommit, Error> {
        if source.snapshot(1, 1)?.digest() != self.source_digest {
            return Err(Error::StateEncoding);
        }
        finalize_stage(
            &self.session,
            &self.inner,
            self.frame,
            account,
            Some(source),
            None,
            account_generation,
            session_generation,
            false,
            trust,
            end_peer,
            now,
        )
    }
}
impl ReceiveStage {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn finalize(
        self,
        account: &Account,
        source: &BoundSession,
        account_generation: u64,
        session_generation: u64,
        trust: Option<&ContactTrust>,
        end_peer: Option<&PeerId>,
        now: u64,
    ) -> Result<AuthenticatedCommit, Error> {
        if source.snapshot(1, 1)?.digest() != self.source_digest || session_generation == 0 {
            return Err(Error::StateEncoding);
        }
        finalize_stage(
            &self.session,
            &self.inner,
            self.frame,
            account,
            Some(source),
            None,
            account_generation,
            session_generation,
            true,
            trust,
            end_peer,
            now,
        )
    }
}
impl InboundStage {
    pub(crate) fn finalize(
        self,
        source: &Account,
        trust: Option<&ContactTrust>,
        end_peer: Option<&PeerId>,
        now: u64,
    ) -> Result<AuthenticatedCommit, Error> {
        if crate::persistence::AccountSnapshot::capture(
            source,
            &self.session.anchor.local,
            self.source_account_generation,
        )?
        .digest()
            != self.source_account_digest
        {
            return Err(Error::StateEncoding);
        }
        // Fallback is reusable upstream, but the staged account still participates
        // in CAS. Only an actual one-time consumption advances the account row.
        let next = (self.session.anchor.local.selected.prekey_kind == OlmPrekeyKind::OneTime)
            .then_some(&self.account);
        finalize_stage(
            &self.session,
            &self.inner,
            self.frame,
            source,
            None,
            next,
            self.source_account_generation,
            0,
            true,
            trust,
            end_peer,
            now,
        )
    }
}

/// Bounded-development preparation only. Not a network sender or provider activation.
/// Supplied trust, selected bundles, end-peer and time require a trusted caller;
/// matching them is not proof of live KAMI-27 authentication or durable freshness.
/// This existing custodian/qualification API is not the production runtime seam.
/// Proposed state must not be installed until storage confirms the commit. No
/// automatic profile restore, individual-key export, or transmission operation.
/// Receipt candidates use dedicated opaque types and require a durable commit.
pub struct QuarantinedSession {
    account: Account,
    bound: BoundSession,
    account_generation: u64,
    session_generation: u64,
    retry: Option<CommitMaterial>,
    lifecycle: Option<crate::prekeys::PrekeyUse>,
}
impl QuarantinedSession {
    #[allow(clippy::too_many_arguments)]
    pub fn outbound(
        account: Account,
        local: Endpoint,
        remote: Endpoint,
        conversation: ConversationId,
        trust: &ContactTrust,
        end_peer: &PeerId,
        now: u64,
    ) -> Result<Self, Error> {
        let anchor = Anchor {
            local,
            remote,
            conversation,
            trust_generation: trust.generation(),
        };
        let bound = BoundSession::outbound(&account, anchor, Some(trust), Some(end_peer), now)?;
        Ok(Self {
            account,
            bound,
            account_generation: 1,
            session_generation: 0,
            retry: None,
            lifecycle: None,
        })
    }
    #[allow(clippy::too_many_arguments)]
    pub fn inbound(
        account: Account,
        local: Endpoint,
        remote: Endpoint,
        conversation: ConversationId,
        frame: &[u8],
        expected: &ExpectedContext,
        trust: &ContactTrust,
        end_peer: &PeerId,
        now: u64,
    ) -> Result<(AuthenticatedCommit, Self), Error> {
        let anchor = Anchor {
            local,
            remote,
            conversation,
            trust_generation: trust.generation(),
        };
        Self::inbound_generation(account, anchor, frame, expected, trust, end_peer, now, 1)
    }
    #[allow(clippy::too_many_arguments)]
    fn inbound_generation(
        account: Account,
        anchor: Anchor,
        frame: &[u8],
        expected: &ExpectedContext,
        trust: &ContactTrust,
        end_peer: &PeerId,
        now: u64,
        source_generation: u64,
    ) -> Result<(AuthenticatedCommit, Self), Error> {
        let stage = BoundSession::stage_inbound_generation(
            &account,
            anchor,
            frame,
            expected,
            Some(trust),
            Some(end_peer),
            now,
            source_generation,
        )?;
        let account_generation =
            if stage.session.anchor.local.selected.prekey_kind == OlmPrekeyKind::OneTime {
                next_generation(source_generation)?
            } else {
                source_generation
            };
        let mut proposed = Self {
            account: Account::from_pickle(stage.account.pickle()),
            bound: stage.session.copy(),
            account_generation,
            session_generation: 1,
            retry: None,
            lifecycle: None,
        };
        let mut commit = stage.finalize(&account, Some(trust), Some(end_peer), now)?;
        proposed.bound.committed_generation = 1;
        proposed.bound.committed_account_generation = account_generation;
        commit.lifecycle = proposed.lifecycle.clone();
        proposed.retry = Some(commit.material.retained()?);
        Ok((commit, proposed))
    }
    #[allow(clippy::too_many_arguments)]
    pub fn prepare_text(
        &self,
        message: kami_core::MessageId,
        sequence: kami_core::SenderSequence,
        created_at_ms: u64,
        text: &str,
        trust: &ContactTrust,
        end_peer: &PeerId,
        now: u64,
    ) -> Result<(AuthenticatedCommit, Self), Error> {
        if text.len() > crate::envelope::MAX_INNER_BYTES {
            return Err(Error::InvalidContext);
        }
        let a = &self.bound.anchor;
        let inner = InnerEnvelopeV1 {
            context: crate::ApplicationContext {
                conversation: a.conversation,
                message,
                sender: a.local.device,
                recipient: a.remote.device,
            },
            kind: kami_protocol::MessageKind::Text,
            sequence: Some(sequence),
            receipt_target: None,
            created_at_ms,
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
            payload: text.as_bytes().to_vec(),
        };
        let stage = self
            .bound
            .stage_send(&inner, Some(trust), Some(end_peer), now)?;
        let mut proposed = self.proposed(stage.session.copy())?;
        let mut commit = stage.finalize(
            &self.account,
            &self.bound,
            self.account_generation,
            self.session_generation,
            Some(trust),
            Some(end_peer),
            now,
        )?;
        commit.lifecycle = proposed.lifecycle.clone();
        proposed.retry = Some(commit.material.retained()?);
        Ok((commit, proposed))
    }
    pub fn prepare_receive(
        &self,
        frame: &[u8],
        expected: &ExpectedContext,
        trust: &ContactTrust,
        end_peer: &PeerId,
        now: u64,
    ) -> Result<(AuthenticatedCommit, Self), Error> {
        let stage = self
            .bound
            .stage_receive(frame, expected, Some(trust), Some(end_peer), now)?;
        let mut proposed = self.proposed(stage.session.copy())?;
        let mut commit = stage.finalize(
            &self.account,
            &self.bound,
            self.account_generation,
            self.session_generation,
            Some(trust),
            Some(end_peer),
            now,
        )?;
        commit.lifecycle = proposed.lifecycle.clone();
        proposed.retry = Some(commit.material.retained()?);
        Ok((commit, proposed))
    }
    /// Revalidate retained stage-owned evidence without encryption or ratcheting.
    /// Retain this proposal across an ambiguous worker reply; do not install its
    /// advanced session until commit/reconciliation confirms durability.
    pub fn prepare_retry(
        &self,
        trust: &ContactTrust,
        end_peer: &PeerId,
        now: u64,
    ) -> Result<AuthenticatedCommit, Error> {
        self.bound.anchor.verify(Some(trust), Some(end_peer), now)?;
        let material = self.retry.as_ref().ok_or(Error::StateEncoding)?;
        if material.context != BoundSession::persistence_context(&self.bound.anchor) {
            return Err(Error::StateEncoding);
        }
        Ok(AuthenticatedCommit {
            material: material.retained()?,
            lifecycle: self.lifecycle.clone(),
        })
    }
    fn proposed(&self, mut bound: BoundSession) -> Result<Self, Error> {
        bound.committed_generation = next_generation(self.session_generation)?;
        bound.committed_account_generation = self.account_generation;
        Ok(Self {
            account: Account::from_pickle(self.account.pickle()),
            bound,
            account_generation: self.account_generation,
            session_generation: next_generation(self.session_generation)?,
            retry: None,
            lifecycle: self.lifecycle.clone(),
        })
    }
}
impl BoundSession {
    fn copy(&self) -> Self {
        Self {
            session: Session::from_pickle(self.session.pickle()),
            anchor: self.anchor.clone(),
            initiator: self.initiator,
            committed_generation: self.committed_generation,
            committed_account_generation: self.committed_account_generation,
        }
    }
}
impl crate::prekeys::AccountLifecycle {
    /// Gated session composition only. Runtime must supply live retained selection
    /// authority and commit the returned candidate before installing any state.
    #[allow(clippy::too_many_arguments)]
    pub fn prepare_outbound(
        &self,
        local: Endpoint,
        remote: Endpoint,
        selected: &kami_protocol::prekey_exchange::Selection,
        conversation: ConversationId,
        trust: &ContactTrust,
        end_peer: &PeerId,
        now: u64,
    ) -> Result<QuarantinedSession, Error> {
        local.matches_account(&self.account)?;
        if selected.recipient != *local.device.as_bytes()
            || !self.inventory().contains(&local.selected)
        {
            return Err(Error::AuthenticationRejected);
        }
        let usage = crate::prekeys::PrekeyUse::selected(selected, &remote, false)?;
        let mut session = QuarantinedSession::outbound(
            Account::from_pickle(self.account.pickle()),
            local,
            remote,
            conversation,
            trust,
            end_peer,
            now,
        )?;
        session.account_generation = self.generation();
        session.lifecycle = Some(usage);
        Ok(session)
    }
    #[allow(clippy::too_many_arguments)]
    pub fn prepare_inbound(
        &self,
        local: Endpoint,
        remote: Endpoint,
        publication: &kami_protocol::prekey_exchange::Selection,
        conversation: ConversationId,
        frame: &[u8],
        expected: &ExpectedContext,
        trust: &ContactTrust,
        end_peer: &PeerId,
        now: u64,
    ) -> Result<(AuthenticatedCommit, QuarantinedSession, Self), Error> {
        local.matches_account(&self.account)?;
        if publication.recipient != *remote.device.as_bytes()
            || publication.account_generation > self.generation()
            || !self.inventory().contains(&local.selected)
        {
            return Err(Error::AuthenticationRejected);
        }
        let usage = crate::prekeys::PrekeyUse::selected(publication, &local, true)?;
        let selected = local.selected.clone();
        let anchor = Anchor {
            local,
            remote,
            conversation,
            trust_generation: trust.generation(),
        };
        let (mut commit, mut session) = QuarantinedSession::inbound_generation(
            Account::from_pickle(self.account.pickle()),
            anchor,
            frame,
            expected,
            trust,
            end_peer,
            now,
            self.generation(),
        )?;
        commit.lifecycle = Some(usage.clone());
        session.lifecycle = Some(usage);
        let owner = self.consumed(&session.account, session.account_generation, &selected)?;
        Ok((commit, session, owner))
    }
}
impl QuarantinedSession {
    /// Synchronize only with the opaque shared account owner after its commit.
    /// Storage's exact old-session CAS still uses its original account generation.
    pub fn synchronize_account(
        mut self,
        owner: &crate::prekeys::AccountLifecycle,
    ) -> Result<Self, Error> {
        self.bound.anchor.local.matches_account(&owner.account)?;
        if owner.generation() < self.account_generation {
            return Err(Error::StateEncoding);
        }
        self.account = Account::from_pickle(owner.account.pickle());
        self.account_generation = owner.generation();
        Ok(self)
    }
}
fn parse_frame(
    bytes: &[u8],
    expected: &ExpectedContext,
) -> Result<(StructurallyValidFrame, OlmMessage), Error> {
    // Existing protocol scanner enforces full encoded bytes <=65536 BEFORE decode.
    let frame = decode_frame(bytes).map_err(|_| Error::AuthenticationRejected)?;
    validate_for_context(&frame, expected).map_err(|_| Error::AuthenticationRejected)?;
    if frame.authentication_context().len() != 32 {
        return Err(Error::AuthenticationRejected);
    }
    let message = decode_olm(frame.protected_payload())?;
    Ok((frame, message))
}
/// One byte provider message type (0 prekey, 1 normal), then the provider's
/// canonical binary message. No JSON/base64 or custom cryptography. The outer
/// protocol version and inner profile are both fixed to v1.
fn encode_olm(message: &OlmMessage) -> Vec<u8> {
    let (kind, bytes) = message.to_parts();
    let mut out = Vec::with_capacity(1 + bytes.len());
    out.push(kind as u8);
    out.extend(bytes);
    out
}
fn decode_olm(bytes: &[u8]) -> Result<OlmMessage, Error> {
    if bytes.len() > kami_protocol::MAX_ENCODED_FRAME_SIZE {
        return Err(Error::AuthenticationRejected);
    }
    let (&kind, body) = bytes.split_first().ok_or(Error::AuthenticationRejected)?;
    let message = OlmMessage::from_parts(usize::from(kind), body)
        .map_err(|_| Error::AuthenticationRejected)?;
    // Provider parsers allow some protobuf variations; require exact canonical
    // re-encoding to reject unknown/duplicate/nonminimal/trailing fields.
    if encode_olm(&message) != bytes {
        return Err(Error::AuthenticationRejected);
    }
    Ok(message)
}
