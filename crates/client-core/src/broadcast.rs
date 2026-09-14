//! MLS broadcast profile boundaries.
//!
//! MLS does not define an application-level "read-only" member bit. Links
//! enforces that policy at the client-core boundary: a broadcast subscriber
//! may join and process authenticated publisher commits, but cannot create a
//! group, create application messages, stage membership changes, or merge a
//! local pending commit.

use crate::{
    mls::{MlsEngine, MlsIdentitySigner},
    protocol::{self, v1},
    CoreError,
};
use uuid::Uuid;

const BROADCAST_POST_DOMAIN: &[u8] = b"links/broadcast-post/v1\0";

/// Directory/RBAC policy for broadcast publisher devices. Implementations must
/// verify that the device is active, belongs to the conversation, and currently
/// has an owner/admin role before accepting its post.
pub trait BroadcastAdminVerifier: Send + Sync {
    fn verify_admin_device(
        &self,
        conversation_id: &str,
        device_id: &str,
        public_key: &[u8; 32],
    ) -> Result<(), CoreError>;
}

impl<F> BroadcastAdminVerifier for F
where
    F: Fn(&str, &str, &[u8; 32]) -> Result<(), CoreError> + Send + Sync,
{
    fn verify_admin_device(
        &self,
        conversation_id: &str,
        device_id: &str,
        public_key: &[u8; 32],
    ) -> Result<(), CoreError> {
        self(conversation_id, device_id, public_key)
    }
}

/// Canonical bytes signed by an admin device. UUIDs are encoded as fixed
/// sixteen-byte values and the payload has an explicit length prefix.
pub fn post_transcript(
    conversation_id: &str,
    message_id: &str,
    sender_device_id: &str,
    post_id: &str,
    epoch: u64,
    admin_device_id: &str,
    admin_public_key: &[u8; 32],
    payload: &[u8],
) -> Result<Vec<u8>, CoreError> {
    for id in [
        conversation_id,
        message_id,
        sender_device_id,
        post_id,
        admin_device_id,
    ] {
        protocol::validate_id(id)?;
    }
    if payload.is_empty() || payload.len() > protocol::MAX_MESSAGE_BYTES {
        return Err(CoreError::Authentication);
    }
    let mut transcript = BROADCAST_POST_DOMAIN.to_vec();
    for id in [
        conversation_id,
        message_id,
        sender_device_id,
        post_id,
        admin_device_id,
    ] {
        let uuid = Uuid::parse_str(id).map_err(|_| CoreError::Authentication)?;
        transcript.extend_from_slice(uuid.as_bytes());
    }
    transcript.extend_from_slice(&epoch.to_be_bytes());
    transcript.extend_from_slice(admin_public_key);
    transcript.extend_from_slice(&(payload.len() as u64).to_be_bytes());
    transcript.extend_from_slice(payload);
    Ok(transcript)
}

/// Build the signed post carried inside an encrypted `Message`.
pub fn sign_post<S: MlsIdentitySigner>(
    conversation_id: &str,
    message_id: &str,
    sender_device_id: &str,
    post_id: String,
    epoch: u64,
    payload: Vec<u8>,
    signer: &S,
) -> Result<v1::BroadcastPost, CoreError> {
    protocol::validate_message(&v1::Message {
        message_id: message_id.to_owned(),
        conversation_id: conversation_id.to_owned(),
        sender_device_id: sender_device_id.to_owned(),
        sent_at_ms: 1,
        sequence_id: 1,
        content: Some(v1::message::Content::BroadcastPost(v1::BroadcastPost {
            post_id: post_id.clone(),
            epoch,
            admin_device_id: sender_device_id.to_owned(),
            admin_public_key: signer.public_key()?.to_vec(),
            payload: payload.clone(),
            signature: vec![0; 64],
        })),
    })?;
    let admin_public_key = signer.public_key()?;
    let transcript = post_transcript(
        conversation_id,
        message_id,
        sender_device_id,
        &post_id,
        epoch,
        sender_device_id,
        &admin_public_key,
        &payload,
    )?;
    let signature = signer.sign(&transcript)?;
    Ok(v1::BroadcastPost {
        post_id,
        epoch,
        admin_device_id: sender_device_id.to_owned(),
        admin_public_key: admin_public_key.to_vec(),
        payload,
        signature: signature.to_vec(),
    })
}

/// Verify the post signature and the account-level admin role before a
/// broadcast renderer sees the payload.
pub fn verify_post<'a, V: BroadcastAdminVerifier>(
    message: &v1::Message,
    post: &'a v1::BroadcastPost,
    admins: &V,
) -> Result<&'a [u8], CoreError> {
    protocol::validate_message(message)?;
    protocol::validate_broadcast_post(post)?;
    if !matches!(
        message.content.as_ref(),
        Some(v1::message::Content::BroadcastPost(_))
    ) || post.post_id != message.message_id
        || post.admin_device_id != message.sender_device_id
    {
        return Err(CoreError::Authentication);
    }
    let admin_public_key: [u8; 32] = post
        .admin_public_key
        .as_slice()
        .try_into()
        .map_err(|_| CoreError::Authentication)?;
    let transcript = post_transcript(
        &message.conversation_id,
        &message.message_id,
        &message.sender_device_id,
        &post.post_id,
        post.epoch,
        &post.admin_device_id,
        &admin_public_key,
        &post.payload,
    )?;
    links_identity::verify(&admin_public_key, &transcript, &post.signature)
        .map_err(|_| CoreError::Authentication)?;
    admins.verify_admin_device(
        &message.conversation_id,
        &post.admin_device_id,
        &admin_public_key,
    )?;
    Ok(&post.payload)
}

/// A passive broadcast subscriber. The wrapped engine is intentionally not
/// exposed, so callers cannot bypass the read-only policy through this type.
pub struct BroadcastSubscriber<M> {
    engine: M,
}

impl<M> BroadcastSubscriber<M> {
    pub fn new(engine: M) -> Self {
        Self { engine }
    }

    pub fn engine(&self) -> &M {
        &self.engine
    }
}

impl<M: MlsEngine> MlsEngine for BroadcastSubscriber<M> {
    fn create_group(&mut self, _: &str, _: &[u8]) -> Result<(), CoreError> {
        Err(CoreError::Authentication)
    }

    fn join_group(&mut self, conversation_id: &str, welcome: &[u8]) -> Result<(), CoreError> {
        self.engine.join_group(conversation_id, welcome)
    }

    fn process_commit(&mut self, conversation_id: &str, commit: &[u8]) -> Result<(), CoreError> {
        self.engine.process_commit(conversation_id, commit)
    }

    fn encrypt(&mut self, _: &str, _: &str, _: &[u8]) -> Result<Vec<u8>, CoreError> {
        Err(CoreError::Authentication)
    }

    fn decrypt(
        &mut self,
        ciphertext: &[u8],
    ) -> Result<crate::mls::AuthenticatedApplication, CoreError> {
        self.engine.decrypt(ciphertext)
    }

    fn current_epoch(&self, conversation_id: &str) -> Result<u64, CoreError> {
        self.engine.current_epoch(conversation_id)
    }

    fn join_direct_group(&mut self, _: &str, _: &[u8]) -> Result<(), CoreError> {
        Err(CoreError::Authentication)
    }

    fn process_direct_commit(&mut self, _: &str, _: &[u8]) -> Result<(), CoreError> {
        Err(CoreError::Authentication)
    }

    fn ensure_direct_group(
        &mut self,
        _: &str,
        _: &[&[u8]],
    ) -> Result<Option<crate::mls::PendingCommit>, CoreError> {
        Err(CoreError::Authentication)
    }

    fn merge_pending_direct_commit(&mut self, _: &str) -> Result<(), CoreError> {
        Err(CoreError::Authentication)
    }

    fn ensure_group(
        &mut self,
        _: &str,
        _: &[&[u8]],
    ) -> Result<Option<crate::mls::PendingCommit>, CoreError> {
        Err(CoreError::Authentication)
    }

    fn merge_pending_group_commit(&mut self, _: &str) -> Result<(), CoreError> {
        Err(CoreError::Authentication)
    }
}
