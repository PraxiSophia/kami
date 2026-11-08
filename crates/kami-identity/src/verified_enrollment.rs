#[cfg(test)]
mod tests;
use crate::{store::read_key, KeyPurpose, KeySlot, NativeCustody, SecretStore};
use kami_key_provider::{KeyError, ProfileKeyId};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;

/// One-use native provisioning state. Runtime must hold the profile lease and
/// establish absence of DB/marker/sidecars before calling begin_fresh_native.
/// ```compile_fail
/// let _ = kami_identity::FreshEnrollment { profile: todo!(), row: vec![] };
/// ```
/// ```compile_fail
/// fn duplicate(f: kami_identity::FreshEnrollment) { let _ = f.clone(); }
/// ```
pub struct FreshEnrollment {
    profile: ProfileKeyId,
    row: Vec<u8>,
}
impl FreshEnrollment {
    pub fn canonical_row(&self) -> &[u8] {
        &self.row
    }
}
/// Anchor-checked public evidence. This is not proof of lease possession.
/// ```compile_fail
/// let _ = kami_identity::VerifiedEnrollment { profile: todo!(), row: vec![] };
/// ```
/// ```compile_fail
/// fn duplicate(p: kami_identity::VerifiedEnrollment) { let _ = p.clone(); }
/// ```
/// ```compile_fail
/// fn expose(p: kami_identity::VerifiedEnrollment) { println!("{p:?}"); }
/// ```
pub struct VerifiedEnrollment {
    pub(crate) profile: ProfileKeyId,
    pub(crate) row: Vec<u8>,
}

fn slot(profile: &ProfileKeyId, purpose: KeyPurpose) -> KeySlot {
    KeySlot::new(profile.clone(), purpose)
}
fn digest(row: &[u8]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(b"kami-enrollment-completion-v1\0");
    h.update((row.len() as u64).to_be_bytes());
    h.update(row);
    h.finalize().into()
}
fn require_absent(store: &impl SecretStore, slot: &KeySlot) -> Result<(), KeyError> {
    match store.read(slot) {
        Err(KeyError::Missing) => Ok(()),
        Ok(_) => Err(KeyError::Corrupt),
        Err(e) => Err(e),
    }
}
fn create_checked(
    store: &impl SecretStore,
    slot: &KeySlot,
    value: &[u8; 32],
) -> Result<(), KeyError> {
    store.create(slot, value)?;
    if bool::from(read_key(store, slot)?.as_slice().ct_eq(value)) {
        Ok(())
    } else {
        Err(KeyError::Corrupt)
    }
}
/// Explicit NEW namespace only. No journal, adoption, repair, retry or overwrite.
/// An interrupted enrollment burns this namespace; ordinary load never generates.
pub fn begin_fresh_native(
    custody: &NativeCustody,
    profile: &ProfileKeyId,
) -> Result<FreshEnrollment, KeyError> {
    let store = &custody.custody.0;
    for purpose in [
        KeyPurpose::Identity,
        KeyPurpose::Database,
        KeyPurpose::Anchor,
    ] {
        require_absent(store, &slot(profile, purpose))?;
    }
    let mut seed = zeroize::Zeroizing::new([0u8; 32]);
    let mut database = zeroize::Zeroizing::new([0u8; 32]);
    getrandom::fill(seed.as_mut()).map_err(|_| KeyError::Unavailable)?;
    getrandom::fill(database.as_mut()).map_err(|_| KeyError::Unavailable)?;
    let public = ed25519_dalek::SigningKey::from_bytes(&seed)
        .verifying_key()
        .to_bytes();
    let mut row = vec![1, 1];
    row.extend_from_slice(profile.as_str().as_bytes());
    row.extend_from_slice(&public);
    validate_enrollment_snapshot(profile, &row)?;
    create_checked(store, &slot(profile, KeyPurpose::Identity), &seed)?;
    create_checked(store, &slot(profile, KeyPurpose::Database), &database)?;
    Ok(FreshEnrollment {
        profile: profile.clone(),
        row,
    })
}
/// Runtime calls only after durable SQLCipher commit and exact worker readback.
/// This API cannot establish that external orchestration invariant by itself.
pub fn complete_fresh_enrollment(
    custody: &NativeCustody,
    fresh: FreshEnrollment,
    committed_row: &[u8],
) -> Result<(), KeyError> {
    if committed_row != fresh.row {
        return Err(KeyError::Corrupt);
    }
    let store = &custody.custody.0;
    check_keys(store, &fresh.profile, committed_row)?;
    let anchor = slot(&fresh.profile, KeyPurpose::Anchor);
    require_absent(store, &anchor)?;
    create_checked(store, &anchor, &digest(committed_row))
}
fn check_keys(
    store: &impl SecretStore,
    profile: &ProfileKeyId,
    row: &[u8],
) -> Result<(), KeyError> {
    validate_enrollment_snapshot(profile, row)?;
    let seed = read_key(store, &slot(profile, KeyPurpose::Identity))?;
    let public = ed25519_dalek::SigningKey::from_bytes(&seed)
        .verifying_key()
        .to_bytes();
    if public.as_slice() != &row[34..] {
        return Err(KeyError::Corrupt);
    }
    read_key(store, &slot(profile, KeyPurpose::Database))?;
    Ok(())
}
/// Concrete native-only verifier; public bytes and generic test stores cannot
/// mint this proof. Static anchors do NOT detect coordinated DB+anchor rollback.
pub fn verify_enrollment_for_transport(
    custody: &NativeCustody,
    profile: &ProfileKeyId,
    row: &[u8],
) -> Result<VerifiedEnrollment, KeyError> {
    verify_with_store(&custody.custody.0, profile, row)?;
    Ok(VerifiedEnrollment {
        profile: profile.clone(),
        row: row.to_vec(),
    })
}
fn verify_with_store(
    store: &impl SecretStore,
    profile: &ProfileKeyId,
    row: &[u8],
) -> Result<(), KeyError> {
    validate_enrollment_snapshot(profile, row)?;
    let anchor = read_key(store, &slot(profile, KeyPurpose::Anchor))?;
    if !bool::from(anchor.as_slice().ct_eq(&digest(row))) {
        return Err(KeyError::Corrupt);
    }
    check_keys(store, profile, row)
}

/// Canonical public evidence, never enrollment authority. v1 Complete uses the
/// device Ed25519 key itself as libp2p identity; all three key slots are derived
/// from the exact fixed-width profile namespace, with no caller slot overrides.
pub fn validate_enrollment_snapshot(profile: &ProfileKeyId, row: &[u8]) -> Result<(), KeyError> {
    if row.len() != 66 || row[..2] != [1, 1] || &row[2..34] != profile.as_str().as_bytes() {
        return Err(KeyError::Corrupt);
    }
    let device: [u8; 32] = row[34..].try_into().map_err(|_| KeyError::Corrupt)?;
    crate::fingerprint::checked_key(&device).map_err(|_| KeyError::Corrupt)?;
    let public = libp2p_identity::ed25519::PublicKey::try_from_bytes(&device)
        .map_err(|_| KeyError::Corrupt)?;
    if public.to_bytes() != device {
        return Err(KeyError::Corrupt);
    }
    Ok(())
}
