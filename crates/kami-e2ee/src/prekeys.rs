//! Opaque Olm account lifecycle staging. No profile restore or raw-key accessor.
//! SQLCipher commit is required before installing a proposal or exposing a proof.
use crate::{persistence::AccountSnapshot, Error};
use kami_identity::{OlmBindingData, OlmBindingProof, OlmPrekeyKind};
use libp2p_identity::PeerId;
use sha2::{Digest, Sha256};
use vodozemac::olm::Account;
use zeroize::Zeroizing;

/// Stage-owned exact selected-prekey evidence; storage must match the durable
/// publication/retention record in the SAME session transaction.
#[derive(Clone)]
pub struct PrekeyUse {
    pub(crate) request: [u8; 32],
    pub(crate) response_digest: [u8; 32],
    pub(crate) reference: Vec<u8>,
    pub(crate) kind: OlmPrekeyKind,
    pub(crate) bundle_generation: u64,
    pub(crate) local: bool,
}
impl PrekeyUse {
    pub fn request(&self) -> [u8; 32] {
        self.request
    }
    pub fn response_digest(&self) -> [u8; 32] {
        self.response_digest
    }
    pub fn reference(&self) -> &[u8] {
        &self.reference
    }
    pub fn kind(&self) -> OlmPrekeyKind {
        self.kind
    }
    pub fn bundle_generation(&self) -> u64 {
        self.bundle_generation
    }
    pub fn local(&self) -> bool {
        self.local
    }
    pub(crate) fn selected(
        selection: &kami_protocol::prekey_exchange::Selection,
        endpoint: &crate::Endpoint,
        local: bool,
    ) -> Result<Self, Error> {
        if selection.proof != endpoint.binding.to_bytes()
            || selection.publisher != *endpoint.device.as_bytes()
            || selection.bundle_generation != endpoint.selected.bundle_generation
        {
            return Err(Error::AuthenticationRejected);
        }
        let bytes = kami_protocol::prekey_exchange::encode(
            &kami_protocol::prekey_exchange::Exchange::Selection(selection.clone()),
        )
        .map_err(|_| Error::AuthenticationRejected)?;
        Ok(Self {
            request: selection.request,
            response_digest: response_digest(&bytes)?,
            reference: endpoint.selected.prekey_reference.clone(),
            kind: endpoint.selected.prekey_kind,
            bundle_generation: selection.bundle_generation,
            local,
        })
    }
}

pub const MAX_INVENTORY: usize = 50;
pub const INITIAL_INVENTORY: usize = 32;
pub const MAX_VALIDITY_MS: u64 = 300_000;

pub(crate) fn account_context(
    device: [u8; 32],
    peer: &PeerId,
    ed: [u8; 32],
    curve: [u8; 32],
) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(b"kami-prekey-account-custody-v1\0");
    h.update(device);
    h.update(peer.to_bytes());
    h.update(ed);
    h.update(curve);
    h.finalize().into()
}
pub fn response_digest(bytes: &[u8]) -> Result<[u8; 32], Error> {
    kami_protocol::prekey_exchange::decode(bytes).map_err(|_| Error::AuthenticationRejected)?;
    Ok(Sha256::digest(bytes).into())
}
pub fn challenge() -> Result<[u8; 32], Error> {
    kami_identity::OlmBindingData::new_challenge().map_err(|_| Error::AuthenticationRejected)
}
/// Private provider state remains owned here. Public inventory contains only
/// references/keys; consumption and publication state are durable storage CAS.
pub struct AccountLifecycle {
    pub(crate) account: Account,
    device: [u8; 32],
    peer: PeerId,
    generation: u64,
    bundle_generation: u64,
    binding_generation: u64,
    inventory: Vec<OlmBindingData>,
}
/// Nonconstructible account/inventory proposal, consumed by the SQLCipher worker.
pub struct LifecycleCommit {
    previous: Option<(u64, [u8; 32], Zeroizing<Vec<u8>>)>,
    generation: u64,
    digest: [u8; 32],
    snapshot: Zeroizing<Vec<u8>>,
    proofs: Vec<OlmBindingProof>,
}
/// Opaque custody transfer, never a network frame or serialized public API.
impl LifecycleCommit {
    pub fn generation(&self) -> u64 {
        self.generation
    }
    pub fn digest(&self) -> [u8; 32] {
        self.digest
    }
    pub fn proofs(&self) -> &[OlmBindingProof] {
        &self.proofs
    }
    pub fn consume<T>(
        self,
        custodian: impl FnOnce(
            Option<(u64, [u8; 32], Zeroizing<Vec<u8>>)>,
            u64,
            [u8; 32],
            Zeroizing<Vec<u8>>,
            Vec<OlmBindingProof>,
        ) -> T,
    ) -> T {
        custodian(
            self.previous,
            self.generation,
            self.digest,
            self.snapshot,
            self.proofs,
        )
    }
}
fn next(n: u64) -> Result<u64, Error> {
    n.checked_add(1)
        .filter(|n| *n <= i64::MAX as u64)
        .ok_or(Error::StateEncoding)
}
fn validity(now: u64, ttl: u64) -> Result<u64, Error> {
    if ttl == 0 || ttl > MAX_VALIDITY_MS {
        return Err(Error::InvalidContext);
    }
    now.checked_add(ttl)
        .filter(|n| *n <= i64::MAX as u64)
        .ok_or(Error::InvalidContext)
}
impl AccountLifecycle {
    /// New profile account only. Runtime must establish durable absence first;
    /// this is never a missing-key repair or database-copy restoration operation.
    pub fn create(
        device: [u8; 32],
        peer: PeerId,
        binding_generation: u64,
        now: u64,
        ttl: u64,
    ) -> Result<Self, Error> {
        if device == [0; 32] || binding_generation == 0 || binding_generation > i64::MAX as u64 {
            return Err(Error::InvalidContext);
        }
        validity(now, ttl)?;
        let mut owner = Self {
            account: Account::new(),
            device,
            peer,
            generation: 1,
            bundle_generation: 1,
            binding_generation,
            inventory: Vec::new(),
        };
        owner.generate(INITIAL_INVENTORY, now, ttl)?;
        owner.account.generate_fallback_key();
        owner.record_fallback(now, ttl)?;
        Ok(owner)
    }
    pub fn generation(&self) -> u64 {
        self.generation
    }
    pub fn snapshot_digest(&self) -> Result<[u8; 32], Error> {
        Ok(self.snapshot()?.digest())
    }
    pub fn select(
        &self,
        available: &[OlmBindingProof],
        now: u64,
        minimum: u64,
    ) -> Result<OlmBindingProof, Error> {
        if available.len() > MAX_INVENTORY + 2 {
            return Err(Error::StateTooLarge);
        }
        for kind in [OlmPrekeyKind::OneTime, OlmPrekeyKind::Fallback] {
            for proof in available {
                let p = proof.data();
                if p.prekey_kind == kind
                    && p.bundle_generation >= minimum
                    && p.not_before_ms <= now
                    && now < p.expires_at_ms
                    && proof.device_key() == &self.device
                    && self.inventory.contains(p)
                {
                    return Ok(proof.clone());
                }
            }
        }
        Err(Error::AuthenticationRejected)
    }
    pub fn one_time_count(&self) -> usize {
        self.account.stored_one_time_key_count()
    }
    pub fn inventory(&self) -> &[OlmBindingData] {
        &self.inventory
    }
    pub(crate) fn consumed(
        &self,
        account: &Account,
        generation: u64,
        selected: &OlmBindingData,
    ) -> Result<Self, Error> {
        if !self.inventory.contains(selected)
            || account.identity_keys() != self.account.identity_keys()
            || generation
                != if selected.prekey_kind == OlmPrekeyKind::OneTime {
                    next(self.generation)?
                } else {
                    self.generation
                }
        {
            return Err(Error::AuthenticationRejected);
        }
        let mut proposal = self.copy();
        proposal.account = Account::from_pickle(account.pickle());
        proposal.generation = generation;
        if selected.prekey_kind == OlmPrekeyKind::OneTime {
            proposal
                .inventory
                .retain(|p| p.prekey_reference != selected.prekey_reference);
        }
        Ok(proposal)
    }
    fn copy(&self) -> Self {
        Self {
            account: Account::from_pickle(self.account.pickle()),
            device: self.device,
            peer: self.peer,
            generation: self.generation,
            bundle_generation: self.bundle_generation,
            binding_generation: self.binding_generation,
            inventory: self.inventory.clone(),
        }
    }
    fn data(
        &self,
        kind: OlmPrekeyKind,
        reference: Vec<u8>,
        key: [u8; 32],
        now: u64,
        ttl: u64,
    ) -> Result<OlmBindingData, Error> {
        Ok(OlmBindingData {
            publisher_peer: self.peer,
            olm_ed25519: *self.account.ed25519_key().as_bytes(),
            olm_curve25519: *self.account.curve25519_key().as_bytes(),
            prekey_kind: kind,
            prekey_reference: reference,
            prekey_curve25519: key,
            binding_generation: self.binding_generation,
            bundle_generation: self.bundle_generation,
            not_before_ms: now,
            expires_at_ms: validity(now, ttl)?,
        })
    }
    fn generate(&mut self, count: usize, now: u64, ttl: u64) -> Result<(), Error> {
        validity(now, ttl)?;
        if count == 0
            || count > MAX_INVENTORY
            || self
                .one_time_count()
                .checked_add(count)
                .is_none_or(|n| n > MAX_INVENTORY)
        {
            return Err(Error::StateTooLarge);
        }
        let generated = self.account.generate_one_time_keys(count);
        if !generated.removed.is_empty() || generated.created.len() != count {
            return Err(Error::StateEncoding);
        }
        for (id, key) in self.account.one_time_keys() {
            if !generated.created.contains(&key) {
                continue;
            }
            let mut reference = vec![OlmPrekeyKind::OneTime as u8];
            reference.extend(id.to_base64().as_bytes());
            self.inventory.push(self.data(
                OlmPrekeyKind::OneTime,
                reference,
                *key.as_bytes(),
                now,
                ttl,
            )?);
        }
        self.inventory
            .sort_by(|a, b| a.prekey_reference.cmp(&b.prekey_reference));
        Ok(())
    }
    fn record_fallback(&mut self, now: u64, ttl: u64) -> Result<(), Error> {
        for (id, key) in self.account.fallback_key() {
            let mut reference = vec![OlmPrekeyKind::Fallback as u8];
            reference.extend(id.to_base64().as_bytes());
            self.inventory.push(self.data(
                OlmPrekeyKind::Fallback,
                reference,
                *key.as_bytes(),
                now,
                ttl,
            )?);
        }
        Ok(())
    }
    pub fn replenish(&self, count: usize, now: u64, ttl: u64) -> Result<Self, Error> {
        let mut proposal = self.copy();
        proposal.generation = next(self.generation)?;
        proposal.bundle_generation = next(self.bundle_generation)?;
        // Outstanding signed selections keep their exact generation and validity.
        proposal.generate(count, now, ttl)?;
        Ok(proposal)
    }
    /// Never overwrite a still-valid previous fallback. Old proofs stay in the
    /// durable inventory as retirement evidence even after provider forgetting.
    pub fn rotate_fallback(&self, now: u64, ttl: u64) -> Result<Self, Error> {
        validity(now, ttl)?;
        let active: Vec<_> = self
            .inventory
            .iter()
            .filter(|p| p.prekey_kind == OlmPrekeyKind::Fallback)
            .collect();
        if active.len() >= 2 && active[active.len() - 2].expires_at_ms > now {
            return Err(Error::AuthenticationRejected);
        }
        let mut proposal = self.copy();
        proposal.generation = next(self.generation)?;
        proposal.bundle_generation = next(self.bundle_generation)?;
        // Retain at most current/previous provider slots; durable tombstones are
        // storage-owned and are never erased by this public inventory projection.
        let keep = active.last().map(|p| p.prekey_reference.clone());
        proposal.inventory.retain(|p| {
            p.prekey_kind != OlmPrekeyKind::Fallback || Some(&p.prekey_reference) == keep.as_ref()
        });
        proposal.account.generate_fallback_key();
        proposal.record_fallback(now, ttl)?;
        Ok(proposal)
    }
    fn snapshot(&self) -> Result<AccountSnapshot, Error> {
        AccountSnapshot::capture_lifecycle(
            &self.account,
            account_context(
                self.device,
                &self.peer,
                *self.account.ed25519_key().as_bytes(),
                *self.account.curve25519_key().as_bytes(),
            ),
            self.generation,
        )
    }
    pub fn prepare_commit(
        &self,
        previous: Option<&Self>,
        mut sign: impl FnMut(OlmBindingData) -> Result<OlmBindingProof, kami_identity::IdentityError>,
    ) -> Result<LifecycleCommit, Error> {
        if previous.is_none() && self.generation != 1 {
            return Err(Error::StateEncoding);
        }
        if let Some(old) = previous {
            if self.generation != next(old.generation)?
                || self.device != old.device
                || self.peer != old.peer
                || self.account.ed25519_key() != old.account.ed25519_key()
                || self.account.curve25519_key() != old.account.curve25519_key()
            {
                return Err(Error::StateEncoding);
            }
        }
        if self.inventory.len() > MAX_INVENTORY + 2 {
            return Err(Error::StateTooLarge);
        }
        let mut proofs = Vec::with_capacity(self.inventory.len());
        for data in &self.inventory {
            let proof = sign(data.clone()).map_err(|_| Error::AuthenticationRejected)?;
            if proof.device_key() != &self.device || proof.data() != data {
                return Err(Error::AuthenticationRejected);
            }
            proofs.push(proof);
        }
        let snapshot = self.snapshot()?;
        let digest = snapshot.digest();
        let previous = previous
            .map(|old| {
                old.snapshot()
                    .map(|s| (old.generation, s.digest(), s.into_bytes()))
            })
            .transpose()?;
        Ok(LifecycleCommit {
            previous,
            generation: self.generation,
            digest,
            snapshot: snapshot.into_bytes(),
            proofs,
        })
    }
}
