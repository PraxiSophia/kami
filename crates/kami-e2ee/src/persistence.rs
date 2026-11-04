//! Quarantined SQLCipher-bound DTOs, not general key export APIs.
use crate::{authenticated::Endpoint, state::MAX_STATE_BYTES, Error};
use sha2::{Digest, Sha256};
use vodozemac::olm::{Account, AccountPickle, Session, SessionPickle};
use zeroize::Zeroizing;

const HEADER: usize = 66;
const MAGIC: &[u8; 8] = b"KAMIWP3\0";
/// A digest retained outside a candidate detects branch substitution only within
/// this process. A digest loaded from the same database is NOT a rollback anchor.
pub(crate) enum RestoreAuthority {
    InProcess([u8; 32]),
    ProfileRestore,
}
struct Snapshot(Zeroizing<Vec<u8>>);
impl Snapshot {
    fn new(
        kind: u8,
        generation: u64,
        account_generation: u64,
        context: [u8; 32],
        pickle: &[u8],
        initiator: bool,
    ) -> Result<Self, Error> {
        if generation == 0
            || generation > i64::MAX as u64
            || account_generation == 0
            || account_generation > i64::MAX as u64
            || pickle.is_empty()
            || pickle.len() > MAX_STATE_BYTES
        {
            return Err(Error::StateEncoding);
        }
        let mut bytes = Zeroizing::new(Vec::with_capacity(HEADER + pickle.len()));
        bytes.extend(MAGIC);
        bytes.extend([1, kind]); // DTO version and fixed Olm-v1 provider kind.
        bytes.extend(generation.to_be_bytes());
        bytes.extend(account_generation.to_be_bytes());
        bytes.extend(context);
        bytes.push(u8::from(initiator));
        bytes.extend((pickle.len() as u32).to_be_bytes());
        bytes.extend([0; 3]); // Reserved profile fields, must remain zero.
        bytes.extend(pickle);
        Self::parse(&bytes, kind)
    }
    fn parse(bytes: &[u8], kind: u8) -> Result<Self, Error> {
        if bytes.len() < HEADER
            || bytes.len() > HEADER + MAX_STATE_BYTES
            || &bytes[..8] != MAGIC
            || bytes[8] != 1
            || bytes[9] != kind
            || bytes[58] > 1
            || bytes[63..66] != [0; 3]
        {
            return Err(Error::StateEncoding);
        }
        let length = u32::from_be_bytes(bytes[59..63].try_into().map_err(|_| Error::StateEncoding)?)
            as usize;
        let generation =
            u64::from_be_bytes(bytes[10..18].try_into().map_err(|_| Error::StateEncoding)?);
        let account_generation =
            u64::from_be_bytes(bytes[18..26].try_into().map_err(|_| Error::StateEncoding)?);
        if length == 0
            || length > MAX_STATE_BYTES
            || bytes.len() != HEADER + length
            || generation == 0
            || generation > i64::MAX as u64
            || account_generation == 0
            || account_generation > i64::MAX as u64
        {
            return Err(Error::StateEncoding);
        }
        Ok(Self(Zeroizing::new(bytes.to_vec())))
    }
    fn digest(&self) -> [u8; 32] {
        Sha256::digest(&*self.0).into()
    }
    fn checked(
        &self,
        generation: u64,
        account_generation: u64,
        context: [u8; 32],
        authority: RestoreAuthority,
    ) -> Result<&[u8], Error> {
        match authority {
            RestoreAuthority::ProfileRestore => return Err(Error::RollbackAnchorUnsupported),
            RestoreAuthority::InProcess(digest) if digest != self.digest() => {
                return Err(Error::StateEncoding)
            }
            RestoreAuthority::InProcess(_) => {}
        }
        if self.0[10..18] != generation.to_be_bytes()
            || self.0[18..26] != account_generation.to_be_bytes()
            || self.0[26..58] != context
        {
            return Err(Error::StateEncoding);
        }
        Ok(&self.0[HEADER..])
    }
}
/// Includes upstream OTK/fallback keys; exported only by consuming a valid commit.
/// This custodian boundary is not a raw individual-key getter.
pub(crate) struct AccountSnapshot(Snapshot);
pub(crate) struct SessionSnapshot(Snapshot);
impl AccountSnapshot {
    pub(crate) fn capture(
        account: &Account,
        endpoint: &Endpoint,
        generation: u64,
    ) -> Result<Self, Error> {
        endpoint.matches_account(account)?;
        let pickle = Zeroizing::new(
            serde_json::to_vec(&account.pickle()).map_err(|_| Error::StateEncoding)?,
        );
        Ok(Self(Snapshot::new(
            1,
            generation,
            generation,
            endpoint.digest(),
            &pickle,
            false,
        )?))
    }
    pub(crate) fn parse(bytes: &[u8]) -> Result<Self, Error> {
        Snapshot::parse(bytes, 1).map(Self)
    }
    pub(crate) fn into_bytes(self) -> Zeroizing<Vec<u8>> {
        self.0 .0
    }
    pub(crate) fn bytes(&self) -> &[u8] {
        &self.0 .0
    }
    pub(crate) fn digest(&self) -> [u8; 32] {
        self.0.digest()
    }
    pub(crate) fn restore(
        &self,
        endpoint: &Endpoint,
        generation: u64,
        authority: RestoreAuthority,
    ) -> Result<Account, Error> {
        let bytes = self
            .0
            .checked(generation, generation, endpoint.digest(), authority)?;
        let pickle =
            serde_json::from_slice::<AccountPickle>(bytes).map_err(|_| Error::StateEncoding)?;
        let account = Account::from_pickle(pickle);
        endpoint.matches_account(&account)?;
        Ok(account)
    }
}
impl SessionSnapshot {
    pub(crate) fn capture(
        session: &Session,
        generation: u64,
        account_generation: u64,
        context: [u8; 32],
        initiator: bool,
    ) -> Result<Self, Error> {
        if session.session_config() != vodozemac::olm::SessionConfig::version_1() {
            return Err(Error::StateEncoding);
        }
        let pickle = Zeroizing::new(
            serde_json::to_vec(&session.pickle()).map_err(|_| Error::StateEncoding)?,
        );
        Ok(Self(Snapshot::new(
            2,
            generation,
            account_generation,
            context,
            &pickle,
            initiator,
        )?))
    }
    pub(crate) fn parse(bytes: &[u8]) -> Result<Self, Error> {
        Snapshot::parse(bytes, 2).map(Self)
    }
    pub(crate) fn into_bytes(self) -> Zeroizing<Vec<u8>> {
        self.0 .0
    }
    pub(crate) fn bytes(&self) -> &[u8] {
        &self.0 .0
    }
    pub(crate) fn digest(&self) -> [u8; 32] {
        self.0.digest()
    }
    pub(crate) fn restore(
        &self,
        generation: u64,
        account_generation: u64,
        context: [u8; 32],
        authority: RestoreAuthority,
    ) -> Result<(Session, bool), Error> {
        let bytes = self
            .0
            .checked(generation, account_generation, context, authority)?;
        let pickle =
            serde_json::from_slice::<SessionPickle>(bytes).map_err(|_| Error::StateEncoding)?;
        let session = Session::from_pickle(pickle);
        if session.session_config() != vodozemac::olm::SessionConfig::version_1() {
            return Err(Error::StateEncoding);
        }
        Ok((session, self.0 .0[58] == 1))
    }
}
