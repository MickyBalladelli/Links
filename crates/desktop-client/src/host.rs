//! Synchronous macOS host orchestration for the shared client core.
//!
//! The native host usually has async URLSession and WebSocket code. It can
//! bridge that code to [`DesktopCoreServices`] on its serialized core queue.
//! This adapter keeps the ordering rules in one place: claim and verify every
//! recipient device, stage and deliver MLS changes, persist exact envelopes,
//! decrypt replay items, commit the inbox/cursor, and only then send QueueAck.

use crate::session::{DesktopFrameResult, DesktopFrameTransport, DesktopReceivedTextMessage};
use links_client_core::{
    crypto::{EnvelopeCrypto, RecipientKeyDirectory},
    envelopes::{ClientCore, FanoutRecipient},
    mls::{MlsEngine, PendingCommit},
    protocol::{self, v1},
    sequences::ConversationSequence,
    sync::{decode_sync_batch_body, SyncState},
    CoreError,
};
use prost::Message;
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

/// One decrypted queue item passed to the host's atomic inbox transaction.
/// The host must encrypt message data before writing it to local storage.
pub enum DesktopInboxItem {
    Message { cursor: u64, message: v1::Message },
    Tombstone { cursor: u64, reason: i32 },
}

/// Host callbacks used by [`DesktopCoreHostAdapter`].
///
/// `lookup_and_claim_recipient_devices` must perform the authenticated
/// directory lookup, claim one pre-key bundle for every active device, and
/// return the MLS KeyPackage belonging to each same device. The adapter then
/// verifies each claimed bundle against the directory identity key before it
/// can seal anything.
///
/// Implementations must make the persistence methods atomic with their local
/// encrypted MLS state. The bearer token must remain in the caller and never
/// enter this trait's durable records.
pub trait DesktopCoreServices: Send {
    fn durable_cursor(&self) -> Result<u64, CoreError>;

    fn now_ms(&self) -> Result<u64, CoreError> {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .ok()
            .and_then(|duration| u64::try_from(duration.as_millis()).ok())
            .filter(|value| *value > 0)
            .ok_or(CoreError::Provider)
    }

    /// Lookup the public device snapshot and claim authenticated pre-keys.
    fn lookup_and_claim_recipient_devices(
        &mut self,
        recipient_user_id: &str,
    ) -> Result<Vec<links_client_core::send::RecipientDevice>, CoreError>;

    fn load_conversation_sequence(
        &mut self,
        conversation_id: &str,
        sender_device_id: &str,
    ) -> Result<ConversationSequence, CoreError>;

    fn next_message_id(&mut self) -> Result<String, CoreError>;

    fn persist_pending_commit(
        &mut self,
        conversation_id: &str,
        pending: &PendingCommit,
    ) -> Result<(), CoreError>;

    /// Deliver the exact staged MLS commit/Welcome through the authenticated
    /// host transport. The commit is not merged locally until this returns.
    fn deliver_mls_bootstrap(
        &mut self,
        conversation_id: &str,
        pending: &PendingCommit,
        transport: &mut dyn DesktopFrameTransport,
    ) -> Result<(), CoreError>;

    fn mark_pending_commit_accepted(&mut self, conversation_id: &str) -> Result<(), CoreError>;

    /// Persist the exact serialized Send frames before any frame is put on the
    /// socket. Retries must reuse these bytes unchanged.
    fn persist_send(
        &mut self,
        message: &v1::Message,
        envelopes: &[v1::Envelope],
        frames: &[Vec<u8>],
        last_sequence_id: u64,
    ) -> Result<(), CoreError>;

    fn mark_outbox_accepted(&mut self, envelope_id: &str) -> Result<(), CoreError>;

    /// Atomically commit decrypted inbox items, MLS changes made while
    /// opening them, and the cursor transition. The host must deduplicate by
    /// message ID and accept only the expected cursor transition.
    fn commit_receive(
        &mut self,
        previous_cursor: u64,
        next_cursor: u64,
        items: &[DesktopInboxItem],
    ) -> Result<(), CoreError>;
}

/// Concrete host implementation for the desktop binding.
pub struct DesktopCoreHostAdapter<S> {
    services: S,
    sync: Option<SyncState>,
}

impl<S> DesktopCoreHostAdapter<S> {
    pub fn new(services: S) -> Self {
        Self {
            services,
            sync: None,
        }
    }

    pub fn services(&self) -> &S {
        &self.services
    }

    pub fn services_mut(&mut self) -> &mut S {
        &mut self.services
    }

    /// Forget only the in-memory replay checkpoint. The next frame restores
    /// it from the durable cursor; encrypted inbox/outbox data is untouched.
    pub fn reset_replay_state(&mut self) {
        self.sync = None;
    }
}

impl<C, M, S> super::core::DesktopCoreHost<C, M> for DesktopCoreHostAdapter<S>
where
    C: EnvelopeCrypto + RecipientKeyDirectory + Send,
    M: MlsEngine + Send,
    S: DesktopCoreServices,
{
    fn durable_cursor(&self) -> Result<u64, CoreError> {
        let cursor = self.services.durable_cursor()?;
        if cursor > protocol::MAX_CURSOR {
            return Err(CoreError::InvalidSync);
        }
        Ok(cursor)
    }

    fn handle_server_frame(
        &mut self,
        core: &mut ClientCore<C, M>,
        frame: &v1::ServerFrame,
        transport: &mut dyn DesktopFrameTransport,
        full_sync: bool,
        on_text_message: &mut dyn FnMut(DesktopReceivedTextMessage),
    ) -> Result<DesktopFrameResult, CoreError> {
        let body = frame.body.as_ref().ok_or(CoreError::InvalidSync)?;
        match body {
            v1::server_frame::Body::Welcome(_) => Ok(DesktopFrameResult::Pending),
            v1::server_frame::Body::Accepted(accepted) => {
                self.services.mark_outbox_accepted(&accepted.envelope_id)?;
                Ok(DesktopFrameResult::Pending)
            }
            v1::server_frame::Body::Error(error) => Err(server_error(error)),
            v1::server_frame::Body::WebRtcSignal(_) => Err(CoreError::Provider),
            v1::server_frame::Body::MlsBootstrap(_) => Err(CoreError::Provider),
            v1::server_frame::Body::Batch(_) | v1::server_frame::Body::CompressedBatch(_) => {
                self.handle_sync_batch(core, body, transport, full_sync, on_text_message)
            }
        }
    }

    fn send_text(
        &mut self,
        core: &mut ClientCore<C, M>,
        conversation_id: &str,
        recipient_user_id: &str,
        text: &str,
        transport: &mut dyn DesktopFrameTransport,
    ) -> Result<(), CoreError> {
        protocol::validate_id(conversation_id)?;
        protocol::validate_id(recipient_user_id)?;
        if recipient_user_id == core.user_id()
            || text.is_empty()
            || text.len() > crate::session::DESKTOP_MAX_TEXT_BYTES
        {
            return Err(CoreError::Authentication);
        }

        let now_ms = self.services.now_ms()?;
        let expires_at_ms = now_ms
            .checked_add(protocol::MAX_RETENTION_MS)
            .ok_or(CoreError::Provider)?;
        let message_id = self.services.next_message_id()?;
        protocol::validate_id(&message_id)?;
        let mut sequence = self
            .services
            .load_conversation_sequence(conversation_id, core.device_id())?;
        if sequence.conversation_id() != conversation_id
            || sequence.sender_device_id() != core.device_id()
        {
            return Err(CoreError::Authentication);
        }

        let recipients = self
            .services
            .lookup_and_claim_recipient_devices(recipient_user_id)?;
        if recipients.is_empty() || recipients.len() > protocol::MAX_FANOUT_DEVICES {
            return Err(CoreError::Authentication);
        }

        let mut fanout = Vec::with_capacity(recipients.len());
        let mut key_packages = Vec::with_capacity(recipients.len());
        let mut device_ids = std::collections::HashSet::with_capacity(recipients.len());
        for recipient in &recipients {
            if recipient.user_id != recipient_user_id || !device_ids.insert(&recipient.device_id) {
                return Err(CoreError::Authentication);
            }
            let sealed_sender_key = recipient.verify()?;
            core.crypto_mut()
                .install_recipient_public_key(&recipient.device_id, sealed_sender_key)?;
            fanout.push(FanoutRecipient::new(recipient.device_id.clone())?);
            key_packages.push(recipient.mls_key_package.as_slice());
        }

        if let Some(pending) = core
            .mls_mut()
            .ensure_direct_group(conversation_id, &key_packages)?
        {
            self.services
                .persist_pending_commit(conversation_id, &pending)?;
            self.services
                .deliver_mls_bootstrap(conversation_id, &pending, transport)?;
            core.mls_mut()
                .merge_pending_direct_commit(conversation_id)?;
            self.services
                .mark_pending_commit_accepted(conversation_id)?;
        }

        let message = v1::Message {
            message_id,
            conversation_id: conversation_id.to_owned(),
            sender_device_id: core.device_id().to_owned(),
            sent_at_ms: now_ms,
            sequence_id: 0,
            content: Some(v1::message::Content::Text(text.to_owned())),
        };
        let (message, envelopes) = core.seal_next_message_for_devices(
            message,
            &mut sequence,
            &fanout,
            expires_at_ms,
            now_ms,
        )?;
        let frames = envelopes
            .iter()
            .map(encode_send_frame)
            .collect::<Result<Vec<_>, _>>()?;
        self.services
            .persist_send(&message, &envelopes, &frames, sequence.last_sequence_id())?;
        for frame in &frames {
            transport.send(frame)?;
        }
        Ok(())
    }
}

impl<S> DesktopCoreHostAdapter<S> {
    fn handle_sync_batch<C, M>(
        &mut self,
        core: &mut ClientCore<C, M>,
        body: &v1::server_frame::Body,
        transport: &mut dyn DesktopFrameTransport,
        full_sync: bool,
        on_text_message: &mut dyn FnMut(DesktopReceivedTextMessage),
    ) -> Result<DesktopFrameResult, CoreError>
    where
        C: EnvelopeCrypto + Send,
        M: MlsEngine + Send,
        S: DesktopCoreServices,
    {
        let batch = decode_sync_batch_body(body)?;
        let cursor = self.services.durable_cursor()?;
        if self.sync.is_none() {
            self.sync = Some(SyncState::restore(core.device_id().to_owned(), cursor)?);
        }
        let sync = self.sync.as_ref().ok_or(CoreError::Provider)?;
        if sync.device_id() != core.device_id() {
            return Err(CoreError::InvalidSync);
        }
        if sync.cursor() != cursor {
            return Err(CoreError::InvalidSync);
        }
        let advance = sync.prepare(&batch)?;
        let mut items = Vec::with_capacity(batch.items.len());
        let mut rendered = Vec::new();
        let now_ms = self.services.now_ms()?;
        for item in batch.items {
            let item_cursor = item.cursor;
            match item.entry.ok_or(CoreError::InvalidSync)? {
                v1::queue_item::Entry::Envelope(envelope) => {
                    let message = core.open_envelope(&envelope, now_ms)?;
                    if let Some(v1::message::Content::Text(text)) = message.content.as_ref() {
                        rendered.push(DesktopReceivedTextMessage {
                            conversation_id: message.conversation_id.clone(),
                            sender_device_id: message.sender_device_id.clone(),
                            text: text.clone(),
                            sequence_id: message.sequence_id,
                            sent_at_ms: message.sent_at_ms,
                        });
                    }
                    items.push(DesktopInboxItem::Message {
                        cursor: item_cursor,
                        message,
                    });
                }
                v1::queue_item::Entry::Tombstone(tombstone) => {
                    items.push(DesktopInboxItem::Tombstone {
                        cursor: item_cursor,
                        reason: tombstone.reason,
                    });
                }
            }
        }

        self.services
            .commit_receive(advance.previous_cursor(), advance.next_cursor(), &items)?;
        let ack = self
            .sync
            .as_mut()
            .ok_or(CoreError::Provider)?
            .commit(advance)?;
        transport.send(&encode_ack_frame(ack)?)?;
        for message in rendered {
            on_text_message(message);
        }

        if self.sync.as_ref().ok_or(CoreError::Provider)?.cursor() < batch.high_watermark {
            transport.send(&encode_replay_frame(
                self.sync.as_ref().ok_or(CoreError::Provider)?.cursor(),
            )?)?;
            return Ok(DesktopFrameResult::Pending);
        }
        if full_sync {
            Ok(DesktopFrameResult::RecoveryComplete)
        } else {
            Ok(DesktopFrameResult::Pending)
        }
    }
}

fn encode_send_frame(envelope: &v1::Envelope) -> Result<Vec<u8>, CoreError> {
    protocol::validate_envelope(envelope)?;
    encode_client_frame(v1::client_frame::Body::Send(envelope.clone()))
}

fn encode_ack_frame(ack: v1::QueueAck) -> Result<Vec<u8>, CoreError> {
    encode_client_frame(v1::client_frame::Body::Ack(ack))
}

fn encode_replay_frame(after_cursor: u64) -> Result<Vec<u8>, CoreError> {
    if after_cursor > protocol::MAX_CURSOR {
        return Err(CoreError::InvalidSync);
    }
    encode_client_frame(v1::client_frame::Body::Replay(v1::Replay {
        after_cursor,
        limit: protocol::MAX_BATCH_ITEMS as u32,
    }))
}

fn encode_client_frame(body: v1::client_frame::Body) -> Result<Vec<u8>, CoreError> {
    let frame = v1::ClientFrame {
        request_id: Uuid::new_v4().to_string(),
        body: Some(body),
    };
    protocol::validate_id(&frame.request_id)?;
    match frame.body.as_ref().ok_or(CoreError::Provider)? {
        v1::client_frame::Body::Send(envelope) => protocol::validate_envelope(envelope)?,
        v1::client_frame::Body::Replay(replay)
            if replay.after_cursor > protocol::MAX_CURSOR
                || replay.limit == 0
                || replay.limit as usize > protocol::MAX_BATCH_ITEMS =>
        {
            return Err(CoreError::InvalidSync);
        }
        v1::client_frame::Body::Ack(ack) if ack.through_cursor > protocol::MAX_CURSOR => {
            return Err(CoreError::InvalidSync)
        }
        v1::client_frame::Body::Hello(_)
        | v1::client_frame::Body::WebRtcSignal(_)
        | v1::client_frame::Body::MlsBootstrap(_) => {
            return Err(CoreError::Provider)
        }
        _ => {}
    }
    let mut bytes = Vec::with_capacity(frame.encoded_len());
    frame.encode(&mut bytes).map_err(|_| CoreError::Provider)?;
    if bytes.len() > protocol::MAX_FRAME_BYTES {
        return Err(CoreError::Protocol(protocol::ProtocolError::TooLarge));
    }
    Ok(bytes)
}

fn server_error(error: &v1::ProtocolError) -> CoreError {
    match error.code {
        2 => CoreError::Authentication,
        7 => CoreError::SessionConflict,
        5 => CoreError::InvalidSync,
        _ => CoreError::Provider,
    }
}
