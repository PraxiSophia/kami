use crate::StorageError;
use rusqlite::{params, Connection, OptionalExtension};
use std::fmt;

pub const MAX_CONTACT_DESCRIPTOR_BYTES: usize = 512;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum TrustState {
    Pending = 0,
    Verified = 1,
    Changed = 2,
    Blocked = 3,
}
/// Neutral, bounded service DTO. Blobs hold canonical public descriptors, not
/// secrets or message payloads. Storage does not authenticate their signatures.
/// The trusted service maps/revalidates them through identity on every load.
#[derive(Clone, Eq, PartialEq)]
pub struct ContactTrustRow {
    contact: [u8; 32],
    state: TrustState,
    generation: u64,
    pinned: Option<Vec<u8>>,
    candidate: Vec<u8>,
}
impl ContactTrustRow {
    pub fn new(
        contact: [u8; 32],
        state: TrustState,
        generation: u64,
        pinned: Option<Vec<u8>>,
        candidate: Vec<u8>,
    ) -> Result<Self, StorageError> {
        if generation == 0
            || generation > i64::MAX as u64
            || candidate.is_empty()
            || candidate.len() > MAX_CONTACT_DESCRIPTOR_BYTES
            || pinned
                .as_ref()
                .is_some_and(|v| v.is_empty() || v.len() > MAX_CONTACT_DESCRIPTOR_BYTES)
        {
            return Err(StorageError::InvalidInput);
        }
        match state {
            TrustState::Pending if pinned.is_some() => return Err(StorageError::InvalidInput),
            TrustState::Verified if pinned.as_ref() != Some(&candidate) => {
                return Err(StorageError::InvalidInput)
            }
            TrustState::Changed if pinned.is_none() => return Err(StorageError::InvalidInput),
            _ => {}
        }
        Ok(Self {
            contact,
            state,
            generation,
            pinned,
            candidate,
        })
    }
    pub fn contact(&self) -> &[u8; 32] {
        &self.contact
    }
    pub fn state(&self) -> TrustState {
        self.state
    }
    pub fn generation(&self) -> u64 {
        self.generation
    }
    pub fn pinned(&self) -> Option<&[u8]> {
        self.pinned.as_deref()
    }
    pub fn candidate(&self) -> &[u8] {
        &self.candidate
    }
}
impl fmt::Debug for ContactTrustRow {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ContactTrustRow([REDACTED])")
    }
}
pub(crate) fn load(
    db: &Connection,
    contact: [u8; 32],
) -> Result<Option<ContactTrustRow>, StorageError> {
    // Inspect lengths before copying, even if a privileged writer disabled CHECKs.
    let mut statement = db.prepare("SELECT format_version,state,generation,length(pinned),length(candidate),pinned,candidate FROM contact_trust WHERE contact=?1")?;
    let mut rows = statement.query([contact.as_slice()])?;
    let Some(row) = rows.next()? else {
        return Ok(None);
    };
    let version: i64 = row.get(0).map_err(|_| StorageError::WrongKeyOrCorrupt)?;
    let state: i64 = row.get(1).map_err(|_| StorageError::WrongKeyOrCorrupt)?;
    let generation: i64 = row.get(2).map_err(|_| StorageError::WrongKeyOrCorrupt)?;
    let pin_len: Option<i64> = row.get(3).map_err(|_| StorageError::WrongKeyOrCorrupt)?;
    let candidate_len: i64 = row.get(4).map_err(|_| StorageError::WrongKeyOrCorrupt)?;
    if version != 1
        || generation < 1
        || !(1..=512).contains(&candidate_len)
        || pin_len.is_some_and(|v| !(1..=512).contains(&v))
    {
        return Err(StorageError::WrongKeyOrCorrupt);
    }
    let state = match state {
        0 => TrustState::Pending,
        1 => TrustState::Verified,
        2 => TrustState::Changed,
        3 => TrustState::Blocked,
        _ => return Err(StorageError::WrongKeyOrCorrupt),
    };
    let pinned = row.get(5).map_err(|_| StorageError::WrongKeyOrCorrupt)?;
    let candidate = row.get(6).map_err(|_| StorageError::WrongKeyOrCorrupt)?;
    ContactTrustRow::new(contact, state, generation as u64, pinned, candidate)
        .map(Some)
        .map_err(|_| StorageError::WrongKeyOrCorrupt)
}
pub(crate) fn cas(
    db: &mut Connection,
    expected: Option<u64>,
    next: ContactTrustRow,
) -> Result<(), StorageError> {
    let generation = match expected {
        None => 1,
        Some(n) => n
            .checked_add(1)
            .filter(|v| n > 0 && *v <= i64::MAX as u64)
            .ok_or(StorageError::InvalidInput)?,
    };
    if next.generation != generation {
        return Err(StorageError::InvalidInput);
    }
    let tx = db.transaction()?;
    let current: Option<i64> = tx
        .query_row(
            "SELECT generation FROM contact_trust WHERE contact=?1",
            [next.contact.as_slice()],
            |r| r.get(0),
        )
        .optional()?;
    if current.map(|n| n as u64) != expected {
        return Err(StorageError::Conflict);
    }
    // No unrestricted KV or SQL API, and no storage -> identity dependency.
    tx.execute("INSERT INTO contact_trust(contact,format_version,state,generation,pinned,candidate) VALUES(?1,1,?2,?3,?4,?5) ON CONFLICT(contact) DO UPDATE SET state=excluded.state,generation=excluded.generation,pinned=excluded.pinned,candidate=excluded.candidate", params![next.contact.as_slice(), next.state as u8, next.generation as i64, next.pinned, next.candidate])?;
    tx.commit()?;
    Ok(())
}
