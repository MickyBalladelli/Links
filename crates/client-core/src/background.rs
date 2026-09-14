//! Background replay and decrypt orchestration for native and web hosts.
//!
//! A host supplies the authenticated TLS transport and a local durable inbox.
//! The worker owns the ordering boundary: it validates contiguous server
//! batches, decrypts envelopes through `ClientCore`, commits local data, and
//! only then acknowledges the mailbox cursor.

use crate::{
    crypto::EnvelopeCrypto,
    envelopes::ClientCore,
    mls::MlsEngine,
    protocol::v1,
    receive::{SFrameKeyHandler, RejectSFrameKeyHandler},
    sframe::SFrameEpochKeyUpdate,
    sync::{SyncAdvance, SyncState},
    CoreError,
};
use async_trait::async_trait;

pub const DEFAULT_REPLAY_LIMIT: u32 = 100;

/// Adapter for the authenticated `wss://` connection. The implementation must
/// use TLS, send Hello with `last_seen_cursor`, and preserve the exact envelope
/// bytes returned by the gateway until the worker commits them locally.
#[async_trait]
pub trait BackgroundTransport: Send {
    async fn connect(
        &mut self,
        device_id: &str,
        last_seen_cursor: u64,
        limit: u32,
    ) -> Result<v1::SyncBatch, CoreError>;

    async fn replay(&mut self, after_cursor: u64, limit: u32) -> Result<v1::SyncBatch, CoreError>;

    /// Send cumulative QueueAck only after `InboxStore::commit_batch` returns.
    async fn acknowledge(&mut self, ack: v1::QueueAck) -> Result<(), CoreError>;
}

/// One locally processed queue item. The store must persist all items and the
/// checkpoint atomically with its message-ID deduplication and MLS provider
/// state, where the host's provider supports that transaction boundary.
pub enum DecryptedSyncItem {
    Message { cursor: u64, message: v1::Message },
    Tombstone { cursor: u64, reason: i32 },
}

#[async_trait]
pub trait InboxStore: Send {
    /// Persist the decrypted items, tombstones, and `advance` in one durable
    /// local transaction. Do not return success for an in-memory-only write.
    async fn commit_batch(
        &mut self,
        advance: &SyncAdvance,
        items: &[DecryptedSyncItem],
    ) -> Result<(), CoreError>;
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct SyncRun {
    pub batches: usize,
    pub messages: usize,
    pub tombstones: usize,
    pub cursor: u64,
}

pub struct BackgroundWorker<C, M, T, I> {
    core: ClientCore<C, M>,
    sync: SyncState,
    transport: T,
    inbox: I,
    replay_limit: u32,
}

impl<C, M, T, I> BackgroundWorker<C, M, T, I> {
    pub fn new(core: ClientCore<C, M>, sync: SyncState, transport: T, inbox: I) -> Self {
        Self {
            core,
            sync,
            transport,
            inbox,
            replay_limit: DEFAULT_REPLAY_LIMIT,
        }
    }

    pub fn with_replay_limit(mut self, replay_limit: u32) -> Result<Self, CoreError> {
        if replay_limit == 0 || replay_limit as usize > crate::protocol::MAX_BATCH_ITEMS {
            return Err(CoreError::InvalidSync);
        }
        self.replay_limit = replay_limit;
        Ok(self)
    }

    pub fn cursor(&self) -> u64 {
        self.sync.cursor()
    }

    /// Drain the mailbox visible to this wakeup. A push is only a wakeup hint;
    /// the worker fetches the authoritative encrypted payloads by cursor.
    pub async fn run_once(&mut self, now_ms: u64) -> Result<SyncRun, CoreError>
    where
        C: EnvelopeCrypto,
        M: MlsEngine,
        T: BackgroundTransport,
        I: InboxStore,
    {
        let mut handler = RejectSFrameKeyHandler;
        self.run_once_with_sframe(&mut handler, now_ms).await
    }

    /// Drain the mailbox and install MLS-authenticated SFrame controls before
    /// committing the cursor. The handler must authorize and durably install
    /// each key, and must accept an exact replay idempotently.
    pub async fn run_once_with_sframe<H>(
        &mut self,
        handler: &mut H,
        now_ms: u64,
    ) -> Result<SyncRun, CoreError>
    where
        C: EnvelopeCrypto,
        M: MlsEngine,
        T: BackgroundTransport,
        I: InboxStore,
        H: SFrameKeyHandler,
    {
        let device_id = self.sync.device_id().to_owned();
        let mut batch = self
            .transport
            .connect(&device_id, self.sync.cursor(), self.replay_limit)
            .await?;
        let mut run = SyncRun {
            cursor: self.sync.cursor(),
            ..SyncRun::default()
        };

        loop {
            let high_watermark = batch.high_watermark;
            let progress = self.process_batch(batch, handler, now_ms).await?;
            run.batches += 1;
            run.messages += progress.messages;
            run.tombstones += progress.tombstones;
            run.cursor = self.sync.cursor();
            if progress.idle || run.cursor >= high_watermark {
                return Ok(run);
            }
            batch = self
                .transport
                .replay(self.sync.cursor(), self.replay_limit)
                .await?;
        }
    }

    async fn process_batch(
        &mut self,
        batch: v1::SyncBatch,
        handler: &mut impl SFrameKeyHandler,
        now_ms: u64,
    ) -> Result<BatchProgress, CoreError>
    where
        C: EnvelopeCrypto,
        M: MlsEngine,
        T: BackgroundTransport,
        I: InboxStore,
    {
        let advance = self.sync.prepare(&batch)?;
        if batch.items.is_empty() {
            return Ok(BatchProgress {
                idle: true,
                messages: 0,
                tombstones: 0,
            });
        }

        let mut items = Vec::with_capacity(batch.items.len());
        let mut messages = 0;
        let mut tombstones = 0;
        for item in batch.items {
            match item.entry.ok_or(CoreError::InvalidSync)? {
                v1::queue_item::Entry::Envelope(envelope) => {
                    let message = self.core.open_envelope(&envelope, now_ms)?;
                    if let Some(update) = SFrameEpochKeyUpdate::from_message(&message)? {
                        handler
                            .handle_sframe_epoch_key(
                                &message.conversation_id,
                                &message.sender_device_id,
                                &update,
                            )
                            .await?;
                        continue;
                    }
                    items.push(DecryptedSyncItem::Message {
                        cursor: item.cursor,
                        message,
                    });
                    messages += 1;
                }
                v1::queue_item::Entry::Tombstone(tombstone) => {
                    items.push(DecryptedSyncItem::Tombstone {
                        cursor: item.cursor,
                        reason: tombstone.reason,
                    });
                    tombstones += 1;
                }
            }
        }

        self.inbox.commit_batch(&advance, &items).await?;
        let ack = self.sync.commit(advance)?;
        self.transport.acknowledge(ack).await?;
        Ok(BatchProgress {
            idle: false,
            messages,
            tombstones,
        })
    }
}

struct BatchProgress {
    idle: bool,
    messages: usize,
    tombstones: usize,
}
