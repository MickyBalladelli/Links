//! Browser binding for the shared encrypted text core.
//!
//! The browser owns the WebSocket and IndexedDB boundary. This object keeps
//! MLS, sealed-sender keys, cursor ordering, and protobuf frame construction
//! in Rust/WASM. Host code only passes authenticated directory records in and
//! receives opaque frames or committed plaintext messages out.

use links_client_core::{
    crypto::{RecipientKeyDirectory, SealedSenderCrypto, SealedSenderKeyResolver},
    envelopes::ClientCore,
    identity::{IdentitySeed, LocalIdentity},
    mls::{MlsCredentialVerifier, MlsEngine, OpenMlsEngine, RustCryptoProvider},
    prekeys::{
        generate_profile, generate_upload, LocalPreKeyProfile, PreKeySecretStore, PreKeySigner,
        SecretKind,
    },
    protocol::{self, v1},
    send::RecipientDevice,
    sequences::ConversationSequence,
    CoreError,
};
use links_desktop_client::{
    DesktopCoreHostAdapter, DesktopCoreServices, DesktopFrameTransport, DesktopInboxItem,
    DesktopMessagingCore, DesktopReceivedTextMessage, RustDesktopMessagingCore,
};
use openmls::prelude::tls_codec::Deserialize as TlsDeserialize;
use openmls::prelude::{BasicCredential, Credential};
use openmls_rust_crypto::MemoryStorage;
use prost::Message;
use std::{collections::HashMap, sync::Arc};
use uuid::Uuid;
use wasm_bindgen::prelude::*;
use zeroize::Zeroizing;

const MAX_RECIPIENTS: usize = protocol::MAX_FANOUT_DEVICES;

#[derive(Clone)]
struct WebSigner {
    identity: Arc<IdentitySeed>,
}

impl PreKeySigner for WebSigner {
    fn public_key(&self) -> Result<[u8; 32], CoreError> {
        Ok(self.identity.public_key())
    }

    fn sign(&self, transcript: &[u8]) -> Result<[u8; 64], CoreError> {
        Ok(self.identity.sign(transcript))
    }
}

struct WebSecretStore {
    x25519: HashMap<(SecretKind, u64), Zeroizing<[u8; 32]>>,
    kem: HashMap<(SecretKind, u64), Zeroizing<[u8; 64]>>,
}

impl WebSecretStore {
    fn new() -> Self {
        Self {
            x25519: HashMap::new(),
            kem: HashMap::new(),
        }
    }

    fn identity_private(&self, revision: u64) -> Result<[u8; 32], CoreError> {
        self.x25519
            .get(&(SecretKind::IdentityDh, revision))
            .map(|seed| **seed)
            .ok_or(CoreError::Provider)
    }
}

impl PreKeySecretStore for WebSecretStore {
    fn store_x25519(
        &mut self,
        kind: SecretKind,
        id: u64,
        seed: &[u8; 32],
    ) -> Result<(), CoreError> {
        self.x25519.insert((kind, id), Zeroizing::new(*seed));
        Ok(())
    }

    fn store_ml_kem_768(
        &mut self,
        kind: SecretKind,
        id: u64,
        seed: &[u8; 64],
    ) -> Result<(), CoreError> {
        self.kem.insert((kind, id), Zeroizing::new(*seed));
        Ok(())
    }

    fn delete(&mut self, kind: SecretKind, id: u64) -> Result<(), CoreError> {
        self.x25519.remove(&(kind, id));
        self.kem.remove(&(kind, id));
        Ok(())
    }
}

struct WebResolver {
    local_device_id: String,
    local_private: [u8; 32],
    recipient_keys: HashMap<String, [u8; 32]>,
}

impl WebResolver {
    fn new(local_device_id: String, local_private: [u8; 32]) -> Self {
        Self {
            local_device_id,
            local_private,
            recipient_keys: HashMap::new(),
        }
    }
}

impl RecipientKeyDirectory for WebResolver {
    fn install_recipient_public_key(
        &mut self,
        recipient_device_id: &str,
        public_key: [u8; 32],
    ) -> Result<(), CoreError> {
        protocol::validate_id(recipient_device_id)?;
        self.recipient_keys
            .insert(recipient_device_id.to_owned(), public_key);
        Ok(())
    }
}

impl SealedSenderKeyResolver for WebResolver {
    fn recipient_public_key(&self, recipient_device_id: &str) -> Result<[u8; 32], CoreError> {
        self.recipient_keys
            .get(recipient_device_id)
            .copied()
            .ok_or(CoreError::Authentication)
    }

    fn local_private_key(
        &self,
        recipient_device_id: &str,
    ) -> Result<Zeroizing<[u8; 32]>, CoreError> {
        if recipient_device_id != self.local_device_id {
            return Err(CoreError::Authentication);
        }
        Ok(Zeroizing::new(self.local_private))
    }
}

#[derive(Clone)]
struct WebVerifier {
    bindings: HashMap<Uuid, links_identity::DeviceBinding>,
}

impl MlsCredentialVerifier for WebVerifier {
    fn verify(&self, binding: &links_identity::DeviceBinding) -> Result<(), CoreError> {
        if let Some(existing) = self.bindings.get(&binding.device_id) {
            if existing != binding {
                return Err(CoreError::Authentication);
            }
        }
        Ok(())
    }
}

struct WebRecipient {
    device: RecipientDevice,
}

struct WebServices {
    local_device_id: String,
    identity_public_key: [u8; 32],
    mls_credential: Vec<u8>,
    cursor: u64,
    sequences: HashMap<String, u64>,
    recipients: HashMap<String, Vec<WebRecipient>>,
    device_users: HashMap<String, String>,
    active_recipient_user: Option<String>,
    outbox: Vec<Vec<u8>>,
    bootstrap_outbox: Vec<Vec<u8>>,
}

impl WebServices {
    fn new(
        local_device_id: String,
        identity_public_key: [u8; 32],
        mls_credential: Vec<u8>,
    ) -> Self {
        Self {
            local_device_id,
            identity_public_key,
            mls_credential,
            cursor: 0,
            sequences: HashMap::new(),
            recipients: HashMap::new(),
            device_users: HashMap::new(),
            active_recipient_user: None,
            outbox: Vec::new(),
            bootstrap_outbox: Vec::new(),
        }
    }

    fn set_recipient(&mut self, recipient: WebRecipient) {
        let user_id = recipient.device.user_id.clone();
        self.device_users
            .insert(recipient.device.device_id.clone(), user_id.clone());
        let records = self.recipients.entry(user_id).or_default();
        records.retain(|item| item.device.device_id != recipient.device.device_id);
        records.push(recipient);
    }

    fn set_device_user(&mut self, device_id: String, user_id: String) {
        self.device_users.insert(device_id, user_id);
    }

    fn user_for_device(&self, device_id: &str) -> String {
        self.device_users
            .get(device_id)
            .cloned()
            .unwrap_or_default()
    }
}

impl DesktopCoreServices for WebServices {
    fn durable_cursor(&self) -> Result<u64, CoreError> {
        Ok(self.cursor)
    }

    fn now_ms(&self) -> Result<u64, CoreError> {
        let now = wall_clock_ms();
        if !now.is_finite() || now <= 0.0 {
            return Err(CoreError::Provider);
        }
        Ok(now.floor() as u64)
    }

    fn lookup_and_claim_recipient_devices(
        &mut self,
        recipient_user_id: &str,
    ) -> Result<Vec<RecipientDevice>, CoreError> {
        protocol::validate_id(recipient_user_id)?;
        self.active_recipient_user = Some(recipient_user_id.to_owned());
        let records = self
            .recipients
            .get(recipient_user_id)
            .ok_or(CoreError::Authentication)?;
        let records = records
            .iter()
            .filter(|record| record.device.device_id != self.local_device_id)
            .collect::<Vec<_>>();
        if records.is_empty() || records.len() > MAX_RECIPIENTS {
            return Err(CoreError::Authentication);
        }
        Ok(records
            .into_iter()
            .map(|record| record.device.clone())
            .collect())
    }

    fn load_conversation_sequence(
        &mut self,
        conversation_id: &str,
        sender_device_id: &str,
    ) -> Result<ConversationSequence, CoreError> {
        ConversationSequence::restore(
            conversation_id.to_owned(),
            sender_device_id.to_owned(),
            self.sequences.get(conversation_id).copied().unwrap_or(0),
        )
    }

    fn next_message_id(&mut self) -> Result<String, CoreError> {
        Ok(Uuid::new_v4().to_string())
    }

    fn persist_pending_commit(
        &mut self,
        _conversation_id: &str,
        _pending: &links_client_core::mls::PendingCommit,
    ) -> Result<(), CoreError> {
        Ok(())
    }

    fn deliver_mls_bootstrap(
        &mut self,
        conversation_id: &str,
        pending: &links_client_core::mls::PendingCommit,
        transport: &mut dyn DesktopFrameTransport,
    ) -> Result<(), CoreError> {
        let welcome = pending.welcome.as_ref().ok_or(CoreError::Provider)?;
        let recipient_user_id = self
            .active_recipient_user
            .as_deref()
            .ok_or(CoreError::Provider)?;
        let records = self
            .recipients
            .get(recipient_user_id)
            .ok_or(CoreError::Authentication)?;
        for record in records
            .iter()
            .filter(|record| record.device.device_id != self.local_device_id)
        {
            let frame =
                encode_client_frame(v1::client_frame::Body::MlsBootstrap(v1::MlsBootstrap {
                    conversation_id: conversation_id.to_owned(),
                    recipient_device_id: record.device.device_id.clone(),
                    commit: pending.commit.clone(),
                    welcome: welcome.clone(),
                    sender_mls_credential: self.mls_credential.clone(),
                    sender_identity_public_key: self.identity_public_key.to_vec(),
                    reset_group: pending.reset_group,
                }))?;
            self.bootstrap_outbox.push(frame.clone());
            transport.send(&frame)?;
        }
        Ok(())
    }

    fn mark_pending_commit_accepted(&mut self, _conversation_id: &str) -> Result<(), CoreError> {
        Ok(())
    }

    fn persist_send(
        &mut self,
        message: &v1::Message,
        _envelopes: &[v1::Envelope],
        frames: &[Vec<u8>],
        last_sequence_id: u64,
    ) -> Result<(), CoreError> {
        self.sequences
            .insert(message.conversation_id.clone(), last_sequence_id);
        self.outbox.extend(frames.iter().cloned());
        Ok(())
    }

    fn mark_outbox_accepted(&mut self, envelope_id: &str) -> Result<(), CoreError> {
        self.outbox.retain(|frame| {
            v1::ClientFrame::decode(frame.as_slice())
                .ok()
                .and_then(|frame| frame.body)
                .and_then(|body| match body {
                    v1::client_frame::Body::Send(envelope) => Some(envelope.envelope_id),
                    _ => None,
                })
                .as_deref()
                != Some(envelope_id)
        });
        Ok(())
    }

    fn commit_receive(
        &mut self,
        previous_cursor: u64,
        next_cursor: u64,
        _items: &[DesktopInboxItem],
    ) -> Result<(), CoreError> {
        if self.cursor != previous_cursor {
            return Err(CoreError::InvalidSync);
        }
        self.cursor = next_cursor;
        Ok(())
    }
}

struct WebTransport {
    frames: Vec<Vec<u8>>,
}

impl WebTransport {
    fn new() -> Self {
        Self { frames: Vec::new() }
    }
}

impl DesktopFrameTransport for WebTransport {
    fn send(&mut self, frame: &[u8]) -> Result<(), CoreError> {
        if frame.is_empty() || frame.len() > protocol::MAX_FRAME_BYTES {
            return Err(CoreError::Protocol(protocol::ProtocolError::TooLarge));
        }
        self.frames.push(frame.to_vec());
        Ok(())
    }
}

type WebCrypto = SealedSenderCrypto<WebResolver>;
type WebMls = OpenMlsEngine<RustCryptoProvider<MemoryStorage>, WebSigner, WebVerifier>;
type WebBoundCore =
    RustDesktopMessagingCore<WebCrypto, WebMls, DesktopCoreHostAdapter<WebServices>>;

#[wasm_bindgen]
pub struct WebMessagingCore {
    core: WebBoundCore,
    signer: WebSigner,
    vault: WebSecretStore,
    profile: LocalPreKeyProfile,
    outgoing: Vec<Vec<u8>>,
    received: Vec<DesktopReceivedTextMessage>,
}

#[wasm_bindgen]
impl WebMessagingCore {
    #[wasm_bindgen(constructor)]
    pub fn new(
        user_id: &str,
        device_id: &str,
        mls_credential: &[u8],
    ) -> Result<WebMessagingCore, JsValue> {
        let identity = IdentitySeed::generate().map_err(js_error)?;
        Self::from_identity_parts(user_id, device_id, identity, mls_credential)
    }

    /// Build the messaging core from the browser account's Ed25519 seed.
    /// The seed is consumed immediately into the Rust signer and never
    /// returned to JavaScript.
    pub fn from_identity_seed(
        user_id: &str,
        device_id: &str,
        mls_credential: &[u8],
        seed: &[u8],
    ) -> Result<WebMessagingCore, JsValue> {
        let seed: [u8; 32] = seed
            .try_into()
            .map_err(|_| js_error(CoreError::Authentication))?;
        Self::from_identity_parts(
            user_id,
            device_id,
            IdentitySeed::from_vault(Zeroizing::new(seed)),
            mls_credential,
        )
    }

    pub(crate) fn from_identity_parts(
        user_id: &str,
        device_id: &str,
        identity: IdentitySeed,
        mls_credential: &[u8],
    ) -> Result<WebMessagingCore, JsValue> {
        let local =
            LocalIdentity::new(user_id.to_owned(), device_id.to_owned()).map_err(js_error)?;
        let signer = WebSigner {
            identity: Arc::new(identity),
        };
        let identity_public_key = signer.public_key().map_err(js_error)?;
        let mut vault = WebSecretStore::new();
        // The browser rebuilds this in-memory core after reload. Use a
        // time-ordered revision so the server accepts the fresh pre-key
        // profile as a rotation instead of conflicting with revision 1.
        let profile_revision = (wall_clock_ms() as u64)
            .saturating_mul(1024)
            .saturating_add((random_unit() * 1024.0) as u64)
            .min(protocol::MAX_CURSOR)
            .max(1);
        let profile = generate_profile(device_id.to_owned(), profile_revision, &signer, &mut vault)
            .map_err(|error| js_error(format!("browser pre-key profile: {error}")))?;
        let local_private = vault.identity_private(profile.revision).map_err(js_error)?;
        // The account service stores the full TLS BasicCredential. Pull out
        // its application identity before checking the browser signer.
        let credential = Credential::tls_deserialize_exact(mls_credential).map_err(|_| {
            js_error(format!(
                "invalid MLS credential ({} bytes)",
                mls_credential.len()
            ))
        })?;
        let basic = BasicCredential::try_from(credential)
            .map_err(|_| js_error("invalid MLS basic credential"))?;
        let binding = links_identity::parse_mls_basic_identity(basic.identity())
            .map_err(|_| js_error("invalid MLS identity"))?;
        if binding.user_id.to_string() != user_id
            || binding.device_id.to_string() != device_id
            || binding.public_key != identity_public_key
        {
            return Err(js_error("browser identity does not match MLS credential"));
        }
        let verifier = WebVerifier {
            bindings: HashMap::from([(binding.device_id, binding.clone())]),
        };
        let mls = OpenMlsEngine::new(
            RustCryptoProvider::new(MemoryStorage::default()),
            signer.clone(),
            mls_credential,
            verifier,
        )
        .map_err(|error| js_error(format!("browser MLS engine: {error}")))?;
        let client = ClientCore::new(
            local,
            WebCrypto::new(WebResolver::new(device_id.to_owned(), local_private)),
            mls,
        );
        let services = WebServices::new(
            device_id.to_owned(),
            identity_public_key,
            mls_credential.to_vec(),
        );
        let host = DesktopCoreHostAdapter::new(services);
        Ok(Self {
            core: RustDesktopMessagingCore::new(client, host),
            signer,
            vault,
            profile,
            outgoing: Vec::new(),
            received: Vec::new(),
        })
    }

    pub fn user_id(&self) -> String {
        self.core.user_id().to_owned()
    }

    pub fn device_id(&self) -> String {
        self.core.device_id().to_owned()
    }

    pub fn public_key(&self) -> Vec<u8> {
        self.signer.public_key().unwrap_or([0; 32]).to_vec()
    }

    pub fn durable_cursor(&self) -> String {
        self.core.durable_cursor().unwrap_or_default().to_string()
    }

    pub fn create_hello(&mut self, access_token: &str) -> Result<Vec<u8>, JsValue> {
        self.core
            .create_hello(access_token, self.core.durable_cursor().map_err(js_error)?)
            .map_err(js_error)
    }

    pub fn set_recipient(
        &mut self,
        user_id: &str,
        device_id: &str,
        identity_public_key: &[u8],
        prekey_bundle: &[u8],
        _mls_credential: &[u8],
        mls_key_package: &[u8],
    ) -> Result<(), JsValue> {
        let public_key: [u8; 32] = identity_public_key
            .try_into()
            .map_err(|_| js_error(CoreError::Authentication))?;
        let bundle = v1::PreKeyBundle::decode(prekey_bundle)
            .map_err(|_| js_error(CoreError::Authentication))?;
        let device = RecipientDevice::new(
            user_id.to_owned(),
            device_id.to_owned(),
            public_key,
            bundle,
            mls_key_package.to_vec(),
        )
        .map_err(js_error)?;
        self.core
            .host_mut()
            .services_mut()
            .set_recipient(WebRecipient { device });
        Ok(())
    }

    pub fn send_text(
        &mut self,
        conversation_id: &str,
        recipient_user_id: &str,
        text: &str,
    ) -> Result<(), JsValue> {
        let mut transport = WebTransport::new();
        self.core
            .send_text(conversation_id, recipient_user_id, text, &mut transport)
            .map_err(js_error)?;
        self.outgoing.extend(transport.frames);
        Ok(())
    }

    pub fn send_text_to_self(
        &mut self,
        conversation_id: &str,
        recipient_user_id: &str,
        text: &str,
    ) -> Result<(), JsValue> {
        let mut transport = WebTransport::new();
        self.core
            .send_text_to_self(conversation_id, recipient_user_id, text, &mut transport)
            .map_err(js_error)?;
        self.outgoing.extend(transport.frames);
        Ok(())
    }

    pub fn send_file(
        &mut self,
        conversation_id: &str,
        recipient_user_id: &str,
        attachment_id: &str,
        mime_type: &str,
        file_name: &str,
        ciphertext_size_bytes: &str,
        content_key: &[u8],
        nonce: &[u8],
        ciphertext_sha256: &[u8],
    ) -> Result<(), JsValue> {
        let ciphertext_size_bytes = ciphertext_size_bytes.parse::<u64>().map_err(js_error)?;
        let metadata = v1::MediaMetadata {
            attachment_id: attachment_id.to_owned(),
            mime_type: mime_type.to_owned(),
            ciphertext_size_bytes,
            content_key: content_key.to_vec(),
            nonce: nonce.to_vec(),
            ciphertext_sha256: ciphertext_sha256.to_vec(),
            width: None,
            height: None,
            duration_ms: None,
            blur_hash: None,
            opus: None,
            original_size_bytes: None,
            encryption_chunk_bytes: None,
            chunk_cids: Vec::new(),
            file_name: Some(file_name.to_owned()),
        };
        links_client_core::attachments::validate_file_metadata(&metadata).map_err(js_error)?;
        let mut transport = WebTransport::new();
        self.core
            .send_file(
                conversation_id,
                recipient_user_id,
                &metadata,
                &mut transport,
            )
            .map_err(js_error)?;
        self.outgoing.extend(transport.frames);
        Ok(())
    }

    pub fn handle_server_frame(&mut self, frame: &[u8]) -> Result<(), JsValue> {
        self.handle_server_frame_bytes(frame).map_err(js_error)
    }

    fn handle_bootstrap(&mut self, bootstrap: v1::MlsBootstrap) -> Result<(), CoreError> {
        protocol::validate_id(&bootstrap.conversation_id)?;
        if bootstrap.recipient_device_id != self.device_id()
            || bootstrap.commit.is_empty()
            || bootstrap.welcome.is_empty()
            || bootstrap.sender_identity_public_key.len() != 32
        {
            return Err(CoreError::Authentication);
        }
        // Senders put the full TLS BasicCredential in the bootstrap, as the
        // account service stores it. Unwrap it before parsing the identity.
        let credential = Credential::tls_deserialize_exact(&bootstrap.sender_mls_credential)
            .map_err(|_| CoreError::Authentication)?;
        let basic = BasicCredential::try_from(credential).map_err(|_| CoreError::Authentication)?;
        let binding = links_identity::parse_mls_basic_identity(basic.identity())
            .map_err(|_| CoreError::Authentication)?;
        let sender_public_key: [u8; 32] = bootstrap
            .sender_identity_public_key
            .as_slice()
            .try_into()
            .map_err(|_| CoreError::Authentication)?;
        if binding.device_id.to_string() == self.device_id()
            || binding.public_key != sender_public_key
        {
            return Err(CoreError::Authentication);
        }
        if bootstrap.reset_group {
            self.core
                .core_mut()
                .mls_mut()
                .reset_direct_group_from_welcome(&bootstrap.conversation_id, &bootstrap.welcome)?;
        } else if self
            .core
            .core()
            .mls()
            .current_epoch(&bootstrap.conversation_id)
            .is_ok()
        {
            // Existing members advance with the commit. Only newly added
            // devices can join from the Welcome.
            if self
                .core
                .core_mut()
                .mls_mut()
                .process_direct_commit(&bootstrap.conversation_id, &bootstrap.commit)
                .is_err()
            {
                // Replayed bootstrap frames are harmless once this commit is
                // already reflected in the local group.
                self.core
                    .core()
                    .mls()
                    .group_ready(&bootstrap.conversation_id)?;
            }
        } else {
            self.core
                .core_mut()
                .mls_mut()
                .join_direct_group(&bootstrap.conversation_id, &bootstrap.welcome)?;
        }
        self.core
            .host_mut()
            .services_mut()
            .set_device_user(binding.device_id.to_string(), binding.user_id.to_string());
        Ok(())
    }

    pub fn take_outgoing(&mut self) -> js_sys::Array {
        let frames = std::mem::take(&mut self.outgoing);
        let output = js_sys::Array::new();
        for frame in frames {
            let value: JsValue = js_sys::Uint8Array::from(frame.as_slice()).into();
            output.push(&value);
        }
        output
    }

    pub fn take_messages(&mut self) -> String {
        let messages = std::mem::take(&mut self.received);
        serde_json::to_string(
            &messages
                .into_iter()
                .map(|message| {
                    let sender_user_id = self
                        .core
                        .host()
                        .services()
                        .user_for_device(&message.sender_device_id);
                    serde_json::json!({
                        "conversationID": message.conversation_id,
                        "senderDeviceID": message.sender_device_id,
                        "senderUserID": sender_user_id,
                        "text": message.text,
                        "sequenceID": message.sequence_id.to_string(),
                        "sentAtMs": message.sent_at_ms.to_string()
                    })
                })
                .collect::<Vec<_>>(),
        )
        .unwrap_or_else(|_| "[]".to_owned())
    }

    pub fn prekey_upload(&mut self, curve_count: u32, kem_count: u32) -> Result<Vec<u8>, JsValue> {
        generate_upload(
            &self.profile,
            curve_count,
            kem_count,
            &self.signer,
            &mut self.vault,
        )
        .map(|upload| upload.encode_to_vec())
        .map_err(|error| js_error(format!("browser pre-key upload: {error}")))
    }

    pub fn key_package(&self) -> Result<Vec<u8>, JsValue> {
        self.core
            .core()
            .mls()
            .generate_key_package()
            .map_err(|error| js_error(format!("browser MLS key package: {error}")))
    }

    pub fn profile_upload(&self) -> Vec<u8> {
        v1::PreKeyUpload {
            protocol_version: protocol::VERSION,
            device_id: self.device_id(),
            profile_revision: self.profile.revision,
            profile: Some(self.profile.profile.clone()),
            one_time_curve_prekeys: Vec::new(),
            one_time_kem_prekeys: Vec::new(),
            upload_id: Uuid::new_v4().to_string(),
        }
        .encode_to_vec()
    }

    pub fn pending_outgoing_count(&self) -> usize {
        self.outgoing.len()
            + self.core.host().services().outbox.len()
            + self.core.host().services().bootstrap_outbox.len()
    }
}

fn encode_client_frame(body: v1::client_frame::Body) -> Result<Vec<u8>, CoreError> {
    let frame = v1::ClientFrame {
        request_id: Uuid::new_v4().to_string(),
        body: Some(body),
    };
    protocol::validate_id(&frame.request_id)?;
    let mut bytes = Vec::with_capacity(frame.encoded_len());
    frame.encode(&mut bytes).map_err(|_| CoreError::Provider)?;
    if bytes.len() > protocol::MAX_FRAME_BYTES {
        return Err(CoreError::Protocol(protocol::ProtocolError::TooLarge));
    }
    Ok(bytes)
}

fn js_error(error: impl std::fmt::Display) -> JsValue {
    JsValue::from_str(&error.to_string())
}

#[cfg(target_arch = "wasm32")]
fn wall_clock_ms() -> f64 {
    js_sys::Date::now()
}

#[cfg(not(target_arch = "wasm32"))]
fn wall_clock_ms() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as f64)
        .unwrap_or(0.0)
}

#[cfg(target_arch = "wasm32")]
fn random_unit() -> f64 {
    js_sys::Math::random()
}

#[cfg(not(target_arch = "wasm32"))]
fn random_unit() -> f64 {
    f64::from(Uuid::new_v4().as_bytes()[0]) / 256.0
}

/// Native entry points so the Rust core can be exercised without a browser.
impl WebMessagingCore {
    pub fn handle_server_frame_bytes(&mut self, frame: &[u8]) -> Result<(), CoreError> {
        let mut transport = WebTransport::new();
        let mut received = Vec::new();
        let server_frame = v1::ServerFrame::decode(frame).map_err(|_| CoreError::InvalidSync)?;
        if let Some(v1::server_frame::Body::MlsBootstrap(bootstrap)) = server_frame.body {
            self.handle_bootstrap(bootstrap)?;
        } else {
            self.core
                .handle_server_frame(frame, &mut transport, false, &mut |message| {
                    received.push(message)
                })?;
        }
        self.outgoing.extend(transport.frames);
        self.received.extend(received);
        Ok(())
    }

    pub fn take_outgoing_frames(&mut self) -> Vec<Vec<u8>> {
        std::mem::take(&mut self.outgoing)
    }
}
