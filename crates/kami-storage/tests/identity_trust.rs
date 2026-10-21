mod common;
#[path = "../../kami-identity/tests/common/vectors.rs"]
mod vectors;
use kami_identity::{
    send_decision, ContactTrust, ContactTrustRepository, ContactTrustState, IdentityError,
    Invitation, PeerBindingProof, SendDecision, TrustTransition, VerificationEvidence,
};
use kami_storage::{ContactTrustRow, Storage, StorageError, TrustState};

// Service-composition example: no production identity <-> storage dependency.
struct Repository(Storage);
fn error(e: StorageError) -> IdentityError {
    if e == StorageError::Conflict {
        IdentityError::Conflict
    } else {
        IdentityError::Persistence
    }
}
impl ContactTrustRepository for Repository {
    fn load(&self, contact: [u8; 32]) -> Result<Option<ContactTrust>, IdentityError> {
        self.0
            .load_contact(contact)
            .map_err(error)?
            .map(|row| {
                let state = match row.state() {
                    TrustState::Pending => ContactTrustState::Pending,
                    TrustState::Verified => ContactTrustState::Verified,
                    TrustState::Changed => ContactTrustState::Changed,
                    TrustState::Blocked => ContactTrustState::Blocked,
                };
                ContactTrust::from_persisted(state, row.generation(), row.pinned(), row.candidate())
            })
            .transpose()
    }
    fn compare_and_swap(
        &self,
        contact: [u8; 32],
        expected: Option<u64>,
        next: &ContactTrust,
    ) -> Result<(), IdentityError> {
        let state = match next.state() {
            ContactTrustState::Pending => TrustState::Pending,
            ContactTrustState::Verified => TrustState::Verified,
            ContactTrustState::Changed => TrustState::Changed,
            ContactTrustState::Blocked => TrustState::Blocked,
        };
        let row = ContactTrustRow::new(
            contact,
            state,
            next.generation(),
            next.pinned().map(Invitation::to_bytes),
            next.candidate().to_bytes(),
        )
        .map_err(error)?;
        self.0
            .compare_and_swap_contact(expected, row)
            .map_err(error)
    }
}
fn invitation(rotated: bool) -> Invitation {
    if rotated {
        return Invitation::parse_bounded(vectors::REPLACEMENT).unwrap();
    }
    let proof = PeerBindingProof::parse_bounded(vectors::BINDING).unwrap();
    Invitation::new(vectors::DEVICE, proof).unwrap()
}
#[test]
fn durable_identity_adapter_revalidates_restart_and_changed_send_holds() {
    let dir = common::directory();
    let mut repo = Repository(common::create(&dir));
    let id = [4; 32];
    let first = invitation(false);
    assert_ne!(
        PeerBindingProof::parse_bounded(vectors::ROTATED)
            .unwrap()
            .peer_id(),
        first.binding().peer_id()
    );
    let pending = ContactTrust::pending(first.clone());
    let queued = kami_core::MessageFacts::new(
        kami_core::MessageId::from_u128(1),
        kami_core::ConversationId::from_u128(2),
        kami_core::DeviceId::from_bytes([1; 32]),
        kami_core::DeviceId::from_bytes(vectors::DEVICE),
        kami_core::SenderSequence::new(1),
        kami_core::ContentBinding::from_bytes([3; 32]),
    );
    repo.0.enqueue(common::message(queued)).unwrap();
    repo.compare_and_swap(id, None, &pending).unwrap();
    repo.0.shutdown().unwrap();
    repo = Repository(common::open(&dir));
    let pending = repo.load(id).unwrap().unwrap();
    assert_eq!(pending.state(), ContactTrustState::Pending);
    let evidence =
        VerificationEvidence::independent_comparison(&pending, first.fingerprint(), true).unwrap();
    let verified = pending
        .transition(TrustTransition::Verify(evidence))
        .unwrap();
    repo.compare_and_swap(id, Some(pending.generation()), &verified)
        .unwrap();
    repo.0.shutdown().unwrap();
    repo = Repository(common::open(&dir));
    assert_eq!(repo.load(id).unwrap(), Some(verified.clone()));
    let changed = verified
        .transition(TrustTransition::Observe(invitation(true)))
        .unwrap();
    repo.compare_and_swap(id, Some(verified.generation()), &changed)
        .unwrap();
    repo.0.shutdown().unwrap();
    repo = Repository(common::open(&dir));
    let changed = repo.load(id).unwrap().unwrap();
    assert_eq!(changed.pinned(), Some(&first));
    assert_eq!(
        send_decision(
            Some(&changed),
            vectors::DEVICE,
            Some(first.binding()),
            Some(&first.binding().peer_id())
        ),
        SendDecision::Hold(IdentityError::Changed)
    );
    let blocked = changed.transition(TrustTransition::Block).unwrap();
    repo.compare_and_swap(id, Some(changed.generation()), &blocked)
        .unwrap();
    let evidence = VerificationEvidence::independent_comparison(
        &changed,
        changed.candidate().fingerprint(),
        true,
    )
    .unwrap();
    let stale_verified = changed
        .transition(TrustTransition::Verify(evidence))
        .unwrap();
    assert_eq!(
        repo.compare_and_swap(id, Some(changed.generation()), &stale_verified),
        Err(IdentityError::Conflict)
    );
    repo.0.shutdown().unwrap();
    repo = Repository(common::open(&dir));
    assert_eq!(repo.load(id).unwrap(), Some(blocked.clone()));
    let unblocked = blocked.transition(TrustTransition::Unblock).unwrap();
    repo.compare_and_swap(id, Some(blocked.generation()), &unblocked)
        .unwrap();
    let evidence = VerificationEvidence::independent_comparison(
        &unblocked,
        unblocked.candidate().fingerprint(),
        true,
    )
    .unwrap();
    let replacement = unblocked
        .transition(TrustTransition::Verify(evidence))
        .unwrap();
    repo.compare_and_swap(id, Some(unblocked.generation()), &replacement)
        .unwrap();
    repo.0.shutdown().unwrap();
    repo = Repository(common::open(&dir));
    let replacement = repo.load(id).unwrap().unwrap();
    assert_ne!(
        replacement.pinned().unwrap().device_key(),
        first.device_key()
    );
    assert_eq!(
        send_decision(
            Some(&replacement),
            vectors::DEVICE,
            Some(replacement.candidate().binding()),
            Some(&replacement.candidate().binding().peer_id())
        ),
        SendDecision::Hold(IdentityError::Changed)
    );
    assert_eq!(repo.0.pending(1).unwrap()[0].facts(), queued);
    assert_eq!(
        queued.recipient(),
        kami_core::DeviceId::from_bytes(vectors::DEVICE)
    );
    // Original queued material remains immutable; storage does not run a sender.
}

#[test]
fn cryptographically_malformed_stored_descriptor_fails_closed() {
    let dir = common::directory();
    let repo = Repository(common::create(&dir));
    let row = ContactTrustRow::new([8; 32], TrustState::Pending, 1, None, vec![1; 40]).unwrap();
    repo.0.compare_and_swap_contact(None, row).unwrap();
    assert_eq!(repo.load([8; 32]), Err(IdentityError::Malformed));
}
