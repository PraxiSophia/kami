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
    RecoveryStatus(oneshot::Sender<Result<kami_storage::RecoveryStatus, Error>>),
    MaintainPrekey(Option<usize>),
    InvalidatePrekey(Option<[u8; 32]>, u64, bool),
    IncomingPrekey(kami_transport::IncomingPrekeyExchange),
    BeginPrekey(
        std::sync::Arc<kami_transport::AuthenticatedConnection>,
        Option<[u8; 32]>,
        oneshot::Sender<Result<kami_protocol::prekey_exchange::Exchange, Error>>,
    ),
    RetainPrekey(
        kami_transport::LivePrekeyExchange,
        oneshot::Sender<Result<kami_protocol::prekey_exchange::Exchange, Error>>,
    ),
    FinishPrekey(kami_transport::LivePrekeyExchange),
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
            storage.enroll_fresh(fresh).map_err(Error::Storage)?;
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
    let local = credentials.as_ref().map(|c| c.invitation().clone());
    if done
        .blocking_send(Completion::Ready(Ok((count, credentials))))
        .is_err()
    {
        return;
    }
    let mut known = std::collections::HashSet::new();
    let mut prekeys = None;
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
            Work::RecoveryStatus(reply) => {
                let status = storage.recovery_status().map_err(Error::Storage);
                let result = status.map(|_| 0);
                let _ = reply.send(status);
                result
            }
            Work::MaintainPrekey(count) => {
                maintain_prekeys(&storage, &key_id, local.as_ref(), &mut prekeys, count)
            }
            Work::InvalidatePrekey(device, through, retire) => storage
                .prekey(kami_storage::prekeys::PrekeyOperation::Invalidate {
                    device,
                    through,
                    retire,
                    now_ms: now_ms().unwrap_or(-1),
                })
                .map(|_| 0)
                .map_err(Error::Storage),
            Work::IncomingPrekey(incoming) => {
                let result =
                    respond_prekey(&storage, &key_id, local.as_ref(), &mut prekeys, &incoming);
                match result {
                    Ok(value) => {
                        let fresh = contact(&storage, incoming.evidence().remote_device());
                        fresh
                            .and_then(|fresh| {
                                incoming.respond(&fresh, value).map_err(Error::Transport)
                            })
                            .map(|_| 0)
                    }
                    Err(error) => {
                        incoming.reject(prekey_failure(error));
                        Err(error)
                    }
                }
            }
            Work::BeginPrekey(cap, request, reply) => {
                let result = begin_prekey(&storage, local.as_ref(), &cap, request);
                let status = result.as_ref().map(|_| 0).map_err(|e| *e);
                let _ = reply.send(result);
                status
            }
            Work::RetainPrekey(evidence, reply) => {
                let result = retain_prekey(&storage, &evidence, false);
                let status = result.as_ref().map(|_| 0).map_err(|e| *e);
                let _ = reply.send(result);
                status
            }
            Work::FinishPrekey(evidence) => retain_prekey(&storage, &evidence, true).map(|_| 0),
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
fn maintain_prekeys(
    storage: &Storage,
    key_id: &kami_key_provider::ProfileKeyId,
    local: Option<&kami_identity::Invitation>,
    owner: &mut Option<kami_e2ee::prekeys::AccountLifecycle>,
    count: Option<usize>,
) -> Result<usize, Error> {
    let local = local.ok_or(Error::LiveTransportUnavailable)?;
    let current = owner.as_ref().ok_or(Error::OperationRejected)?;
    let now = now_ms()? as u64;
    let proposal = match count {
        Some(count) => current.replenish(count, now, kami_e2ee::prekeys::MAX_VALIDITY_MS),
        None => current.rotate_fallback(now, kami_e2ee::prekeys::MAX_VALIDITY_MS),
    }
    .map_err(|_| Error::OperationRejected)?;
    let native = NativeCustody::open();
    let row = enrollment_row(storage)?;
    let verified = kami_identity::verify_enrollment_for_transport(&native, key_id, &row)
        .map_err(Error::Identity)?;
    let mut signed = native
        .create_olm_bindings(&verified, &row, local.binding(), proposal.inventory())
        .map_err(|_| Error::OperationRejected)?
        .into_iter();
    let commit = proposal
        .prepare_commit(Some(current), |data| {
            signed
                .next()
                .filter(|p| p.data() == &data)
                .ok_or(kami_identity::IdentityError::Conflict)
        })
        .map_err(|_| Error::OperationRejected)?;
    match storage.prekey(kami_storage::prekeys::PrekeyOperation::Install(commit)) {
        Ok(_) => {
            *owner = Some(proposal);
            Ok(0)
        }
        // Ambiguous worker/commit errors never install or regenerate a candidate.
        Err(error) => {
            *owner = None;
            Err(Error::Storage(error))
        }
    }
}
fn prekey_failure(error: Error) -> kami_protocol::prekey_exchange::Failure {
    use kami_protocol::prekey_exchange::Failure as F;
    match error {
        Error::Busy => F::Busy,
        Error::Identity(_) => F::CustodyUnavailable,
        Error::Storage(kami_storage::StorageError::Busy) => F::Busy,
        Error::Storage(kami_storage::StorageError::Conflict) => F::Conflict,
        Error::Storage(_) => F::Indeterminate,
        Error::Transport(kami_transport::Error::Expired) => F::Expired,
        Error::Transport(_) => F::Unauthorized,
        _ => F::Depleted,
    }
}
fn prekey_fence(
    storage: &Storage,
    evidence: &kami_transport::LivePrekeyExchange,
) -> Result<kami_storage::prekeys::PrekeyFence, Error> {
    let fresh = contact(storage, evidence.remote_device())?;
    let (_, peer) = evidence.inspect(&fresh).map_err(Error::Transport)?;
    Ok(kami_storage::prekeys::PrekeyFence {
        contact: fresh,
        peer: peer.to_bytes(),
        now_ms: now_ms()?,
    })
}
fn begin_prekey(
    storage: &Storage,
    local: Option<&kami_identity::Invitation>,
    cap: &kami_transport::AuthenticatedConnection,
    request: Option<[u8; 32]>,
) -> Result<kami_protocol::prekey_exchange::Exchange, Error> {
    use kami_storage::prekeys::{PrekeyFence, PrekeyOperation as O, PrekeyOutcome as R};
    let fresh = contact(storage, cap.remote_device())?;
    let peer = cap.authenticated_peer(&fresh).map_err(Error::Transport)?;
    let requester = *local.ok_or(Error::LiveTransportUnavailable)?.device_key();
    let retry = request.is_some();
    let request = match request {
        Some(id) => id,
        None => kami_e2ee::prekeys::challenge().map_err(|_| Error::OperationRejected)?,
    };
    let value = storage
        .prekey(O::Begin {
            fence: PrekeyFence {
                contact: fresh,
                peer: peer.to_bytes(),
                now_ms: now_ms()?,
            },
            requester,
            request,
            retry,
        })
        .map_err(Error::Storage)?;
    cap.authenticated_peer(&contact(storage, cap.remote_device())?)
        .map_err(Error::Transport)?;
    match value {
        R::Exchange(value) => Ok(value),
        _ => Err(Error::OperationRejected),
    }
}
fn retain_prekey(
    storage: &Storage,
    evidence: &kami_transport::LivePrekeyExchange,
    finish: bool,
) -> Result<kami_protocol::prekey_exchange::Exchange, Error> {
    use kami_protocol::prekey_exchange::Exchange as E;
    use kami_storage::prekeys::{PrekeyOperation as O, PrekeyOutcome as R};
    let fence = prekey_fence(storage, evidence)?;
    let (value, _) = evidence.inspect(&fence.contact).map_err(Error::Transport)?;
    let operation = match (finish, value) {
        (false, E::Selection(selection)) => O::Retain {
            fence,
            selection: selection.clone(),
        },
        (true, E::AckResult(acknowledgement)) => O::Finish {
            fence,
            acknowledgement: acknowledgement.clone(),
        },
        _ => return Err(Error::OperationRejected),
    };
    let value = storage.prekey(operation).map_err(Error::Storage)?;
    prekey_fence(storage, evidence)?;
    match value {
        R::Exchange(value) => Ok(value),
        _ => Err(Error::OperationRejected),
    }
}
fn respond_prekey(
    storage: &Storage,
    key_id: &kami_key_provider::ProfileKeyId,
    local: Option<&kami_identity::Invitation>,
    owner: &mut Option<kami_e2ee::prekeys::AccountLifecycle>,
    incoming: &kami_transport::IncomingPrekeyExchange,
) -> Result<kami_protocol::prekey_exchange::Exchange, Error> {
    use kami_protocol::prekey_exchange::Exchange as E;
    use kami_storage::prekeys::{PrekeyOperation as O, PrekeyOutcome as R};
    let local = local.ok_or(Error::LiveTransportUnavailable)?;
    let evidence = incoming.evidence();
    let fence = prekey_fence(storage, evidence)?;
    let (value, _) = evidence.inspect(&fence.contact).map_err(Error::Transport)?;
    let result = match value {
        E::Query(query) => {
            if query.publisher != *local.device_key() {
                return Err(Error::OperationRejected);
            }
            let state = storage
                .prekey(O::State {
                    fence,
                    request: query.request,
                })
                .map_err(Error::Storage)?;
            let R::State(mut state) = state else {
                return Err(Error::OperationRejected);
            };
            if let Some(frozen) = state.frozen {
                // Freeze rechecks the exact original query and current authority.
                let E::Selection(ref s) = frozen else {
                    return Err(Error::OperationRejected);
                };
                let proof = kami_identity::OlmBindingProof::parse_bounded(&s.proof)
                    .map_err(|_| Error::OperationRejected)?;
                storage
                    .prekey(O::Freeze {
                        fence: prekey_fence(storage, evidence)?,
                        query: query.clone(),
                        proof: Box::new(proof),
                        account_generation: s.account_generation,
                        account_digest: state.account_digest.ok_or(Error::OperationRejected)?,
                    })
                    .map_err(Error::Storage)?
            } else {
                if state.account_generation.is_none() {
                    let proposal = kami_e2ee::prekeys::AccountLifecycle::create(
                        *local.device_key(),
                        local.binding().peer_id(),
                        1,
                        now_ms()? as u64,
                        kami_e2ee::prekeys::MAX_VALIDITY_MS,
                    )
                    .map_err(|_| Error::OperationRejected)?;
                    let native = NativeCustody::open();
                    let row = enrollment_row(storage)?;
                    let enrollment =
                        kami_identity::verify_enrollment_for_transport(&native, key_id, &row)
                            .map_err(Error::Identity)?;
                    let mut signed = native
                        .create_olm_bindings(
                            &enrollment,
                            &row,
                            local.binding(),
                            proposal.inventory(),
                        )
                        .map_err(|_| Error::OperationRejected)?
                        .into_iter();
                    let commit = proposal
                        .prepare_commit(None, |data| {
                            signed
                                .next()
                                .filter(|p| p.data() == &data)
                                .ok_or(kami_identity::IdentityError::Conflict)
                        })
                        .map_err(|_| Error::OperationRejected)?;
                    storage.prekey(O::Install(commit)).map_err(Error::Storage)?;
                    *owner = Some(proposal);
                    let R::State(fresh) = storage
                        .prekey(O::State {
                            fence: prekey_fence(storage, evidence)?,
                            request: query.request,
                        })
                        .map_err(Error::Storage)?
                    else {
                        return Err(Error::OperationRejected);
                    };
                    state = fresh;
                }
                // Existing persisted accounts are never silently regenerated or
                // restored from database-only authority. WP-C remains separate.
                let owner = owner.as_ref().ok_or(Error::OperationRejected)?;
                if state.account_generation != Some(owner.generation())
                    || state.account_digest
                        != Some(
                            owner
                                .snapshot_digest()
                                .map_err(|_| Error::OperationRejected)?,
                        )
                {
                    return Err(Error::OperationRejected);
                }
                let proof = owner
                    .select(&state.available, now_ms()? as u64, query.minimum_generation)
                    .map_err(|_| Error::OperationRejected)?;
                storage
                    .prekey(O::Freeze {
                        fence: prekey_fence(storage, evidence)?,
                        query: query.clone(),
                        proof: Box::new(proof),
                        account_generation: owner.generation(),
                        account_digest: state.account_digest.ok_or(Error::OperationRejected)?,
                    })
                    .map_err(Error::Storage)?
            }
        }
        E::Ack(acknowledgement) => storage
            .prekey(O::Acknowledge {
                fence,
                acknowledgement: acknowledgement.clone(),
            })
            .map_err(Error::Storage)?,
        _ => return Err(Error::OperationRejected),
    };
    prekey_fence(storage, evidence)?;
    let row = enrollment_row(storage)?;
    kami_identity::verify_enrollment_for_transport(&NativeCustody::open(), key_id, &row)
        .map_err(Error::Identity)?;
    match result {
        R::Exchange(value) => Ok(value),
        _ => Err(Error::OperationRejected),
    }
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
