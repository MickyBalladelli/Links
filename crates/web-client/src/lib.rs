//! WASM-facing Web client identity and paired-device bootstrap.
//!
//! The Web host owns UI, QR image rendering, browser storage policy, and
//! network adapters. This crate keeps the identity seed in WASM memory and
//! delegates pairing format and MLS credential validation to client-core.

use links_client_core::{
    attachments::{decrypt_large_file_chunk, LargeFileEncryptor},
    identity::IdentitySeed,
    pairing::{PairingPayload, PairingRegistrationResponse},
    protocol, CoreError,
    webrtc::{
        decode_server_signal, encode_client_signal, is_server_signal, WebRtcSignal,
        WebRtcSignalKind,
    },
};
use uuid::Uuid;
use wasm_bindgen::prelude::*;

pub use links_client_core::decentralized::{
    DecentralizedClient, DecentralizedClientPlan, DecentralizedChunkStorage,
    DecentralizedMediaRelay, DecentralizedMediaRoute, DecentralizedTransportAdapter,
};

fn js_error(error: impl std::fmt::Display) -> JsValue {
    JsValue::from_str(&error.to_string())
}

/// Private metadata returned after a browser has staged all encrypted chunks.
/// Sizes are decimal strings so JavaScript never loses uint64 precision.
#[wasm_bindgen]
pub struct WebLargeFileMetadata {
    attachment_id: String,
    mime_type: String,
    original_size_bytes: String,
    ciphertext_size_bytes: String,
    content_key: Vec<u8>,
    nonce: Vec<u8>,
    ciphertext_sha256: Vec<u8>,
    width: u32,
    height: u32,
    duration_ms: String,
}

#[wasm_bindgen]
impl WebLargeFileMetadata {
    pub fn attachment_id(&self) -> String {
        self.attachment_id.clone()
    }

    pub fn mime_type(&self) -> String {
        self.mime_type.clone()
    }

    pub fn original_size_bytes(&self) -> String {
        self.original_size_bytes.clone()
    }

    pub fn ciphertext_size_bytes(&self) -> String {
        self.ciphertext_size_bytes.clone()
    }

    pub fn content_key(&self) -> Vec<u8> {
        self.content_key.clone()
    }

    pub fn nonce(&self) -> Vec<u8> {
        self.nonce.clone()
    }

    pub fn ciphertext_sha256(&self) -> Vec<u8> {
        self.ciphertext_sha256.clone()
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn duration_ms(&self) -> String {
        self.duration_ms.clone()
    }
}

/// WASM handle for the same bounded chunk encryptor used by native hosts.
#[wasm_bindgen]
pub struct WebLargeFileEncryptor {
    inner: Option<LargeFileEncryptor>,
}

#[wasm_bindgen]
impl WebLargeFileEncryptor {
    #[wasm_bindgen(constructor)]
    pub fn new(
        attachment_id: &str,
        mime_type: &str,
        width: u32,
        height: u32,
        duration_ms: &str,
    ) -> Result<Self, JsValue> {
        let duration_ms = duration_ms.parse::<u64>().map_err(js_error)?;
        let inner = LargeFileEncryptor::new(
            attachment_id.to_owned(),
            mime_type.to_owned(),
            (width > 0).then_some(width),
            (height > 0).then_some(height),
            (duration_ms > 0).then_some(duration_ms),
        )
        .map_err(js_error)?;
        Ok(Self { inner: Some(inner) })
    }

    pub fn encrypt_chunk(&mut self, plaintext: &[u8]) -> Result<Vec<u8>, JsValue> {
        self.inner
            .as_mut()
            .ok_or_else(|| js_error("large file encryptor is finished"))?
            .encrypt_chunk(plaintext)
            .map_err(js_error)
    }

    pub fn finish(&mut self) -> Result<WebLargeFileMetadata, JsValue> {
        let mut inner = self
            .inner
            .take()
            .ok_or_else(|| js_error("large file encryptor is finished"))?;
        let encrypted = inner.finish().map_err(js_error)?;
        web_large_file_metadata(&encrypted.media).map_err(js_error)
    }
}

/// WASM handle for chunk decryption after private MLS metadata arrives.
#[wasm_bindgen]
pub struct WebLargeFileDecryptor {
    media: links_client_core::protocol::v1::MediaMetadata,
}

#[wasm_bindgen]
impl WebLargeFileDecryptor {
    #[wasm_bindgen(constructor)]
    pub fn new(
        attachment_id: &str,
        mime_type: &str,
        original_size_bytes: &str,
        ciphertext_size_bytes: &str,
        content_key: &[u8],
        nonce: &[u8],
        ciphertext_sha256: &[u8],
        width: u32,
        height: u32,
        duration_ms: &str,
    ) -> Result<Self, JsValue> {
        let original_size_bytes = original_size_bytes.parse::<u64>().map_err(js_error)?;
        let ciphertext_size_bytes = ciphertext_size_bytes.parse::<u64>().map_err(js_error)?;
        let duration_ms = duration_ms.parse::<u64>().map_err(js_error)?;
        let media = links_client_core::protocol::v1::MediaMetadata {
            attachment_id: attachment_id.to_owned(),
            mime_type: mime_type.to_owned(),
            ciphertext_size_bytes,
            content_key: content_key.to_vec(),
            nonce: nonce.to_vec(),
            ciphertext_sha256: ciphertext_sha256.to_vec(),
            width: (width > 0).then_some(width),
            height: (height > 0).then_some(height),
            duration_ms: (duration_ms > 0).then_some(duration_ms),
            blur_hash: None,
            opus: None,
            original_size_bytes: Some(original_size_bytes),
            encryption_chunk_bytes: Some(
                links_client_core::attachments::LARGE_FILE_CIPHERTEXT_CHUNK_BYTES as u32,
            ),
            chunk_cids: Vec::new(),
            file_name: None,
        };
        links_client_core::protocol::validate_media_metadata(&media).map_err(js_error)?;
        Ok(Self { media })
    }

    pub fn decrypt_chunk(&self, chunk_index: &str, ciphertext: &[u8]) -> Result<Vec<u8>, JsValue> {
        let chunk_index = chunk_index.parse::<u64>().map_err(js_error)?;
        decrypt_large_file_chunk(&self.media, chunk_index, ciphertext).map_err(js_error)
    }
}

fn web_large_file_metadata(
    media: &links_client_core::protocol::v1::MediaMetadata,
) -> Result<WebLargeFileMetadata, CoreError> {
    links_client_core::protocol::validate_media_metadata(media)?;
    Ok(WebLargeFileMetadata {
        attachment_id: media.attachment_id.clone(),
        mime_type: media.mime_type.clone(),
        original_size_bytes: media.original_size_bytes.unwrap().to_string(),
        ciphertext_size_bytes: media.ciphertext_size_bytes.to_string(),
        content_key: media.content_key.clone(),
        nonce: media.nonce.clone(),
        ciphertext_sha256: media.ciphertext_sha256.clone(),
        width: media.width.unwrap_or(0),
        height: media.height.unwrap_or(0),
        duration_ms: media.duration_ms.unwrap_or(0).to_string(),
    })
}

fn canonical_uuid(value: &str) -> Result<Uuid, CoreError> {
    protocol::validate_id(value)?;
    Uuid::parse_str(value).map_err(|_| CoreError::Authentication)
}

/// Browser-local self-sovereign identity. The seed is held inside WASM and is
/// never returned to JavaScript; only public keys, signatures and recovery
/// phrases explicitly requested by the host cross the binding.
#[wasm_bindgen]
pub struct WebSelfSovereignIdentity {
    identity: IdentitySeed,
}

/// Server-delivered SDP/ICE signal decoded by the shared Rust protocol.
/// Signaling text is exposed to the WebRTC host, never to the Links server
/// application layer or the encrypted media path.
#[wasm_bindgen]
pub struct WebRtcSignalDelivery {
    inner: links_client_core::webrtc::WebRtcSignalDelivery,
}

#[wasm_bindgen]
impl WebRtcSignalDelivery {
    pub fn request_id(&self) -> String {
        self.inner.request_id.clone()
    }

    pub fn sender_device_id(&self) -> String {
        self.inner.sender_device_id.to_string()
    }

    pub fn session_id(&self) -> String {
        self.inner.signal.session_id.to_string()
    }

    pub fn target_device_id(&self) -> String {
        self.inner.signal.target_device_id.to_string()
    }

    pub fn kind(&self) -> u8 {
        match self.inner.signal.kind {
            WebRtcSignalKind::Offer => 1,
            WebRtcSignalKind::Answer => 2,
            WebRtcSignalKind::IceCandidate => 3,
        }
    }

    pub fn sdp(&self) -> String {
        self.inner.signal.sdp.clone()
    }

    pub fn sdp_mid(&self) -> String {
        self.inner.signal.sdp_mid.clone().unwrap_or_default()
    }

    pub fn sdp_mline_index(&self) -> u32 {
        self.inner.signal.sdp_mline_index.unwrap_or_default()
    }
}

#[wasm_bindgen]
impl WebSelfSovereignIdentity {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Result<Self, JsValue> {
        Ok(Self {
            identity: IdentitySeed::generate().map_err(js_error)?,
        })
    }

    /// Generate a local English 12- or 24-word recovery phrase.
    pub fn generate_recovery_phrase(word_count: u32) -> Result<String, JsValue> {
        links_identity::generate_recovery_mnemonic(word_count as usize)
            .map(|mnemonic| mnemonic.as_str().to_owned())
            .map_err(js_error)
    }

    /// Stable WebAuthn PRF salt for passkey-derived identities.
    pub fn passkey_identity_prf_salt() -> Vec<u8> {
        links_identity::passkey_identity_prf_salt().to_vec()
    }

    /// Restore an identity from a BIP-39 phrase. The phrase remains local.
    pub fn from_recovery_phrase(phrase: &str, passphrase: &str) -> Result<Self, JsValue> {
        let mnemonic = links_identity::RecoveryMnemonic::from_phrase(phrase).map_err(js_error)?;
        Ok(Self {
            identity: mnemonic
                .derive_identity_seed(passphrase)
                .map_err(js_error)?,
        })
    }

    /// Derive an identity from a local WebAuthn PRF result. The PRF must come
    /// from a user-verified passkey operation and must never be sent to Links.
    pub fn from_passkey_prf(prf_output: &[u8]) -> Result<Self, JsValue> {
        Ok(Self {
            identity: IdentitySeed::from_passkey_prf(prf_output).map_err(js_error)?,
        })
    }

    pub fn public_key(&self) -> Vec<u8> {
        self.identity.public_key().to_vec()
    }

    /// Sign a transcript built by the shared protocol helpers.
    pub fn sign(&self, transcript: &[u8]) -> Vec<u8> {
        self.identity.sign(transcript).to_vec()
    }

    pub fn username_registration_signature(
        &self,
        challenge_id: &str,
        handle: &str,
        device_id: &str,
        mls_node_id: &str,
        challenge: &[u8],
        expires_at_ms: u64,
    ) -> Result<Vec<u8>, JsValue> {
        self.username_signature(
            false,
            challenge_id,
            handle,
            device_id,
            mls_node_id,
            challenge,
            expires_at_ms,
        )
    }

    pub fn username_login_signature(
        &self,
        challenge_id: &str,
        handle: &str,
        device_id: &str,
        mls_node_id: &str,
        challenge: &[u8],
        expires_at_ms: u64,
    ) -> Result<Vec<u8>, JsValue> {
        self.username_signature(
            true,
            challenge_id,
            handle,
            device_id,
            mls_node_id,
            challenge,
            expires_at_ms,
        )
    }

    fn username_signature(
        &self,
        login: bool,
        challenge_id: &str,
        handle: &str,
        device_id: &str,
        mls_node_id: &str,
        challenge: &[u8],
        expires_at_ms: u64,
    ) -> Result<Vec<u8>, JsValue> {
        let challenge_id = canonical_uuid(challenge_id).map_err(js_error)?;
        let device_id = canonical_uuid(device_id).map_err(js_error)?;
        let mls_node_id = canonical_uuid(mls_node_id).map_err(js_error)?;
        let challenge: [u8; 32] = challenge
            .try_into()
            .map_err(|_| js_error(CoreError::Authentication))?;
        let transcript = if login {
            links_identity::username_login_transcript(
                challenge_id,
                handle,
                device_id,
                mls_node_id,
                &self.identity.public_key(),
                &challenge,
                expires_at_ms,
            )
        } else {
            links_identity::username_registration_transcript(
                challenge_id,
                handle,
                device_id,
                mls_node_id,
                &self.identity.public_key(),
                &challenge,
                expires_at_ms,
            )
        }
        .map_err(js_error)?;
        Ok(self.identity.sign(&transcript).to_vec())
    }
}

/// Web device identity kept inside the WASM instance.
///
/// The seed is intentionally not exposed to JavaScript. A host may persist
/// only its opaque client record through an audited storage/provider boundary;
/// this initial facade does not export or log key material.
#[wasm_bindgen]
pub struct WebClientIdentity {
    identity: IdentitySeed,
    user_id: Uuid,
    device_id: Uuid,
    mls_node_id: Uuid,
    mls_credential: Option<Vec<u8>>,
}

#[wasm_bindgen]
impl WebClientIdentity {
    /// Create a fresh Web/desktop identity. IDs must be canonical non-nil UUIDs.
    #[wasm_bindgen(constructor)]
    pub fn new(user_id: &str, device_id: &str, mls_node_id: &str) -> Result<Self, JsValue> {
        Ok(Self {
            identity: IdentitySeed::generate().map_err(js_error)?,
            user_id: canonical_uuid(user_id).map_err(js_error)?,
            device_id: canonical_uuid(device_id).map_err(js_error)?,
            mls_node_id: canonical_uuid(mls_node_id).map_err(js_error)?,
            mls_credential: None,
        })
    }

    /// Build a fresh signed `links://connect?...` URI for mobile approval.
    /// Calling this again starts a new attempt with a new nonce.
    pub fn pairing_uri(&self) -> Result<String, JsValue> {
        let nonce = PairingPayload::generate_nonce().map_err(js_error)?;
        PairingPayload::signed_with_identity(
            self.user_id,
            self.device_id,
            self.mls_node_id,
            nonce,
            &self.identity,
        )
        .and_then(|payload| payload.to_uri())
        .map_err(js_error)
    }

    /// Return only the public Ed25519 identity key.
    pub fn public_key(&self) -> Vec<u8> {
        self.identity.public_key().to_vec()
    }

    /// Install the mobile approval response after strict server-response
    /// validation. The MLS credential is public, but it must match this exact
    /// device identity before the Web client uses it.
    pub fn complete_registration(
        &mut self,
        user_id: &str,
        device_id: &str,
        mls_node_id: &str,
        public_key: &[u8],
        mls_credential: &[u8],
    ) -> Result<(), JsValue> {
        let response = PairingRegistrationResponse::new(
            canonical_uuid(user_id).map_err(js_error)?,
            canonical_uuid(device_id).map_err(js_error)?,
            canonical_uuid(mls_node_id).map_err(js_error)?,
            public_key
                .try_into()
                .map_err(|_| js_error(CoreError::Authentication))?,
            mls_credential.to_vec(),
        )
        .map_err(js_error)?;
        if response.user_id() != self.user_id
            || response.device_id() != self.device_id
            || response.mls_node_id() != self.mls_node_id
            || response.public_key() != self.identity.public_key()
        {
            return Err(js_error(CoreError::Authentication));
        }
        self.mls_credential = Some(response.mls_credential().to_vec());
        Ok(())
    }

    pub fn is_registered(&self) -> bool {
        self.mls_credential.is_some()
    }

    /// Return the public MLS BasicCredential for the Web OpenMLS provider.
    pub fn mls_credential(&self) -> Result<Vec<u8>, JsValue> {
        self.mls_credential
            .as_ref()
            .cloned()
            .ok_or_else(|| js_error(CoreError::Authentication))
    }

    pub fn user_id(&self) -> String {
        self.user_id.to_string()
    }

    pub fn device_id(&self) -> String {
        self.device_id.to_string()
    }

    pub fn mls_node_id(&self) -> String {
        self.mls_node_id.to_string()
    }

    /// Encode one authenticated-device WebRTC offer, answer, or ICE signal.
    /// The access token remains in the normal Hello frame, not in SDP.
    pub fn encode_webrtc_signal(
        &self,
        request_id: &str,
        session_id: &str,
        target_device_id: &str,
        kind: u8,
        sdp: &str,
        sdp_mid: &str,
        sdp_mline_index: u32,
    ) -> Result<Vec<u8>, JsValue> {
        let kind = match kind {
            1 => WebRtcSignalKind::Offer,
            2 => WebRtcSignalKind::Answer,
            3 => WebRtcSignalKind::IceCandidate,
            _ => return Err(js_error(CoreError::Authentication)),
        };
        let signal = WebRtcSignal::new(
            canonical_uuid(session_id).map_err(js_error)?,
            canonical_uuid(target_device_id).map_err(js_error)?,
            kind,
            sdp.to_owned(),
            (!sdp_mid.is_empty()).then_some(sdp_mid.to_owned()),
            (sdp_mline_index != 0).then_some(sdp_mline_index),
        )
        .map_err(js_error)?;
        encode_client_signal(request_id, &signal).map_err(js_error)
    }

    /// Decode one server-delivered WebRTC signal. Non-signaling frames are
    /// rejected so the host must dispatch frames by body before calling this.
    pub fn decode_webrtc_signal(&self, frame: &[u8]) -> Result<WebRtcSignalDelivery, JsValue> {
        decode_server_signal(frame)
            .map(|inner| WebRtcSignalDelivery { inner })
            .map_err(js_error)
    }

    pub fn is_webrtc_signal_frame(&self, frame: &[u8]) -> bool {
        is_server_signal(frame)
    }
}
