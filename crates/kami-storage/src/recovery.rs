//! SQLCipher evidence recovers artifacts, never mutable cryptographic authority.
//! No database-local record can distinguish a coherent older copy from current state.
use crate::StorageError;
use kami_key_provider::ProfileKeyId;
use rusqlite::Connection;

/// Process-local only. Never serialize this status into a rollbackable database.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecoveryStatus {
    /// New worker or reopened profile; exact durable artifacts remain readable.
    Disabled,
    /// This worker consumed a native, one-use enrollment and completed its anchor.
    FreshEnrollment,
    /// An ambiguous durable outcome invalidated this worker's mutable authority.
    Quarantined,
}

/// Requirements, not a capability or a database repair/restore authorization.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReestablishmentPreconditions {
    pub independently_fresh_enrollment: bool,
    pub fresh_account_and_prekeys: bool,
    pub new_session_identity: bool,
    pub independently_verified_contact: bool,
}
impl RecoveryStatus {
    pub fn preconditions(self) -> ReestablishmentPreconditions {
        ReestablishmentPreconditions {
            independently_fresh_enrollment: true,
            fresh_account_and_prekeys: true,
            new_session_identity: true,
            independently_verified_contact: true,
        }
    }
    pub(crate) fn require(self) -> Result<(), StorageError> {
        if self == Self::FreshEnrollment {
            Ok(())
        } else {
            Err(StorageError::EnrollmentRequired)
        }
    }
    pub(crate) fn observe<T>(
        &mut self,
        result: Result<T, StorageError>,
    ) -> Result<T, StorageError> {
        if matches!(
            result,
            Err(StorageError::Io | StorageError::WrongKeyOrCorrupt)
        ) {
            *self = Self::Quarantined;
        }
        result
    }
}

/// SQLite recovers the WAL before this transaction. Validate its coherent view;
/// clean shutdown, generations, and quarantine/retirement markers grant nothing.
pub(crate) fn startup(db: &mut Connection) -> Result<RecoveryStatus, StorageError> {
    let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    let check: String = tx.query_row("PRAGMA quick_check(1)", [], |r| r.get(0))?;
    if check != "ok"
        || tx
            .prepare("PRAGMA foreign_key_check")?
            .query([])?
            .next()?
            .is_some()
    {
        return Err(StorageError::WrongKeyOrCorrupt);
    }
    tx.commit()?;
    Ok(RecoveryStatus::Disabled)
}

/// Read-only exact request recovery. Never delegate disabled requests to a writer,
/// including between a preflight check and that writer's transaction.
pub(crate) fn prepare(
    db: &mut Connection,
    authority: RecoveryStatus,
    request: crate::SendRequest,
    now: i64,
    quota: u32,
) -> Result<crate::Preparation, StorageError> {
    if authority == RecoveryStatus::FreshEnrollment {
        return crate::sender_state::prepare(db, request, now, quota);
    }
    use hmac::{Hmac, KeyInit, Mac};
    use rusqlite::{params, OptionalExtension};
    let tx = db.transaction()?;
    let binding: Option<Vec<u8>> = tx
        .query_row(
            "SELECT request_mac FROM sender_prepare WHERE sender=?1 AND id=?2 AND state=1",
            params![
                request.sender.as_bytes(),
                request.message.as_u128().to_be_bytes()
            ],
            |r| r.get(0),
        )
        .optional()?;
    let binding = binding.ok_or(StorageError::EnrollmentRequired)?;
    let key = zeroize::Zeroizing::new(tx.query_row(
        "SELECT key FROM sender_binding_key WHERE singleton=1",
        [],
        |r| r.get::<_, Vec<u8>>(0),
    )?);
    if key.len() != 32 {
        return Err(StorageError::WrongKeyOrCorrupt);
    }
    let mut mac =
        Hmac::<sha2::Sha256>::new_from_slice(&key).map_err(|_| StorageError::WrongKeyOrCorrupt)?;
    mac.update(b"kami-sender-request-v1\0");
    mac.update(&request.message.as_u128().to_be_bytes());
    mac.update(&request.conversation.as_u128().to_be_bytes());
    mac.update(request.sender.as_bytes());
    mac.update(request.recipient.as_bytes());
    mac.update(&request.created_at_ms.to_be_bytes());
    mac.update(&(request.text().len() as u64).to_be_bytes());
    mac.update(request.text().as_bytes());
    mac.verify_slice(&binding)
        .map_err(|_| StorageError::Conflict)?;
    let message = crate::sender_state::load(&tx, request.sender, request.message)?;
    Ok(crate::Preparation::Committed(message))
}

/// Consume only to match exact durable evidence; no writer or missing-row repair.
pub(crate) fn authenticated(
    db: &mut Connection,
    stage: kami_e2ee::AuthenticatedCommit,
    prepared: Option<&crate::PreparedSend>,
) -> Result<Vec<u8>, StorageError> {
    let material = stage.into_material();
    let facts = material.facts();
    let incoming = material.incoming();
    let frame = material.frame().to_vec();
    let id = material.session_id();
    let generation = material.session_generation();
    let contact = material.contact();
    let trust_generation = material.trust_generation();
    let pin = material.pin().to_vec();
    let context = material.context().to_vec();
    let digest = material.next_digest();
    let expected =
        material
            .into_snapshots()
            .consume(|_, _, _, state| crate::session_state::SessionRow {
                id,
                generation,
                contact,
                trust_generation,
                pin,
                context,
                digest,
                state,
            });
    let tx = db.transaction()?;
    if let Some(reservation) = prepared {
        crate::sender_state::check_committed(&tx, reservation, facts)?;
    }
    crate::session_state::reconcile(&tx, &expected, facts, &frame, incoming)?
        .ok_or(StorageError::EnrollmentRequired)
}

/// Keep all linked artifact reads on one coherent SQLCipher snapshot.
pub(crate) fn read<T>(
    db: &mut Connection,
    read: impl FnOnce(&Connection) -> Result<T, StorageError>,
) -> Result<T, StorageError> {
    let tx = db.transaction()?;
    read(&tx)
}

/// The worker owns BOTH phases: durable row/readback, then native completion and
/// verification. A lost reply cannot replay this non-cloneable capability.
pub(crate) fn enroll(
    db: &mut Connection,
    lease: &crate::ProfileLease,
    profile: &ProfileKeyId,
    create: bool,
    max_database_pages: u32,
    fresh: kami_identity::FreshEnrollment,
) -> Result<(), StorageError> {
    if !create {
        return Err(StorageError::EnrollmentRequired);
    }
    // Freshness cannot be grafted onto a profile with prior cryptographic state.
    let occupied: bool = db.query_row(
        "SELECT EXISTS(SELECT 1 FROM e2ee_account) OR EXISTS(SELECT 1 FROM e2ee_session) OR EXISTS(SELECT 1 FROM e2ee_frame) OR EXISTS(SELECT 1 FROM enrollment_complete)",
        [], |r| r.get(0),
    )?;
    if occupied {
        return Err(StorageError::EnrollmentRequired);
    }
    kami_identity::validate_enrollment_snapshot(profile, fresh.canonical_row())
        .map_err(|_| StorageError::InvalidInput)?;
    let native = kami_identity::NativeCustody::open();
    // Verify the actual encrypted database with the newly provisioned native
    // key. Re-loading a generic provider would not bind its earlier key choice.
    drop(crate::open::connect(
        lease,
        profile,
        &native,
        false,
        max_database_pages,
    )?);
    let committed = crate::enrollment::commit(db, profile, fresh.canonical_row(), true)?;
    kami_identity::complete_fresh_enrollment(&native, fresh, &committed)
        .map_err(StorageError::Key)?;
    let readback = crate::enrollment::read(db, profile)?.ok_or(StorageError::WrongKeyOrCorrupt)?;
    if readback != committed {
        return Err(StorageError::WrongKeyOrCorrupt);
    }
    kami_identity::verify_enrollment_for_transport(&native, profile, &readback)
        .map_err(StorageError::Key)?;
    Ok(())
}
