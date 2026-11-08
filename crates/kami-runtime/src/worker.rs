use crate::{Error, Profile};
use kami_core::{DeviceId, MessageId};
use kami_identity::{NativeCustody, SealedTransportCredentials};
use kami_storage::{ProfileLease, Storage};
use tokio::sync::{mpsc, oneshot, watch};

pub(crate) struct Authority(pub watch::Sender<bool>, pub kami_transport::Revocation);
impl Drop for Authority {
    fn drop(&mut self) {
        self.1.revoke();
        self.0.send_replace(false);
    }
}
#[cfg(test)]
type TestWork = Box<dyn FnOnce(&Storage) -> Result<usize, Error> + Send>;
pub(crate) enum Work {
    Recover,
    RetryScan,
    Cancel(DeviceId, MessageId),
    Contact(
        [u8; 32],
        oneshot::Sender<Result<kami_identity::ContactTrust, Error>>,
    ),
    #[cfg(test)]
    Test(TestWork),
}
pub(crate) enum Completion {
    Contacts(Vec<([u8; 32], Option<kami_identity::ContactTrust>)>),
    Ready(Result<(usize, Option<Box<SealedTransportCredentials>>), Error>),
    Done(u64, Result<usize, Error>),
    Closed(Result<(), Error>),
}
/// All storage/native-provider operations remain on this bounded blocking task.
/// Its authority channel revokes on return/panic, independently of diagnostics.
pub(crate) fn run(
    profile: Profile,
    mut jobs: mpsc::Receiver<(u64, Work)>,
    done: mpsc::Sender<Completion>,
    direct: bool,
    authority: Authority,
) {
    let key_id = profile.key_id.clone();
    let result = (|| {
        let mut lease = ProfileLease::acquire(&profile.directory).map_err(|e| {
            if matches!(
                e.get_ref()
                    .and_then(|s| s.downcast_ref::<std::fs::TryLockError>()),
                Some(std::fs::TryLockError::WouldBlock)
            ) {
                Error::ProfileAlreadyOwned
            } else {
                Error::Ownership
            }
        })?;
        lease.record_owner().map_err(|_| Error::Ownership)?;
        let native = NativeCustody::open();
        let fresh = if direct && profile.create {
            for name in [
                "messages.db",
                "enrollment",
                "messages.db-wal",
                "messages.db-shm",
                "messages.db-journal",
            ] {
                if profile
                    .directory
                    .join(name)
                    .try_exists()
                    .map_err(|_| Error::Ownership)?
                {
                    return Err(Error::Identity(kami_key_provider::KeyError::Corrupt));
                }
            }
            Some(kami_identity::begin_fresh_native(&native, &key_id).map_err(Error::Identity)?)
        } else {
            None
        };
        let provider: std::sync::Arc<dyn kami_key_provider::KeyProvider> = if direct {
            std::sync::Arc::new(NativeCustody::open())
        } else {
            profile.key_provider
        };
        let storage = if profile.create {
            Storage::create(lease, profile.key_id, provider, profile.storage)
        } else {
            Storage::open(lease, profile.key_id, provider, profile.storage)
        }
        .map_err(Error::Storage)?;
        if let Some(fresh) = fresh {
            let committed = storage
                .commit_fresh_enrollment(fresh.canonical_row())
                .map_err(Error::Storage)?;
            kami_identity::complete_fresh_enrollment(&native, fresh, &committed)
                .map_err(Error::Identity)?;
        }
        let credentials = if direct {
            let row = enrollment_row(&storage)?;
            let verified = kami_identity::verify_enrollment_for_transport(&native, &key_id, &row)
                .map_err(Error::Identity)?;
            let fresh = enrollment_row(&storage)?;
            Some(Box::new(
                verified
                    .materialize(&native, &fresh)
                    .map_err(Error::Identity)?,
            ))
        } else {
            None
        };
        let count = reconcile(&storage)?;
        Ok::<_, Error>((storage, count, credentials))
    })();
    let (storage, count, credentials) = match result {
        Ok(v) => v,
        Err(e) => {
            let _ = done.blocking_send(Completion::Ready(Err(e)));
            return;
        }
    };
    // Created after Storage so unwind revokes transport before dropping/joining
    // the lease-holding DB worker, even when an admitted operation panics.
    let live_guard = Authority(authority.0.clone(), authority.1.clone());
    authority.0.send_replace(true);
    if done
        .blocking_send(Completion::Ready(Ok((count, credentials))))
        .is_err()
    {
        return;
    }
    let mut known = std::collections::HashSet::new();
    while let Some((id, work)) = jobs.blocking_recv() {
        if direct {
            let checked = enrollment_row(&storage).and_then(|row| {
                kami_identity::verify_enrollment_for_transport(
                    &NativeCustody::open(),
                    &key_id,
                    &row,
                )
                .map(|_| ())
                .map_err(Error::Identity)
            });
            if let Err(error) = checked {
                authority.1.revoke();
                authority.0.send_replace(false);
                let _ = done.blocking_send(Completion::Done(id, Err(error)));
                break;
            }
        }
        let result = match work {
            Work::Recover => reconcile(&storage),
            Work::RetryScan => {
                if direct {
                    let updates = known
                        .iter()
                        .map(|device| (*device, contact(&storage, *device).ok()))
                        .collect();
                    if done.blocking_send(Completion::Contacts(updates)).is_err() {
                        break;
                    }
                }
                now_ms().and_then(|now| retry_due(&storage, now))
            }
            Work::Cancel(sender, id) => storage
                .cancel_send(sender, id)
                .map(|()| 0)
                .map_err(Error::Storage),
            Work::Contact(device, reply) => {
                let result = if known.len() >= 32 && !known.contains(&device) {
                    Err(Error::Busy)
                } else {
                    contact(&storage, device)
                };
                if result.is_ok() {
                    known.insert(device);
                }
                let status = result.as_ref().map(|_| 0).map_err(|e| *e);
                let _ = reply.send(result);
                status
            }
            #[cfg(test)]
            Work::Test(f) => f(&storage),
        };
        if done.blocking_send(Completion::Done(id, result)).is_err() {
            break;
        }
    }
    drop(live_guard);
    drop(authority);
    // Do not abort this join or release ownership on a timeout. Supervisor closes
    // jobs only after transport children stopped, so lease outlives the swarm.
    let result = storage.shutdown().map_err(Error::Storage);
    let _ = done.blocking_send(Completion::Closed(result));
}
fn enrollment_row(storage: &Storage) -> Result<Vec<u8>, Error> {
    storage
        .enrollment_snapshot()
        .map_err(Error::Storage)?
        .ok_or(Error::Identity(kami_key_provider::KeyError::Missing))
}
fn contact(storage: &Storage, device: [u8; 32]) -> Result<kami_identity::ContactTrust, Error> {
    use kami_identity::ContactTrustState as I;
    use kami_storage::TrustState as S;
    let row = storage
        .load_contact(device)
        .map_err(Error::Storage)?
        .ok_or(Error::LiveTransportUnavailable)?;
    let state = match row.state() {
        S::Pending => I::Pending,
        S::Verified => I::Verified,
        S::Changed => I::Changed,
        S::Blocked => I::Blocked,
    };
    let trust = kami_identity::ContactTrust::from_persisted(
        state,
        row.generation(),
        row.pinned(),
        row.candidate(),
    )
    .map_err(|_| Error::OperationRejected)?;
    if trust.pinned().is_none_or(|pin| pin.device_key() != &device) {
        return Err(Error::LiveTransportUnavailable);
    }
    Ok(trust)
}
fn reconcile(storage: &Storage) -> Result<usize, Error> {
    projection(storage, None)
}
pub(crate) fn retry_due(storage: &Storage, now: i64) -> Result<usize, Error> {
    if now < 0 {
        return Err(Error::OperationRejected);
    }
    projection(storage, Some(now))
}
fn now_ms() -> Result<i64, Error> {
    let elapsed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| Error::OperationRejected)?;
    i64::try_from(elapsed.as_millis()).map_err(|_| Error::OperationRejected)
}
fn projection(storage: &Storage, now: Option<i64>) -> Result<usize, Error> {
    let mut after = None;
    let mut count = 0;
    loop {
        let page = storage
            .recover_sends_after(after, 64)
            .map_err(Error::Storage)?;
        count += page
            .iter()
            .filter(|row| {
                now.is_none_or(|now| match row.state() {
                    kami_storage::SendState::Queued => row.next_eligible_ms() <= now,
                    kami_storage::SendState::InFlight => row
                        .lease_expires_at_ms()
                        .is_some_and(|deadline| deadline <= now),
                    _ => false,
                })
            })
            .count();
        match page.last() {
            None => return Ok(count),
            Some(row) => {
                let f = row.facts();
                after = Some((f.conversation_id(), f.sender(), f.sequence()));
            }
        }
    }
}
