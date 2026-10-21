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
                            let _ = reply.send(crate::outbox::cancel(&mut db, sender, id));
                        }
                        Request::ApplyReceipt(f, device, reply) => {
                            let _ = reply.send(crate::outbox::apply_receipt(&mut db, f, device));
                        }
                        Request::Ordered(c, s, now, wait, limit, reply) => {
                            let _ = reply
                                .send(crate::ordering::history(&mut db, c, s, now, wait, limit));
                        }
                        Request::Accept(msg, at, reply) => {
                            let _ = reply.send(crate::inbox::accept(&mut db, msg, at));
                        }
                        Request::Receipt(sender, id, reply) => {
                            let _ = reply.send(crate::inbox::receipt(&db, sender, id));
                        }
                        Request::DeleteHistory(sender, id, reply) => {
                            let _ = reply.send(crate::inbox::delete_history(&mut db, sender, id));
                        }
                        Request::History(conversation, limit, reply) => {
                            let _ = reply.send(crate::inbox::history(&db, conversation, limit));
                        }
                        Request::Enqueue(msg, reply) => {
                            let _ = reply.send(crate::transactions::enqueue(
                                &mut db,
                                msg,
                                config.outbox_bytes,
                                config.outbox_messages,
                            ));
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
