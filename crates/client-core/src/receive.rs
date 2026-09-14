//! Authenticated MLS receive orchestration.
//!
//! The normal queue contains opaque envelopes. MLS Welcome/Commit messages
//! arrive through an authenticated client transport adapter. SFrame key
//! updates use private `MlsControl` content inside an MLS application message
//! and never become transport-visible control frames.

use crate::{
    background::DecryptedSyncItem,
    broadcast::{verify_post, BroadcastAdminVerifier, BroadcastMasterKey, BroadcastSubscriber},
    crypto::EnvelopeCrypto,
    envelopes::ClientCore,
    mls::MlsEngine,
    protocol::{self, v1},
    sframe::SFrameEpochKeyUpdate,
    sync::{SyncAdvance, SyncState},
    CoreError,
};
use async_trait::async_trait;
use std::collections::HashSet;

#[derive(Clone, PartialEq, Eq)]
pub enum MlsEpochUpdate {
    Welcome {
        conversation_id: String,
        bytes: Vec<u8>,
    },
    Commit {
        conversation_id: String,
        bytes: Vec<u8>,
    },
    GroupWelcome {
        conversation_id: String,
        bytes: Vec<u8>,
    },
    GroupCommit {
        conversation_id: String,
        bytes: Vec<u8>,
    },
    BroadcastWelcome {
        conversation_id: String,
        bytes: Vec<u8>,
    },
    BroadcastCommit {
        conversation_id: String,
        bytes: Vec<u8>,
    },
}

impl MlsEpochUpdate {
    pub fn welcome(conversation_id: String, bytes: Vec<u8>) -> Result<Self, CoreError> {
        Self::new(conversation_id, bytes).map(|(conversation_id, bytes)| Self::Welcome {
            conversation_id,
            bytes,
        })
    }

    pub fn commit(conversation_id: String, bytes: Vec<u8>) -> Result<Self, CoreError> {
        Self::new(conversation_id, bytes).map(|(conversation_id, bytes)| Self::Commit {
            conversation_id,
            bytes,
        })
    }

    pub fn group_welcome(conversation_id: String, bytes: Vec<u8>) -> Result<Self, CoreError> {
        Self::new(conversation_id, bytes).map(|(conversation_id, bytes)| Self::GroupWelcome {
            conversation_id,
            bytes,
        })
    }

    pub fn group_commit(conversation_id: String, bytes: Vec<u8>) -> Result<Self, CoreError> {
        Self::new(conversation_id, bytes).map(|(conversation_id, bytes)| Self::GroupCommit {
            conversation_id,
            bytes,
        })
    }

    pub fn broadcast_welcome(conversation_id: String, bytes: Vec<u8>) -> Result<Self, CoreError> {
        Self::new(conversation_id, bytes).map(|(conversation_id, bytes)| Self::BroadcastWelcome {
            conversation_id,
            bytes,
        })
    }

    pub fn broadcast_commit(conversation_id: String, bytes: Vec<u8>) -> Result<Self, CoreError> {
        Self::new(conversation_id, bytes).map(|(conversation_id, bytes)| Self::BroadcastCommit {
            conversation_id,
            bytes,
        })
    }

    pub fn conversation_id(&self) -> &str {
        match self {
            Self::Welcome {
                conversation_id, ..
            }
            | Self::Commit {
                conversation_id, ..
            }
            | Self::GroupWelcome {
                conversation_id, ..
            }
            | Self::GroupCommit {
                conversation_id, ..
            }
            | Self::BroadcastWelcome {
                conversation_id, ..
            }
            | Self::BroadcastCommit {
                conversation_id, ..
            } => conversation_id,
        }
    }

    pub fn is_group(&self) -> bool {
        matches!(
            self,
            Self::GroupWelcome { .. }
                | Self::GroupCommit { .. }
                | Self::BroadcastWelcome { .. }
                | Self::BroadcastCommit { .. }
        )
    }

    pub fn is_broadcast(&self) -> bool {
        matches!(
            self,
            Self::BroadcastWelcome { .. } | Self::BroadcastCommit { .. }
        )
    }

    fn new(conversation_id: String, bytes: Vec<u8>) -> Result<(String, Vec<u8>), CoreError> {
        protocol::validate_id(&conversation_id)?;
        if bytes.is_empty() {
            return Err(CoreError::Authentication);
        }
        if bytes.len() > protocol::MAX_FRAME_BYTES {
            return Err(CoreError::Protocol(protocol::ProtocolError::TooLarge));
        }
        Ok((conversation_id, bytes))
    }

    fn apply<M: MlsEngine>(&self, mls: &mut M) -> Result<(), CoreError> {
        match self {
            Self::Welcome {
                conversation_id,
                bytes,
            } => mls.join_direct_group(conversation_id, bytes),
            Self::GroupWelcome {
                conversation_id,
                bytes,
            } => mls.join_group(conversation_id, bytes),
            Self::Commit {
                conversation_id,
                bytes,
            } => mls.process_direct_commit(conversation_id, bytes),
            Self::GroupCommit {
                conversation_id,
                bytes,
            } => mls.process_commit(conversation_id, bytes),
            Self::BroadcastWelcome {
                conversation_id,
                bytes,
            } => mls.join_group(conversation_id, bytes),
            Self::BroadcastCommit {
                conversation_id,
                bytes,
            } => mls.process_commit(conversation_id, bytes),
        }
    }
}

/// A replay batch plus the MLS updates that must be applied before opening its
/// envelopes. The adapter must authenticate and order these updates.
pub struct DirectReceiveBatch {
    pub batch: v1::SyncBatch,
    pub mls_updates: Vec<MlsEpochUpdate>,
}

/// Group receive uses the same replay envelope, but its transport adapter must
/// supply `GroupWelcome`/`GroupCommit` updates for the group's MLS state.
pub type GroupReceiveBatch = DirectReceiveBatch;

#[async_trait]
pub trait DirectReceiveTransport: Send {
    async fn connect(
        &mut self,
        device_id: &str,
        last_seen_cursor: u64,
        limit: u32,
    ) -> Result<DirectReceiveBatch, CoreError>;

    async fn replay(
        &mut self,
        after_cursor: u64,
        limit: u32,
    ) -> Result<DirectReceiveBatch, CoreError>;

    /// This is the server-side mailbox acknowledgement, not the private
    /// E2EE Receipts message returned by `receive_available`.
    async fn acknowledge(&mut self, ack: v1::QueueAck) -> Result<(), CoreError>;
}

pub trait GroupReceiveTransport: DirectReceiveTransport {
    fn validate_group_batch(&self, batch: &GroupReceiveBatch) -> Result<(), CoreError> {
        if batch.mls_updates.iter().any(|update| !update.is_group()) {
            return Err(CoreError::Authentication);
        }
        Ok(())
    }
}

impl<T> GroupReceiveTransport for T where T: DirectReceiveTransport + ?Sized {}

#[async_trait]
pub trait DirectReceiveStore: Send {
    /// Persist MLS state, received messages/tombstones, and the cursor in one
    /// local transaction. Do not acknowledge a batch before this succeeds.
    async fn commit_batch(
        &mut self,
        advance: &SyncAdvance,
        mls_updates: &[MlsEpochUpdate],
        items: &[DecryptedSyncItem],
    ) -> Result<(), CoreError>;
}

pub trait GroupReceiveStore: DirectReceiveStore {}

impl<T> GroupReceiveStore for T where T: DirectReceiveStore + ?Sized {}

pub trait BroadcastReceiveTransport: DirectReceiveTransport {
    fn validate_broadcast_batch(&self, batch: &GroupReceiveBatch) -> Result<(), CoreError> {
        if batch
            .mls_updates
            .iter()
            .any(|update| !update.is_broadcast())
        {
            return Err(CoreError::Authentication);
        }
        Ok(())
    }
}

impl<T> BroadcastReceiveTransport for T where T: DirectReceiveTransport + ?Sized {}

/// Broker consumer boundary for broadcast dispatches. The adapter must fetch
/// only the requested conversation and acknowledge only after the caller has
/// rendered the verified message. Re-delivery after a failed ack is expected.
#[async_trait]
pub trait BroadcastDispatchTransport: Send {
    async fn fetch_broadcast(
        &mut self,
        conversation_id: &str,
        limit: u32,
    ) -> Result<Vec<v1::BroadcastDispatch>, CoreError>;

    async fn acknowledge_broadcast(
        &mut self,
        dispatch: &v1::BroadcastDispatch,
    ) -> Result<(), CoreError>;
}

#[async_trait]
pub trait MessageRenderer: Send {
    async fn render(&mut self, message: &v1::Message) -> Result<(), CoreError>;
}

/// Receives MLS-authenticated SFrame keys. Implementations must authorize the
/// sender for the media session and durably install the key before returning.
/// Replaying the same update must be idempotent because cursor commit follows
/// this callback.
#[async_trait]
pub trait SFrameKeyHandler: Send {
    async fn handle_sframe_epoch_key(
        &mut self,
        conversation_id: &str,
        sender_device_id: &str,
        update: &SFrameEpochKeyUpdate,
    ) -> Result<(), CoreError>;
}

pub(crate) struct RejectSFrameKeyHandler;

#[async_trait]
impl SFrameKeyHandler for RejectSFrameKeyHandler {
    async fn handle_sframe_epoch_key(
        &mut self,
        _: &str,
        _: &str,
        _: &SFrameEpochKeyUpdate,
    ) -> Result<(), CoreError> {
        Err(CoreError::Authentication)
    }
}

/// Private, in-conversation delivery receipt request. The host must pass its
/// content through the normal MLS send coordinator; this type is never sent
/// as plaintext to the gateway.
pub struct DeliveryReceipt {
    pub conversation_id: String,
    pub message_ids: Vec<String>,
    pub observed_at_ms: u64,
}

impl DeliveryReceipt {
    fn new(
        conversation_id: String,
        message_ids: Vec<String>,
        observed_at_ms: u64,
    ) -> Result<Self, CoreError> {
        protocol::validate_id(&conversation_id)?;
        if observed_at_ms == 0 || message_ids.is_empty() || message_ids.len() > 100 {
            return Err(CoreError::Authentication);
        }
        let mut ids = HashSet::with_capacity(message_ids.len());
        for id in &message_ids {
            protocol::validate_id(id)?;
            if !ids.insert(id) {
                return Err(CoreError::Authentication);
            }
        }
        Ok(Self {
            conversation_id,
            message_ids,
            observed_at_ms,
        })
    }

    pub fn content(&self) -> v1::message::Content {
        v1::message::Content::Receipts(v1::Receipts {
            kind: v1::receipts::Kind::Delivered as i32,
            message_ids: self.message_ids.clone(),
            observed_at_ms: self.observed_at_ms,
        })
    }
}

pub struct DirectReceiveResult {
    pub messages: Vec<v1::Message>,
    pub delivery_receipts: Vec<DeliveryReceipt>,
    pub queue_acks: Vec<v1::QueueAck>,
    pub tombstones: usize,
    pub cursor: u64,
}

pub type GroupReceiveResult = DirectReceiveResult;
pub type BroadcastReceiveResult = DirectReceiveResult;

/// Fetch replay pages, apply authenticated MLS epoch updates, decrypt and
/// render messages, durably commit them, emit QueueAck, and return private
/// delivery-receipt requests for the host's MLS send path.
pub async fn receive_available<C, M, T, S, R>(
    core: &mut ClientCore<C, M>,
    sync: &mut SyncState,
    transport: &mut T,
    store: &mut S,
    renderer: &mut R,
    replay_limit: u32,
    now_ms: u64,
) -> Result<DirectReceiveResult, CoreError>
where
    C: EnvelopeCrypto,
    M: MlsEngine,
    T: DirectReceiveTransport,
    S: DirectReceiveStore,
    R: MessageRenderer,
{
    let mut handler = RejectSFrameKeyHandler;
    receive_available_with_sframe(
        core,
        sync,
        transport,
        store,
        renderer,
        &mut handler,
        replay_limit,
        now_ms,
    )
    .await
}

/// Direct receive path with an authenticated MLS SFrame control sink.
pub async fn receive_available_with_sframe<C, M, T, S, R, H>(
    core: &mut ClientCore<C, M>,
    sync: &mut SyncState,
    transport: &mut T,
    store: &mut S,
    renderer: &mut R,
    handler: &mut H,
    replay_limit: u32,
    now_ms: u64,
) -> Result<DirectReceiveResult, CoreError>
where
    C: EnvelopeCrypto,
    M: MlsEngine,
    T: DirectReceiveTransport,
    S: DirectReceiveStore,
    R: MessageRenderer,
    H: SFrameKeyHandler,
{
    if replay_limit == 0 || replay_limit as usize > protocol::MAX_BATCH_ITEMS || now_ms == 0 {
        return Err(CoreError::InvalidSync);
    }
    let device_id = sync.device_id().to_owned();
    let mut next = transport
        .connect(&device_id, sync.cursor(), replay_limit)
        .await?;
    let mut result = DirectReceiveResult {
        messages: Vec::new(),
        delivery_receipts: Vec::new(),
        queue_acks: Vec::new(),
        tombstones: 0,
        cursor: sync.cursor(),
    };

    loop {
        let high_watermark = next.batch.high_watermark;
        let page = receive_batch(
            core,
            sync,
            transport,
            store,
            renderer,
            next,
            now_ms,
            true,
            handler,
            |_| Ok(()),
        )
        .await?;
        result.messages.extend(page.messages);
        result.delivery_receipts.extend(page.delivery_receipts);
        result.queue_acks.push(page.queue_ack);
        result.tombstones += page.tombstones;
        result.cursor = sync.cursor();
        if page.idle || result.cursor >= high_watermark {
            return Ok(result);
        }
        next = transport.replay(sync.cursor(), replay_limit).await?;
    }
}

/// Receive many-to-many group traffic through the shared cursor/replay path.
/// Group MLS updates are applied before any envelope is opened, and the
/// returned receipts stay private until the host sends them through MLS.
pub async fn receive_group_available<C, M, T, S, R>(
    core: &mut ClientCore<C, M>,
    sync: &mut SyncState,
    transport: &mut T,
    store: &mut S,
    renderer: &mut R,
    replay_limit: u32,
    now_ms: u64,
) -> Result<GroupReceiveResult, CoreError>
where
    C: EnvelopeCrypto,
    M: MlsEngine,
    T: GroupReceiveTransport,
    S: GroupReceiveStore,
    R: MessageRenderer,
{
    let mut handler = RejectSFrameKeyHandler;
    receive_group_available_with_sframe(
        core,
        sync,
        transport,
        store,
        renderer,
        &mut handler,
        replay_limit,
        now_ms,
    )
    .await
}

/// Group receive path with an authenticated MLS SFrame control sink.
pub async fn receive_group_available_with_sframe<C, M, T, S, R, H>(
    core: &mut ClientCore<C, M>,
    sync: &mut SyncState,
    transport: &mut T,
    store: &mut S,
    renderer: &mut R,
    handler: &mut H,
    replay_limit: u32,
    now_ms: u64,
) -> Result<GroupReceiveResult, CoreError>
where
    C: EnvelopeCrypto,
    M: MlsEngine,
    T: GroupReceiveTransport,
    S: GroupReceiveStore,
    R: MessageRenderer,
    H: SFrameKeyHandler,
{
    if replay_limit == 0 || replay_limit as usize > protocol::MAX_BATCH_ITEMS || now_ms == 0 {
        return Err(CoreError::InvalidSync);
    }
    let device_id = sync.device_id().to_owned();
    let mut next = transport
        .connect(&device_id, sync.cursor(), replay_limit)
        .await?;
    transport.validate_group_batch(&next)?;
    let mut result = DirectReceiveResult {
        messages: Vec::new(),
        delivery_receipts: Vec::new(),
        queue_acks: Vec::new(),
        tombstones: 0,
        cursor: sync.cursor(),
    };

    loop {
        let high_watermark = next.batch.high_watermark;
        let page = receive_batch(
            core,
            sync,
            transport,
            store,
            renderer,
            next,
            now_ms,
            true,
            handler,
            |_| Ok(()),
        )
        .await?;
        result.messages.extend(page.messages);
        result.delivery_receipts.extend(page.delivery_receipts);
        result.queue_acks.push(page.queue_ack);
        result.tombstones += page.tombstones;
        result.cursor = sync.cursor();
        if page.idle || result.cursor >= high_watermark {
            return Ok(result);
        }
        next = transport.replay(sync.cursor(), replay_limit).await?;
        transport.validate_group_batch(&next)?;
    }
}

/// Receive broadcast traffic with a passive subscriber engine. Only
/// broadcast Welcome/Commit updates are accepted, and the subscriber engine
/// rejects every local publishing or membership operation.
pub async fn receive_broadcast_available<C, M, T, S, R>(
    core: &mut ClientCore<C, BroadcastSubscriber<M>>,
    sync: &mut SyncState,
    transport: &mut T,
    store: &mut S,
    renderer: &mut R,
    admins: &impl BroadcastAdminVerifier,
    replay_limit: u32,
    now_ms: u64,
) -> Result<BroadcastReceiveResult, CoreError>
where
    C: EnvelopeCrypto,
    M: MlsEngine,
    T: BroadcastReceiveTransport,
    S: GroupReceiveStore,
    R: MessageRenderer,
{
    if replay_limit == 0 || replay_limit as usize > protocol::MAX_BATCH_ITEMS || now_ms == 0 {
        return Err(CoreError::InvalidSync);
    }
    let device_id = sync.device_id().to_owned();
    let mut next = transport
        .connect(&device_id, sync.cursor(), replay_limit)
        .await?;
    transport.validate_broadcast_batch(&next)?;
    let mut handler = RejectSFrameKeyHandler;
    let mut result = DirectReceiveResult {
        messages: Vec::new(),
        delivery_receipts: Vec::new(),
        queue_acks: Vec::new(),
        tombstones: 0,
        cursor: sync.cursor(),
    };

    loop {
        let high_watermark = next.batch.high_watermark;
        let page = receive_batch(
            core,
            sync,
            transport,
            store,
            renderer,
            next,
            now_ms,
            false,
            &mut handler,
            |message| {
                let Some(v1::message::Content::BroadcastPost(post)) = message.content.as_ref()
                else {
                    return Err(CoreError::Authentication);
                };
                verify_post(message, post, admins).map(|_| ())
            },
        )
        .await?;
        result.messages.extend(page.messages);
        result.delivery_receipts.extend(page.delivery_receipts);
        result.queue_acks.push(page.queue_ack);
        result.tombstones += page.tombstones;
        result.cursor = sync.cursor();
        if page.idle || result.cursor >= high_watermark {
            return Ok(result);
        }
        next = transport.replay(sync.cursor(), replay_limit).await?;
        transport.validate_broadcast_batch(&next)?;
    }
}

/// Fetch broker dispatches, decrypt each signed Message with the channel
/// master key, verify the admin signature and current role, render it, then
/// acknowledge it. The broker never sees the plaintext post.
pub async fn receive_broadcast_dispatches<B, V, R>(
    conversation_id: &str,
    master_key: &BroadcastMasterKey,
    admins: &V,
    transport: &mut B,
    renderer: &mut R,
    fetch_limit: u32,
) -> Result<Vec<v1::Message>, CoreError>
where
    B: BroadcastDispatchTransport,
    V: BroadcastAdminVerifier,
    R: MessageRenderer,
{
    protocol::validate_id(conversation_id)?;
    if fetch_limit == 0 || fetch_limit as usize > protocol::MAX_BATCH_ITEMS {
        return Err(CoreError::InvalidSync);
    }
    let dispatches = transport
        .fetch_broadcast(conversation_id, fetch_limit)
        .await?;
    if dispatches.len() > fetch_limit as usize {
        return Err(CoreError::InvalidSync);
    }

    let mut seen_posts = HashSet::with_capacity(dispatches.len());
    let mut messages = Vec::with_capacity(dispatches.len());
    for dispatch in dispatches {
        protocol::validate_broadcast_dispatch(&dispatch)?;
        if dispatch.conversation_id != conversation_id
            || !seen_posts.insert(dispatch.post_id.clone())
        {
            return Err(CoreError::Authentication);
        }
        let message = master_key.decrypt_dispatch(&dispatch)?;
        let Some(v1::message::Content::BroadcastPost(post)) = message.content.as_ref() else {
            return Err(CoreError::Authentication);
        };
        verify_post(&message, post, admins)?;
        renderer.render(&message).await?;
        transport.acknowledge_broadcast(&dispatch).await?;
        messages.push(message);
    }
    Ok(messages)
}

struct ReceivedPage {
    messages: Vec<v1::Message>,
    delivery_receipts: Vec<DeliveryReceipt>,
    queue_ack: v1::QueueAck,
    tombstones: usize,
    idle: bool,
}

async fn receive_batch<C, M, T, S, R, F>(
    core: &mut ClientCore<C, M>,
    sync: &mut SyncState,
    transport: &mut T,
    store: &mut S,
    renderer: &mut R,
    incoming: DirectReceiveBatch,
    now_ms: u64,
    collect_delivery_receipts: bool,
    handler: &mut impl SFrameKeyHandler,
    validate_message: F,
) -> Result<ReceivedPage, CoreError>
where
    C: EnvelopeCrypto,
    M: MlsEngine,
    T: DirectReceiveTransport,
    S: DirectReceiveStore,
    R: MessageRenderer,
    F: Fn(&v1::Message) -> Result<(), CoreError>,
{
    let advance = sync.prepare(&incoming.batch)?;
    for update in &incoming.mls_updates {
        update.apply(core.mls_mut())?;
    }

    let had_items = !incoming.batch.items.is_empty();
    let mut items = Vec::with_capacity(incoming.batch.items.len());
    let mut messages = Vec::new();
    let mut receipts = Vec::new();
    let mut tombstones = 0;
    for item in incoming.batch.items {
        match item.entry.ok_or(CoreError::InvalidSync)? {
            v1::queue_item::Entry::Envelope(envelope) => {
                let message = core.open_envelope(&envelope, now_ms)?;
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
                validate_message(&message)?;
                if collect_delivery_receipts {
                    add_receipt(&mut receipts, &message, now_ms)?;
                }
                items.push(DecryptedSyncItem::Message {
                    cursor: item.cursor,
                    message: message.clone(),
                });
                messages.push(message);
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

    store
        .commit_batch(&advance, &incoming.mls_updates, &items)
        .await?;
    for message in &messages {
        renderer.render(message).await?;
    }
    let queue_ack = sync.commit(advance)?;
    transport.acknowledge(queue_ack.clone()).await?;
    Ok(ReceivedPage {
        idle: !had_items,
        messages,
        delivery_receipts: receipts,
        queue_ack,
        tombstones,
    })
}

fn add_receipt(
    receipts: &mut Vec<DeliveryReceipt>,
    message: &v1::Message,
    observed_at_ms: u64,
) -> Result<(), CoreError> {
    if let Some(receipt) = receipts
        .iter_mut()
        .find(|receipt| receipt.conversation_id == message.conversation_id)
    {
        if receipt.message_ids.len() == 100 {
            return Err(CoreError::Authentication);
        }
        if !receipt.message_ids.contains(&message.message_id) {
            receipt.message_ids.push(message.message_id.clone());
        }
        return Ok(());
    }
    receipts.push(DeliveryReceipt::new(
        message.conversation_id.clone(),
        vec![message.message_id.clone()],
        observed_at_ms,
    )?);
    Ok(())
}
