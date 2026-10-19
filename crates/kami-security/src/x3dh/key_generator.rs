use std::time::{SystemTime, UNIX_EPOCH};
use rand_core::OsRng;
use rand::rng;
use x25519_dalek::{PublicKey, StaticSecret};
use xeddsa::Sign;
use xeddsa::xed25519::PrivateKey as XEdDSAPrivateKey;

use super::key_bundle::{
    IdentityKey,
    OneTimePrekey,
    SignedPrekey,
    X25519KeyPair,
};

impl X25519KeyPair {
    pub fn public_key(&self) -> &PublicKey {
        &self.public_key
    }

    pub fn public_key_bytes(&self) -> [u8; 32] {
        *self.public_key.as_bytes()
    }

    pub(crate) fn private_key(&self) -> &StaticSecret {
        &self.private_key
    }
}

#[derive(Debug)]
pub enum KeyGeneratorError {
    InvalidPrekeyRange,
}

pub struct KeyGenerator;

impl KeyGenerator {
    fn generate_x25519() -> X25519KeyPair {
        let private_key = StaticSecret::random_from_rng(OsRng);

        let public_key = PublicKey::from(&private_key);

        X25519KeyPair {
            private_key,
            public_key,
        }
    }

    pub fn generate_identity_key() -> IdentityKey {
        IdentityKey {
            keypair: Self::generate_x25519(),
        }
    }

    pub fn generate_signed_prekey(identity: &IdentityKey, id: u32) -> SignedPrekey {
        let keypair = Self::generate_x25519();

        let public_key_bytes = keypair.public_key_bytes();

        let signing_key = XEdDSAPrivateKey(identity.keypair.private_key().to_bytes());

        let mut rng = rng();

        let signature = signing_key.sign(&public_key_bytes, &mut rng);

        let created_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        SignedPrekey {
            id,
            keypair,
            signature,
            created_at,
        }
    }

    pub fn generate_one_time_prekey(id: u32) -> OneTimePrekey {
        OneTimePrekey {
            id,
            keypair: Self::generate_x25519(),
        }
    }

    pub fn generate_one_time_prekeys(start_id: u32, count: usize) -> Result<Vec<OneTimePrekey>, KeyGeneratorError> {
        let count_u32 = u32::try_from(count).map_err(|_| KeyGeneratorError::InvalidPrekeyRange)?;

        if count_u32 > 0 {
            start_id.checked_add(count_u32 - 1).ok_or(KeyGeneratorError::InvalidPrekeyRange)?;
        }

        let mut prekeys = Vec::with_capacity(count);

        for offset in 0..count_u32 {
            let id = start_id + offset;

            prekeys.push(Self::generate_one_time_prekey(id));
        }

        Ok(prekeys)
    }

    pub fn generate_ephemeral_key() -> X25519KeyPair {
        Self::generate_x25519()
    }
}