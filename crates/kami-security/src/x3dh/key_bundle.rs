use x25519_dalek::{PublicKey, StaticSecret};
pub struct X25519KeyPair {
    pub(crate) private_key: StaticSecret,
    pub(crate) public_key: PublicKey,
}

pub struct IdentityKey {
    pub(crate) keypair: X25519KeyPair,
}

pub struct SignedPrekey {
    pub(crate) id: u32,
    pub(crate) keypair: X25519KeyPair,
    pub(crate) signature: [u8; 64],
    pub(crate) created_at: u64,
}

pub struct OneTimePrekey {
    pub(crate) id: u32,
    pub(crate) keypair: X25519KeyPair,
}

pub struct UserKeyBundle {
    pub(crate) identity_key: IdentityKey,
    pub(crate) active_signed_prekey: SignedPrekey,
    pub(crate) previous_signed_prekeys: Vec<SignedPrekey>,
    pub(crate) one_time_prekeys: Vec<OneTimePrekey>,
}

pub struct PublicKeyBundle {
    pub identity_public_key: [u8; 32],
    pub signed_prekey_id: u32,
    pub signed_prekey_public_key: [u8; 32],
    pub signed_prekey_signature: [u8; 64],
    pub one_time_prekey: Option<(u32, [u8; 32])>,
}