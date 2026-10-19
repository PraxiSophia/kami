use std::time::{Duration, SystemTime};

use super::key_generator::KeyGenerator;
use super::key_bundle::{SignedPrekey, OneTimePrekey, UserKeyBundle};

const MAX_PREVIOUS_SIGNED_PREKEYS: usize = 2;
const MIN_ONE_TIME_PREKEYS: usize = 100;
const MAX_ONE_TIME_PREKEYS: usize = 10_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignedPrekeyRotationPolicy {
    SevenDays,
    FifteenDays,
    ThirtyDays,
}

impl SignedPrekeyRotationPolicy {
    pub fn duration(&self) -> Duration {
        let days = match self {
            Self::SevenDays => 7,
            Self::FifteenDays => 15,
            Self::ThirtyDays => 30,
        };

        Duration::from_secs(days * 24 * 60 * 60)
    }
}

#[derive(Debug)]
pub enum KeyManagerError {
    InvalidPrekeyRange,
    OneTimePrekeyLimitNotSufficient,
    SignedPrekeyIdExhausted,
    OneTimePrekeyIdExhausted,
    InvalidSystemTime,
    GeneratorError,
}

pub struct KeyManager {
    bundle: UserKeyBundle,
    rotation_policy: SignedPrekeyRotationPolicy,
    next_signed_prekey_id: Option<u32>,
    next_one_time_prekey_id: Option<u32>,
}

impl KeyManager {
    pub fn generate_initial_bundle(spk_rotation_policy: SignedPrekeyRotationPolicy, initial_opk_count: usize)
        -> Result<Self, KeyManagerError> {
        if initial_opk_count > MAX_ONE_TIME_PREKEYS || initial_opk_count < MIN_ONE_TIME_PREKEYS {
            return Err(KeyManagerError::OneTimePrekeyLimitNotSufficient);
        }

        let identity_key = KeyGenerator::generate_identity_key();

        let active_signed_prekey = KeyGenerator::generate_signed_prekey(&identity_key, 1);


        let one_time_prekeys = KeyGenerator::generate_one_time_prekeys(1, initial_opk_count)
            .map_err(|_| KeyManagerError::InvalidPrekeyRange)?;

        let bundle = UserKeyBundle {
            identity_key,
            active_signed_prekey,
            previous_signed_prekeys: Vec::new(),
            one_time_prekeys,
        };

        let next_signed_prekey_id = Some(2);
        let next_one_time_prekey_id = Some(1 + initial_opk_count as u32);

        Ok(Self {
            bundle,
            rotation_policy: spk_rotation_policy,
            next_signed_prekey_id,
            next_one_time_prekey_id,
        })
    }


    pub fn get_identity_public_key(&self) -> [u8; 32] {
        self.bundle.identity_key.keypair.public_key_bytes()
    }


    pub fn get_rotation_policy(&self) -> SignedPrekeyRotationPolicy {
        self.rotation_policy
    }


    pub fn set_rotation_policy(&mut self, policy: SignedPrekeyRotationPolicy) {
        self.rotation_policy = policy;
    }


    pub fn should_rotate_signed_prekey(&self, now: SystemTime) -> bool {
        let now_secs = match now.duration_since(SystemTime::UNIX_EPOCH) {
            Ok(duration) => duration.as_secs(),
            Err(_) => return false,
        };

        let created_at = self.bundle.active_signed_prekey.created_at;

        let elapsed = match now_secs.checked_sub(created_at) {
            Some(seconds) => seconds,
            None => return false,
        };

        elapsed >= self.rotation_policy.duration().as_secs()
    }


    pub fn rotate_signed_prekey(&mut self, now: SystemTime) -> Result<bool, KeyManagerError> {
        let now_secs = now
            .duration_since(SystemTime::UNIX_EPOCH)
            .map_err(|_| KeyManagerError::InvalidSystemTime)?
            .as_secs();

        if !self.should_rotate_signed_prekey(now) {
            return Ok(false);
        }

        let new_id = self.next_signed_prekey_id.ok_or(KeyManagerError::SignedPrekeyIdExhausted)?;

        let mut new_signed_prekey = KeyGenerator::generate_signed_prekey(
            &self.bundle.identity_key,
            new_id,
        );

        new_signed_prekey.created_at = now_secs;

        let old_active = std::mem::replace(&mut self.bundle.active_signed_prekey, new_signed_prekey);

        self.bundle.previous_signed_prekeys.insert(0, old_active);

        self.bundle.previous_signed_prekeys.truncate(MAX_PREVIOUS_SIGNED_PREKEYS);

        self.next_signed_prekey_id = new_id.checked_add(1);

        Ok(true)
    }


    pub fn find_signed_prekey(&self, id: u32) -> Option<&SignedPrekey> {
        if self.bundle.active_signed_prekey.id == id {
            return Some(&self.bundle.active_signed_prekey);
        }

        self.bundle.previous_signed_prekeys.iter().find(|prekey| prekey.id == id)
    }


    pub fn find_one_time_prekey(&self, id: u32) -> Option<&OneTimePrekey> {
        self.bundle.one_time_prekeys.iter().find(|prekey| prekey.id == id)
    }


    pub fn get_one_time_prekey_count(&self) -> usize {
        self.bundle.one_time_prekeys.len()
    }


    pub fn consume_one_time_prekey(&mut self, id: u32) -> Option<OneTimePrekey> {
        let position = self.bundle.one_time_prekeys.iter().position(|prekey| prekey.id == id)?;

        Some(self.bundle.one_time_prekeys.remove(position))
    }


    pub fn replenish_one_time_prekeys(&mut self, target_count: usize) -> Result<usize, KeyManagerError> {
        if target_count > MAX_ONE_TIME_PREKEYS {
            return Err(KeyManagerError::OneTimePrekeyLimitNotSufficient);
        }

        let current_count = self.bundle.one_time_prekeys.len();

        if current_count >= target_count {
            return Ok(0);
        }

        let required_count = target_count - current_count;

        let start_id = self.next_one_time_prekey_id.ok_or(KeyManagerError::OneTimePrekeyIdExhausted)?;

        let new_prekeys = KeyGenerator::generate_one_time_prekeys(start_id, required_count)
            .map_err(|_| KeyManagerError::GeneratorError)?;

        let required_u32 = u32::try_from(required_count).map_err(|_| KeyManagerError::InvalidPrekeyRange)?;

        self.next_one_time_prekey_id = start_id.checked_add(required_u32);

        self.bundle.one_time_prekeys.extend(new_prekeys);

        Ok(required_count)
    }
}