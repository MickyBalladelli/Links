//! Authenticated one-to-one send orchestration.
//!
//! The directory adapter supplies a verified pre-key bundle and an MLS
//! KeyPackage for every active recipient device. The transport adapter owns
//! the not-yet-versioned MLS commit/Welcome control path and the opaque
//! envelope path; this crate never treats one as the other.

use crate::{
    attachments::EncryptedVoiceNote,
    crypto::{EnvelopeCrypto, RecipientKeyDirectory},
    envelopes::{ClientCore, FanoutRecipient},
    mls::{MlsEngine, PendingCommit},
    prekeys::claimed_bundle,
    protocol::{self, v1},
    sequences::ConversationSequence,
    CoreError,
};
use async_trait::async_trait;
use std::collections::HashSet;

/// One active physical recipient device from an authenticated directory
/// snapshot. The MLS package is separate because the current pre-key wire
/// schema does not carry MLS KeyPackages.
pub struct RecipientDevice {
    pub user_id: String,
    pub device_id: String,
    pub identity_public_key: [u8; 32],
    pub prekey_bundle: v1::PreKeyBundle,
    pub mls_key_package: Vec<u8>,
}

impl RecipientDevice {
    pub fn new(
        user_id: String,
        device_id: String,
        identity_public_key: [u8; 32],
        prekey_bundle: v1::PreKeyBundle,
        mls_key_package: Vec<u8>,
    ) -> Result<Self, CoreError> {
        protocol::validate_id(&user_id)?;
        protocol::validate_id(&device_id)?;
        if mls_key_package.is_empty() || mls_key_package.len() > protocol::MAX_FRAME_BYTES {
            return Err(CoreError::Authentication);
        }
        Ok(Self {
            user_id,
            device_id,
            identity_public_key,
            prekey_bundle,
            mls_key_package,
        })
    }

    /// Verify the server claim and return the recipient's authenticated
    /// X25519 key used by the Sealed Sender wrapper.
    pub fn verify(&self) -> Result<[u8; 32], CoreError> {
        if self.prekey_bundle.device_id != self.device_id {
            return Err(CoreError::Authentication);
        }
        let bundle = claimed_bundle(&self.prekey_bundle, &self.identity_public_key)?;
        Ok(bundle.identity.dh_key)
    }
}

/// Authenticated directory lookup for every active physical device belonging
/// to the target user. Implementations must verify directory authenticity
/// before returning records.
#[async_trait]
pub trait DirectChatDirectory: Send + Sync {
    async fn query_recipient_devices(
        &self,
        recipient_user_id: &str,
    ) -> Result<Vec<RecipientDevice>, CoreError>;
}

/// Adapter for the MLS bootstrap control path and opaque envelope delivery.
/// The existing transport protobuf has no MLS commit/Welcome message yet, so
/// this boundary must be backed by an authenticated versioned adapter.
#[async_trait]
pub trait DirectChatTransport: Send {
    async fn deliver_mls_bootstrap(
        &mut self,
        conversation_id: &str,
        pending: &PendingCommit,
    ) -> Result<(), CoreError>;

    async fn send_envelope(&mut self, envelope: &v1::Envelope) -> Result<(), CoreError>;
}

/// Durable send boundary. Persist pending MLS state before bootstrap delivery,
/// then persist the exact message, sequence and envelopes before sending any
/// envelope to the gateway. The store must make each operation transactional
/// with its host's MLS provider state.
#[async_trait]
pub trait DirectChatStore: Send {
    async fn persist_pending_commit(
        &mut self,
        conversation_id: &str,
        pending: &PendingCommit,
    ) -> Result<(), CoreError>;

    async fn mark_pending_commit_accepted(
        &mut self,
        conversation_id: &str,
    ) -> Result<(), CoreError>;

    async fn persist_send(
        &mut self,
        message: &v1::Message,
        envelopes: &[v1::Envelope],
        last_sequence_id: u64,
    ) -> Result<(), CoreError>;
}

pub struct DirectSendResult {
    pub message: v1::Message,
    pub envelopes: Vec<v1::Envelope>,
}

/// Query recipient prekeys, establish or resume the direct MLS group, encrypt
/// one application message, and persist/send one Sealed Sender envelope per
/// active recipient device.
pub async fn send_message<C, M, D, T, O>(
    core: &mut ClientCore<C, M>,
    sequence: &mut ConversationSequence,
    directory: &D,
    transport: &mut T,
    store: &mut O,
    conversation_id: String,
    recipient_user_id: String,
    message_id: String,
    content: v1::message::Content,
    sent_at_ms: u64,
    expires_at_ms: u64,
) -> Result<DirectSendResult, CoreError>
where
    C: EnvelopeCrypto + RecipientKeyDirectory,
    M: MlsEngine,
    D: DirectChatDirectory,
    T: DirectChatTransport,
    O: DirectChatStore,
{
    protocol::validate_id(&conversation_id)?;
    protocol::validate_id(&recipient_user_id)?;
    protocol::validate_id(&message_id)?;
    if recipient_user_id == core.user_id()
        || conversation_id != sequence.conversation_id()
        || sequence.sender_device_id() != core.device_id()
    {
        return Err(CoreError::Authentication);
    }

    let recipients = directory
        .query_recipient_devices(&recipient_user_id)
        .await?;
    if recipients.is_empty() || recipients.len() > protocol::MAX_FANOUT_DEVICES {
        return Err(CoreError::Authentication);
    }

    let mut device_ids = HashSet::with_capacity(recipients.len());
    let mut fanout = Vec::with_capacity(recipients.len());
    let mut key_packages = Vec::with_capacity(recipients.len());
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

    let pending = core
        .mls_mut()
        .ensure_direct_group(&conversation_id, &key_packages)?;
    if let Some(pending) = pending {
        store
            .persist_pending_commit(&conversation_id, &pending)
            .await?;
        transport
            .deliver_mls_bootstrap(&conversation_id, &pending)
            .await?;
        core.mls_mut()
            .merge_pending_direct_commit(&conversation_id)?;
        store.mark_pending_commit_accepted(&conversation_id).await?;
    }

    let message = v1::Message {
        message_id,
        conversation_id,
        sender_device_id: core.device_id().to_owned(),
        sent_at_ms,
        sequence_id: 0,
        content: Some(content),
    };
    let (message, envelopes) =
        core.seal_next_message_for_devices(message, sequence, &fanout, expires_at_ms, sent_at_ms)?;
    store
        .persist_send(&message, &envelopes, sequence.last_sequence_id())
        .await?;
    for envelope in &envelopes {
        transport.send_envelope(envelope).await?;
    }
    Ok(DirectSendResult { message, envelopes })
}

pub async fn send_text<C, M, D, T, O>(
    core: &mut ClientCore<C, M>,
    sequence: &mut ConversationSequence,
    directory: &D,
    transport: &mut T,
    store: &mut O,
    conversation_id: String,
    recipient_user_id: String,
    message_id: String,
    text: String,
    sent_at_ms: u64,
    expires_at_ms: u64,
) -> Result<DirectSendResult, CoreError>
where
    C: EnvelopeCrypto + RecipientKeyDirectory,
    M: MlsEngine,
    D: DirectChatDirectory,
    T: DirectChatTransport,
    O: DirectChatStore,
{
    send_message(
        core,
        sequence,
        directory,
        transport,
        store,
        conversation_id,
        recipient_user_id,
        message_id,
        v1::message::Content::Text(text),
        sent_at_ms,
        expires_at_ms,
    )
    .await
}

/// Upload the opaque attachment before calling this coordinator, then send
/// only its private MediaMetadata through the normal MLS/Sealed Sender path.
/// The host must verify the exact upload receipt before invoking this function.
pub async fn send_voice_note<C, M, D, T, O>(
    core: &mut ClientCore<C, M>,
    sequence: &mut ConversationSequence,
    directory: &D,
    transport: &mut T,
    store: &mut O,
    conversation_id: String,
    recipient_user_id: String,
    message_id: String,
    attachment: &EncryptedVoiceNote,
    sent_at_ms: u64,
    expires_at_ms: u64,
) -> Result<DirectSendResult, CoreError>
where
    C: EnvelopeCrypto + RecipientKeyDirectory,
    M: MlsEngine,
    D: DirectChatDirectory,
    T: DirectChatTransport,
    O: DirectChatStore,
{
    let _validated = EncryptedVoiceNote::new(
        attachment.media.clone(),
        attachment.ciphertext.clone(),
    )?;
    send_message(
        core,
        sequence,
        directory,
        transport,
        store,
        conversation_id,
        recipient_user_id,
        message_id,
        v1::message::Content::Media(attachment.media.clone()),
        sent_at_ms,
        expires_at_ms,
    )
    .await
}
