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
    mls::{MlsCredentialVerifier, OpenMlsEngine, RustCryptoProvider},
    prekeys::{generate_profile, generate_upload, LocalPreKeyProfile, PreKeySecretStore, PreKeySigner, SecretKind},
    protocol::{self, v1},
    send::RecipientDevice,
    sequences::ConversationSequence,
    CoreError,
};
use links_desktop_client::{
    DesktopCoreHostAdapter, DesktopCoreServices, DesktopFrameTransport, DesktopInboxItem,
    DesktopMessagingCore, DesktopReceivedTextMessage, RustDesktopMessagingCore,
};
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
    credential: Vec<u8>,
}

struct WebServices {
    device_id: String,
    identity_public_key: [u8; 32],
    mls_credential: Vec<u8>,
    cursor: u64,
    sequences: HashMap<String, u64>,
    recipients: HashMap<String, Vec<WebRecipient>>,
    active_recipient_user: Option<String>,
    outbox: Vec<Vec<u8>>,
    bootstrap_outbox: Vec<Vec<u8>>,
}

impl WebServices {
    fn new(device_id: String, identity_public_key: [u8; 32], mls_credential: Vec<u8>) -> Self {
        Self {
            device_id,
            identity_public_key,
            mls_credential,
            cursor: 0,
            sequences: HashMap::new(),
            recipients: HashMap::new(),
            active_recipient_user: None,
            outbox: Vec::new(),
            bootstrap_outbox: Vec::new(),
        }
    }

    fn set_recipient(&mut self, recipient: WebRecipient) {
        let user_id = recipient.device.user_id.clone();
        let records = self.recipients.entry(user_id).or_default();
        records.retain(|item| item.device.device_id != recipient.device.device_id);
        records.push(recipient);
    }

    fn take_outbox(&mut self) -> Vec<Vec<u8>> {
        let mut frames = std::mem::take(&mut self.bootstrap_outbox);
        frames.extend(std::mem::take(&mut self.outbox));
        frames
    }
}

impl DesktopCoreServices for WebServices {
    fn durable_cursor(&self) -> Result<u64, CoreError> {
        Ok(self.cursor)
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
        if records.is_empty() || records.len() > MAX_RECIPIENTS {
            return Err(CoreError::Authentication);
        }
        Ok(records.iter().map(|record| record.device.clone()).collect())
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
        for record in records {
            let frame = encode_client_frame(v1::client_frame::Body::MlsBootstrap(
                v1::MlsBootstrap {
                    conversation_id: conversation_id.to_owned(),
                    recipient_device_id: record.device.device_id.clone(),
                    commit: pending.commit.clone(),
                    welcome: welcome.clone(),
                    sender_mls_credential: self.mls_credential.clone(),
                    sender_identity_public_key: self.identity_public_key.to_vec(),
                    reset_group: false,
                },
            ))?;
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
type WebBoundCore = RustDesktopMessagingCore<WebCrypto, WebMls, DesktopCoreHostAdapter<WebServices>>;

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

    pub(crate) fn from_identity_parts(
        user_id: &str,
        device_id: &str,
        identity: IdentitySeed,
        mls_credential: &[u8],
    ) -> Result<WebMessagingCore, JsValue> {
        let local = LocalIdentity::new(user_id.to_owned(), device_id.to_owned()).map_err(js_error)?;
        let signer = WebSigner {
            identity: Arc::new(identity),
        };
        let identity_public_key = signer.public_key().map_err(js_error)?;
        let mut vault = WebSecretStore::new();
        let profile = generate_profile(device_id.to_owned(), 1, &signer, &mut vault)
            .map_err(js_error)?;
        let local_private = vault.identity_private(1).map_err(js_error)?;
        let binding = links_identity::parse_mls_basic_identity(mls_credential)
            .map_err(|_| js_error(CoreError::Authentication))?;
        if binding.user_id.to_string() != user_id
            || binding.device_id.to_string() != device_id
            || binding.public_key != identity_public_key
        {
            return Err(js_error(CoreError::Authentication));
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
        .map_err(js_error)?;
        let client = ClientCore::new(
            local,
            WebCrypto::new(WebResolver::new(device_id.to_owned(), local_private)),
            mls,
        );
        let services = WebServices::new(device_id.to_owned(), identity_public_key, mls_credential.to_vec());
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
        self.core
            .durable_cursor()
            .unwrap_or_default()
            .to_string()
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
        mls_credential: &[u8],
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
            .set_recipient(WebRecipient {
                device,
                credential: mls_credential.to_vec(),
            });
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

    pub fn handle_server_frame(&mut self, frame: &[u8]) -> Result<(), JsValue> {
        let mut transport = WebTransport::new();
        let mut received = Vec::new();
        self.core
            .handle_server_frame(frame, &mut transport, false, &mut |message| {
                received.push(message)
            })
            .map_err(js_error)?;
        self.outgoing.extend(transport.frames);
        self.received.extend(received);
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
                    serde_json::json!({
                        "conversationID": message.conversation_id,
                        "senderDeviceID": message.sender_device_id,
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
        generate_upload(&self.profile, curve_count, kem_count, &self.signer, &mut self.vault)
            .map(|upload| upload.encode_to_vec())
            .map_err(js_error)
    }

    pub fn key_package(&self) -> Result<Vec<u8>, JsValue> {
        self.core
            .core()
            .mls()
            .generate_key_package()
            .map_err(js_error)
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
            + self
                .core
                .host()
                .services()
                .outbox
                .len()
            + self
                .core
                .host()
                .services()
                .bootstrap_outbox
                .len()
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
