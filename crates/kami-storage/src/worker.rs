use crate::{open, ProfileLease, StorageError};
use kami_core::{ConversationId, DeviceId, MessageFacts, MessageId};
use kami_key_provider::{KeyProvider, ProfileKeyId};
use std::sync::{mpsc, Arc};
use std::thread::{self, JoinHandle};
#[cfg(test)]
mod tests;

type Reply<T> = mpsc::SyncSender<Result<T, StorageError>>;
#[derive(Clone, Copy, Debug)]
pub struct Config {
    pub queue_capacity: usize,
    pub outbox_bytes: u64,
    pub outbox_messages: u32,
    pub max_database_pages: u32,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            queue_capacity: 32,
            outbox_bytes: 16 * 1024 * 1024,
            outbox_messages: 4096,
            max_database_pages: 16_384,
        }
    }
}
enum Request {
    Prekey(
        Box<crate::prekeys::PrekeyOperation>,
        Reply<crate::prekeys::PrekeyOutcome>,
    ),
    ReadEnrollment(Reply<Option<Vec<u8>>>),
    CommitEnrollment(Vec<u8>, Reply<Vec<u8>>),
    RecoverDeliveryReceipt(
        kami_e2ee::ReplayPermit,
        Vec<u8>,
        Reply<Option<MessageFacts>>,
    ),
    CommitDelivery(
        Box<kami_e2ee::AuthenticatedDelivery>,
        Reply<crate::DurableDelivery>,
    ),
    RecoverDelivery(
        kami_e2ee::ReplayPermit,
        Vec<u8>,
        Reply<Option<crate::DurableDelivery>>,
    ),
    CommitDeliveryReceipt(
        kami_e2ee::AuthenticatedReceiptCommit,
        Reply<crate::Acceptance>,
    ),
    SenderRecord(DeviceId, MessageId, Reply<crate::QueuedSend>),
    HoldSend(DeviceId, MessageId, Reply<()>),
    ResumeSend(DeviceId, MessageId, u64, i64, Reply<()>),
    RecoverSends(
        Option<(ConversationId, DeviceId, kami_core::SenderSequence)>,
        usize,
        Reply<Vec<crate::QueuedSend>>,
    ),
    CancelSend(DeviceId, MessageId, Reply<()>),
    ClaimSend(DeviceId, MessageId, i64, Reply<Option<crate::SendLease>>),
    FailSend(crate::SendLease, kami_core::AttemptFailure, i64, Reply<()>),
    CommitPrepared(
        crate::PreparedSend,
        kami_e2ee::AuthenticatedCommit,
        i64,
        Reply<Vec<u8>>,
    ),
    CleanupPrepared(i64, Reply<usize>),
    PrepareSend(crate::SendRequest, i64, Reply<crate::Preparation>),
    Authenticated(kami_e2ee::AuthenticatedCommit, Reply<Vec<u8>>),
    #[allow(dead_code)]
    E2eeLoad(
        [u8; 32],
        Reply<(
            Option<crate::session_state::AccountRow>,
            Option<crate::session_state::SessionRow>,
        )>,
    ),
    #[allow(dead_code)]
    E2eeAccount(
        Option<crate::session_state::AccountRow>,
        crate::session_state::AccountRow,
        Reply<()>,
    ),
    #[allow(dead_code)]
    E2eeCommit(Box<crate::session_state::Commit>, Reply<crate::Acceptance>),
    #[allow(dead_code)]
    E2eeReconcile(
        crate::session_state::SessionRow,
        MessageFacts,
        Vec<u8>,
        bool,
        Reply<Option<Vec<u8>>>,
    ),
    LoadContact([u8; 32], Reply<Option<crate::ContactTrustRow>>),
    CasContact(Option<u64>, crate::ContactTrustRow, Reply<()>),
    #[cfg(test)]
    Pause(mpsc::SyncSender<()>, mpsc::Receiver<()>),
    Cipher(Reply<String>),
    NextSequence(ConversationId, DeviceId, Reply<kami_core::SenderSequence>),
    Enqueue(crate::StoredMessage, Reply<()>),
    Pending(usize, Reply<Vec<crate::StoredMessage>>),
    Accept(crate::StoredMessage, i64, Reply<crate::Acceptance>),
    Receipt(DeviceId, MessageId, Reply<Option<MessageFacts>>),
    DeleteHistory(DeviceId, MessageId, Reply<()>),
    History(ConversationId, usize, Reply<Vec<crate::StoredMessage>>),
    Cancel(DeviceId, MessageId, Reply<()>),
    ApplyReceipt(MessageFacts, DeviceId, Reply<()>),
    Ordered(
        ConversationId,
        DeviceId,
        i64,
        i64,
        usize,
        Reply<Vec<crate::OrderedMessage>>,
    ),
}
/// Blocking facade. Use a blocking service task, never a networking/async executor task.
/// All commands own their input and return only after commit; no transaction escapes.
pub struct Storage {
    sender: Option<mpsc::SyncSender<Request>>,
    thread: Option<JoinHandle<()>>,
}
impl Storage {
    /// Typed prekey-only durable transition. Supplied fences are not provenance.
    pub fn prekey(
        &self,
        operation: crate::prekeys::PrekeyOperation,
    ) -> Result<crate::prekeys::PrekeyOutcome, StorageError> {
        self.call(|reply| Request::Prekey(Box::new(operation), reply))
    }
    /// Untrusted bounded evidence; the native identity anchor remains authority.
    pub fn enrollment_snapshot(&self) -> Result<Option<Vec<u8>>, StorageError> {
        self.call(Request::ReadEnrollment)
    }
    /// Insert once, only on the worker that created this new database. Never a
    /// migration/repair/adoption API, and never sufficient for native readiness.
    pub fn commit_fresh_enrollment(&self, row: &[u8]) -> Result<Vec<u8>, StorageError> {
        if row.len() != 66 {
            return Err(StorageError::InvalidInput);
        }
        self.call(|reply| Request::CommitEnrollment(row.to_vec(), reply))
    }
    pub fn recover_delivery_receipt(
        &self,
        permit: kami_e2ee::ReplayPermit,
        frame: Vec<u8>,
    ) -> Result<Option<MessageFacts>, StorageError> {
        if frame.is_empty() || frame.len() > 65_536 {
            return Err(StorageError::InvalidInput);
        }
        self.call(|reply| Request::RecoverDeliveryReceipt(permit, frame, reply))
    }
    pub fn commit_delivery(
        &self,
        stage: kami_e2ee::AuthenticatedDelivery,
    ) -> Result<crate::DurableDelivery, StorageError> {
        self.call(|reply| Request::CommitDelivery(Box::new(stage), reply))
    }
    pub fn recover_delivery(
        &self,
        permit: kami_e2ee::ReplayPermit,
        frame: Vec<u8>,
    ) -> Result<Option<crate::DurableDelivery>, StorageError> {
        if frame.is_empty() || frame.len() > 65_536 {
            return Err(StorageError::InvalidInput);
        }
        self.call(|reply| Request::RecoverDelivery(permit, frame, reply))
    }
    pub fn commit_delivery_receipt(
        &self,
        stage: kami_e2ee::AuthenticatedReceiptCommit,
    ) -> Result<crate::Acceptance, StorageError> {
        self.call(|reply| Request::CommitDeliveryReceipt(stage, reply))
    }
    pub fn sender_record(
        &self,
        sender: DeviceId,
        id: MessageId,
    ) -> Result<crate::QueuedSend, StorageError> {
        self.call(|reply| Request::SenderRecord(sender, id, reply))
    }
    pub fn hold_send(&self, sender: DeviceId, id: MessageId) -> Result<(), StorageError> {
        self.call(|reply| Request::HoldSend(sender, id, reply))
    }
    /// Explicit service reverification of the ORIGINAL pin, never replacement adoption.
    pub fn resume_send(
        &self,
        sender: DeviceId,
        id: MessageId,
        generation: u64,
        now_ms: i64,
    ) -> Result<(), StorageError> {
        self.call(|reply| Request::ResumeSend(sender, id, generation, now_ms, reply))
    }
    /// Bounded ordered recovery projection, not authorization to transmit.
    pub fn recover_sends(&self, limit: usize) -> Result<Vec<crate::QueuedSend>, StorageError> {
        self.recover_sends_after(None, limit)
    }
    /// Continue strictly after the last returned conversation/sender/sequence key.
    pub fn recover_sends_after(
        &self,
        after: Option<(ConversationId, DeviceId, kami_core::SenderSequence)>,
        limit: usize,
    ) -> Result<Vec<crate::QueuedSend>, StorageError> {
        self.call(|reply| Request::RecoverSends(after, limit, reply))
    }
    /// Stop future attempts without discarding authenticated immutable material.
    pub fn cancel_send(&self, sender: DeviceId, id: MessageId) -> Result<(), StorageError> {
        self.call(|reply| Request::CancelSend(sender, id, reply))
    }
    /// Lease exact persisted bytes; stale leases are re-fenced, not counted again.
    pub fn claim_send(
        &self,
        sender: DeviceId,
        id: MessageId,
        now_ms: i64,
    ) -> Result<Option<crate::SendLease>, StorageError> {
        self.call(|reply| Request::ClaimSend(sender, id, now_ms, reply))
    }
    pub fn fail_send(&self, lease: crate::SendLease, now_ms: i64) -> Result<(), StorageError> {
        self.record_send_failure(lease, kami_core::AttemptFailure::Unavailable, now_ms)
    }
    pub fn record_send_failure(
        &self,
        lease: crate::SendLease,
        reason: kami_core::AttemptFailure,
        now_ms: i64,
    ) -> Result<(), StorageError> {
        self.call(|reply| Request::FailSend(lease, reason, now_ms, reply))
    }
    /// Atomically consume a reservation and the opaque authenticated stage.
    pub fn commit_prepared(
        &self,
        reservation: crate::PreparedSend,
        stage: kami_e2ee::AuthenticatedCommit,
        now_ms: i64,
    ) -> Result<Vec<u8>, StorageError> {
        self.call(|reply| Request::CommitPrepared(reservation, stage, now_ms, reply))
    }
    /// Expire only uncommitted reservations; retain bounded request tombstones.
    pub fn cleanup_prepared(&self, now_ms: i64) -> Result<usize, StorageError> {
        self.call(|reply| Request::CleanupPrepared(now_ms, reply))
    }
    /// Reserve one durable lane before crypto preparation. No plaintext is persisted.
    pub fn prepare_send(
        &self,
        request: crate::SendRequest,
        now_ms: i64,
    ) -> Result<crate::Preparation, StorageError> {
        self.call(|reply| Request::PrepareSend(request, now_ms, reply))
    }
    /// Quarantined text-only stage commit/reconciliation on the SQLCipher worker.
    /// Returned exact bytes prove persistence, not live provenance or ACK authority.
    /// No receipt ratchet, Delivered transition, network send or profile restore.
    pub fn commit_authenticated(
        &self,
        stage: kami_e2ee::AuthenticatedCommit,
    ) -> Result<Vec<u8>, StorageError> {
        self.call(|reply| Request::Authenticated(stage, reply))
    }
    /// Load one bounded neutral trust row. The service must revalidate its
    /// cryptographic contents; a stored Verified tag alone is not authentication.
    pub fn load_contact(
        &self,
        contact: [u8; 32],
    ) -> Result<Option<crate::ContactTrustRow>, StorageError> {
        self.call(|reply| Request::LoadContact(contact, reply))
    }
    /// Atomic durable insert/CAS on the existing encrypted worker. None inserts
    /// generation 1; Some(n) requires n+1. Service owns trust-transition policy.
    pub fn compare_and_swap_contact(
        &self,
        expected: Option<u64>,
        next: crate::ContactTrustRow,
    ) -> Result<(), StorageError> {
        self.call(|reply| Request::CasContact(expected, next, reply))
    }
    pub fn create(
        lease: ProfileLease,
        profile: ProfileKeyId,
        provider: Arc<dyn KeyProvider>,
        config: Config,
    ) -> Result<Self, StorageError> {
        Self::start(lease, profile, provider, config, true)
    }
    pub fn open(
        lease: ProfileLease,
        profile: ProfileKeyId,
        provider: Arc<dyn KeyProvider>,
        config: Config,
    ) -> Result<Self, StorageError> {
        Self::start(lease, profile, provider, config, false)
    }
    fn start(
        lease: ProfileLease,
        profile: ProfileKeyId,
        provider: Arc<dyn KeyProvider>,
        config: Config,
        create: bool,
    ) -> Result<Self, StorageError> {
        if config.queue_capacity == 0
            || config.queue_capacity > 1024
            || config.max_database_pages < 16
        {
            return Err(StorageError::InvalidInput);
        }
        let (sender, receiver) = mpsc::sync_channel(config.queue_capacity);
        let (ready, status) = mpsc::sync_channel(1);
        let thread = thread::Builder::new()
            .name("kami-storage".into())
            .spawn(move || {
                let mut db = match open::connect(
                    &lease,
                    &profile,
                    &*provider,
                    create,
                    config.max_database_pages,
                ) {
                    Ok(db) => db,
                    Err(e) => {
                        let _ = ready.send(Err(e));
                        return;
                    }
                };
                let _ = ready.send(Ok(()));
                while let Ok(request) = receiver.recv() {
                    match request {
                        Request::Prekey(operation, reply) => {
                            let _ = reply.send(crate::prekeys::execute(&mut db, *operation));
                        }
                        Request::ReadEnrollment(reply) => {
                            let _ = reply.send(crate::enrollment::read(&db, &profile));
                        }
                        Request::CommitEnrollment(row, reply) => {
                            let _ = reply
                                .send(crate::enrollment::commit(&mut db, &profile, &row, create));
                        }
                        Request::RecoverDeliveryReceipt(permit, frame, reply) => {
                            let _ = reply
                                .send(crate::receiver_state::recover_receipt(&db, permit, frame));
                        }
                        Request::CommitDelivery(stage, reply) => {
                            let _ = reply.send(crate::receiver_state::commit(
                                &mut db,
                                *stage,
                                config.outbox_bytes,
                                config.outbox_messages,
                            ));
                        }
                        Request::RecoverDelivery(permit, frame, reply) => {
                            let _ = reply.send(crate::receiver_state::recover(&db, permit, frame));
                        }
                        Request::CommitDeliveryReceipt(stage, reply) => {
                            let _ = reply.send(crate::receiver_state::consume(
                                &mut db,
                                stage,
                                config.outbox_bytes,
                            ));
                        }
                        Request::SenderRecord(sender, id, reply) => {
                            let _ = reply.send(crate::sender_state::load(&db, sender, id));
                        }
                        Request::HoldSend(sender, id, reply) => {
                            let _ = reply.send(crate::sender_state::hold(&mut db, sender, id));
                        }
                        Request::ResumeSend(sender, id, generation, now, reply) => {
                            let _ = reply.send(crate::sender_state::resume(
                                &mut db, sender, id, generation, now,
                            ));
                        }
                        Request::RecoverSends(after, limit, reply) => {
                            let _ = reply.send(crate::sender_state::recover(&db, after, limit));
                        }
                        Request::CancelSend(sender, id, reply) => {
                            let _ = reply.send(crate::sender_state::cancel(&mut db, sender, id));
                        }
                        Request::ClaimSend(sender, id, now, reply) => {
                            let _ =
                                reply.send(crate::sender_state::claim(&mut db, sender, id, now));
                        }
                        Request::FailSend(lease, reason, now, reply) => {
                            let _ =
                                reply.send(crate::sender_state::fail(&mut db, lease, reason, now));
                        }
                        Request::CommitPrepared(reservation, stage, now, reply) => {
                            let _ = reply.send(crate::session_state::commit_prepared(
                                &mut db,
                                stage,
                                config.outbox_bytes,
                                config.outbox_messages,
                                Some((&reservation, now)),
                            ));
                        }
                        Request::CleanupPrepared(now, reply) => {
                            let _ = reply.send(crate::sender_state::cleanup(&mut db, now));
                        }
                        Request::PrepareSend(request, now, reply) => {
                            let _ = reply.send(crate::sender_state::prepare(
                                &mut db,
                                request,
                                now,
                                config.outbox_messages,
                            ));
                        }
                        Request::Authenticated(stage, reply) => {
                            let _ = reply.send(crate::session_state::commit_authenticated(
                                &mut db,
                                stage,
                                config.outbox_bytes,
                                config.outbox_messages,
                            ));
                        }
                        Request::E2eeLoad(id, reply) => {
                            let result =
                                crate::session_state::load_account(&db).and_then(|account| {
                                    crate::session_state::load_session(&db, id)
                                        .map(|session| (account, session))
                                });
                            let _ = reply.send(result);
                        }
                        Request::E2eeAccount(previous, next, reply) => {
                            let _ = reply.send(crate::session_state::commit_account(
                                &mut db, previous, next,
                            ));
                        }
                        Request::E2eeCommit(command, reply) => {
                            let _ = reply.send(crate::session_state::commit(
                                &mut db,
                                *command,
                                config.outbox_bytes,
                                config.outbox_messages,
                            ));
                        }
                        Request::E2eeReconcile(session, facts, frame, incoming, reply) => {
                            let _ = reply.send(crate::session_state::reconcile(
                                &db, &session, facts, &frame, incoming,
                            ));
                        }
                        Request::LoadContact(contact, reply) => {
                            let _ = reply.send(crate::contact_trust::load(&db, contact));
                        }
                        Request::CasContact(expected, next, reply) => {
                            let _ = reply.send(crate::contact_trust::cas(&mut db, expected, next));
                        }
                        Request::NextSequence(c, s, reply) => {
                            let _ = reply.send(crate::transactions::next_sequence(&db, c, s));
                        }
                        #[cfg(test)]
                        Request::Pause(entered, hold) => {
                            let _ = entered.send(());
                            let _ = hold.recv();
                        }
                        Request::Cancel(sender, id, reply) => {
                            let _ = reply.send(
                                crate::session_state::legacy_guard(&db, sender, id)
                                    .and_then(|()| crate::outbox::cancel(&mut db, sender, id)),
                            );
                        }
                        Request::ApplyReceipt(f, device, reply) => {
                            let result =
                                crate::session_state::legacy_guard(&db, f.sender(), f.message_id())
                                    .and_then(|()| {
                                        crate::outbox::apply_receipt(&mut db, f, device)
                                    });
                            let _ = reply.send(result);
                        }
                        Request::Ordered(c, s, now, wait, limit, reply) => {
                            let _ = reply
                                .send(crate::ordering::history(&mut db, c, s, now, wait, limit));
                        }
                        Request::Accept(msg, at, reply) => {
                            let f = msg.facts();
                            let result =
                                crate::session_state::legacy_guard(&db, f.sender(), f.message_id())
                                    .and_then(|()| crate::inbox::accept(&mut db, msg, at));
                            let _ = reply.send(result);
                        }
                        Request::Receipt(sender, id, reply) => {
                            let _ = reply.send(
                                crate::session_state::legacy_guard(&db, sender, id)
                                    .and_then(|()| crate::inbox::receipt(&db, sender, id)),
                            );
                        }
                        Request::DeleteHistory(sender, id, reply) => {
                            let _ = reply.send(crate::inbox::delete_history(&mut db, sender, id));
                        }
                        Request::History(conversation, limit, reply) => {
                            let _ = reply.send(crate::inbox::history(&db, conversation, limit));
                        }
                        Request::Enqueue(msg, reply) => {
                            let f = msg.facts();
                            let _ = reply.send(
                                crate::session_state::legacy_guard(&db, f.sender(), f.message_id())
                                    .and_then(|()| {
                                        crate::transactions::enqueue(
                                            &mut db,
                                            msg,
                                            config.outbox_bytes,
                                            config.outbox_messages,
                                        )
                                    }),
                            );
                        }
                        Request::Pending(limit, reply) => {
                            let _ = reply.send(crate::transactions::pending(&db, limit));
                        }
                        Request::Cipher(reply) => {
                            let _ = reply.send(
                                db.pragma_query_value(None, "cipher_version", |r| r.get(0))
                                    .map_err(Into::into),
                            );
                        }
                    }
                }
                // Closing SQLite before releasing the lease prevents a second writer during close.
                drop(db);
                drop(lease);
            })
            .map_err(|_| StorageError::Io)?;
        match status.recv().map_err(|_| StorageError::Closed)? {
            Ok(()) => Ok(Self {
                sender: Some(sender),
                thread: Some(thread),
            }),
            Err(e) => {
                let _ = thread.join();
                Err(e)
            }
        }
    }
    /// New or duplicate ACK eligibility is returned only after commit. Caller owns authentication.
    pub fn accept(
        &self,
        msg: crate::StoredMessage,
        received_at_ms: i64,
    ) -> Result<crate::Acceptance, StorageError> {
        self.call(|r| Request::Accept(msg, received_at_ms, r))
    }
    pub fn receipt(
        &self,
        sender: DeviceId,
        id: MessageId,
    ) -> Result<Option<MessageFacts>, StorageError> {
        self.call(|r| Request::Receipt(sender, id, r))
    }
    pub fn delete_history(&self, sender: DeviceId, id: MessageId) -> Result<(), StorageError> {
        self.call(|r| Request::DeleteHistory(sender, id, r))
    }
    pub fn history(
        &self,
        conversation: ConversationId,
        limit: usize,
    ) -> Result<Vec<crate::StoredMessage>, StorageError> {
        self.call(|r| Request::History(conversation, limit, r))
    }
    pub fn cancel(&self, sender: DeviceId, id: MessageId) -> Result<(), StorageError> {
        self.call(|r| Request::Cancel(sender, id, r))
    }
    /// Trusted service supplies an authenticated DEVICE_DELIVERED assertion, never socket/relay success.
    pub fn apply_authenticated_receipt(
        &self,
        facts: MessageFacts,
        authenticated_device: DeviceId,
    ) -> Result<(), StorageError> {
        self.call(|r| Request::ApplyReceipt(facts, authenticated_device, r))
    }
    pub fn ordered_history(
        &self,
        conversation: ConversationId,
        sender: DeviceId,
        now_ms: i64,
        gap_wait_ms: i64,
        limit: usize,
    ) -> Result<Vec<crate::OrderedMessage>, StorageError> {
        self.call(|r| Request::Ordered(conversation, sender, now_ms, gap_wait_ms, limit, r))
    }
    /// Supply the next sequence. Allocation is checked and committed atomically with immutable material.
    pub fn enqueue(&self, msg: crate::StoredMessage) -> Result<(), StorageError> {
        self.call(|r| Request::Enqueue(msg, r))
    }
    pub fn pending(&self, limit: usize) -> Result<Vec<crate::StoredMessage>, StorageError> {
        self.call(|r| Request::Pending(limit, r))
    }
    /// A proposal, not a reservation: enqueue atomically checks this next value. Serialize crypto/session preparation in the service.
    pub fn next_sender_sequence(
        &self,
        conversation: ConversationId,
        sender: DeviceId,
    ) -> Result<kami_core::SenderSequence, StorageError> {
        self.call(|r| Request::NextSequence(conversation, sender, r))
    }
    pub fn cipher_version(&self) -> Result<String, StorageError> {
        self.call(Request::Cipher)
    }
    fn call<T>(&self, request: impl FnOnce(Reply<T>) -> Request) -> Result<T, StorageError> {
        let (reply, receiver) = mpsc::sync_channel(1);
        self.sender
            .as_ref()
            .ok_or(StorageError::Closed)?
            .try_send(request(reply))
            .map_err(|e| match e {
                mpsc::TrySendError::Full(_) => StorageError::QueueFull,
                mpsc::TrySendError::Disconnected(_) => StorageError::Closed,
            })?;
        receiver.recv().map_err(|_| StorageError::Closed)?
    }
    pub fn shutdown(mut self) -> Result<(), StorageError> {
        self.sender.take();
        self.thread
            .take()
            .ok_or(StorageError::Closed)?
            .join()
            .map_err(|_| StorageError::Closed)
    }
}
impl Drop for Storage {
    fn drop(&mut self) {
        self.sender.take();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
