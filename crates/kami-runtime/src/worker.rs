use crate::{Error, Profile};
use kami_core::{DeviceId, MessageId};
use kami_storage::{ProfileLease, Storage};
use tokio::sync::mpsc;

#[cfg(test)]
type TestWork = Box<dyn FnOnce(&Storage) -> Result<usize, Error> + Send>;

pub(crate) enum Work {
    Recover,
    RetryScan,
    Cancel(DeviceId, MessageId),
    #[cfg(test)]
    Test(TestWork),
}
pub(crate) enum Completion {
    Ready(Result<usize, Error>),
    Done(u64, Result<usize, Error>),
    Closed(Result<(), Error>),
}

/// All synchronous APIs stay on this blocking task. Storage itself holds the
/// existing kernel profile lock on its worker until its join finishes.
pub(crate) fn run(
    profile: Profile,
    mut jobs: mpsc::Receiver<(u64, Work)>,
    done: mpsc::Sender<Completion>,
) {
    let result = (|| {
        let mut lease = ProfileLease::acquire(&profile.directory).map_err(|e| {
            if matches!(
                e.get_ref()
                    .and_then(|source| source.downcast_ref::<std::fs::TryLockError>()),
                Some(std::fs::TryLockError::WouldBlock)
            ) {
                Error::ProfileAlreadyOwned
            } else {
                Error::Ownership
            }
        })?;
        lease.record_owner().map_err(|_| Error::Ownership)?;
        let storage = if profile.create {
            Storage::create(lease, profile.key_id, profile.key_provider, profile.storage)
        } else {
            Storage::open(lease, profile.key_id, profile.key_provider, profile.storage)
        }
        .map_err(Error::Storage)?;
        let count = reconcile(&storage)?;
        Ok::<_, Error>((storage, count))
    })();
    let (storage, count) = match result {
        Ok(value) => value,
        Err(e) => {
            let _ = done.blocking_send(Completion::Ready(Err(e)));
            return;
        }
    };
    if done.blocking_send(Completion::Ready(Ok(count))).is_err() {
        return;
    }
    while let Some((id, work)) = jobs.blocking_recv() {
        let result = match work {
            Work::Recover => reconcile(&storage),
            Work::RetryScan => now_ms().and_then(|now| retry_due(&storage, now)),
            Work::Cancel(sender, id) => storage
                .cancel_send(sender, id)
                .map(|()| 0)
                .map_err(Error::Storage),
            #[cfg(test)]
            Work::Test(f) => f(&storage),
        };
        if done.blocking_send(Completion::Done(id, result)).is_err() {
            break;
        }
    }
    // Do not abort this join or release ownership on a timeout.
    let result = storage.shutdown().map_err(Error::Storage);
    let _ = done.blocking_send(Completion::Closed(result));
}

/// Read durable rows in bounded pages, never authorize transmission or override
/// leases. Existing sender backoff and fresh trust checks remain authoritative.
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
