//! Canonical QR payloads for registering an additional device.
//!
//! The payload contains public device metadata, a fresh pairing nonce, and an
//! Ed25519 signature. It never contains a private key, bearer token, or seed.
//! QR libraries should encode/decode the URI text produced here and leave all
//! validation to this module.

use crate::CoreError;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use uuid::Uuid;

pub const PAIRING_URI_PREFIX: &str = "links://connect?";
pub const PAIRING_VERSION: u8 = 1;
pub const PAIRING_NONCE_BYTES: usize = 32;
pub const PAIRING_PUBLIC_KEY_BYTES: usize = 32;
pub const PAIRING_SIGNATURE_BYTES: usize = 64;
pub const MAX_PAIRING_URI_BYTES: usize = 1024;

/// Public information needed by an authenticated device to approve a new
/// physical client. The signature is made by the new device's identity key.
#[derive(Clone, PartialEq, Eq)]
pub struct PairingPayload {
    user_id: Uuid,
    device_id: Uuid,
    mls_node_id: Uuid,
    public_key: [u8; PAIRING_PUBLIC_KEY_BYTES],
    nonce: [u8; PAIRING_NONCE_BYTES],
    signature: [u8; PAIRING_SIGNATURE_BYTES],
}

impl PairingPayload {
    /// Build a payload from a signature produced by the new device's identity
    /// key. Use `signing_transcript` as the exact message to sign.
    pub fn new(
        user_id: Uuid,
        device_id: Uuid,
        mls_node_id: Uuid,
        public_key: [u8; PAIRING_PUBLIC_KEY_BYTES],
        nonce: [u8; PAIRING_NONCE_BYTES],
        signature: [u8; PAIRING_SIGNATURE_BYTES],
    ) -> Result<Self, CoreError> {
        let payload = Self {
            user_id,
            device_id,
            mls_node_id,
            public_key,
            nonce,
            signature,
        };
        payload.validate_fields()?;
        Ok(payload)
    }

    /// Generate and sign a payload when the identity seed is already available
    /// to the caller. Hardware-backed callers should instead sign the returned
    /// `signing_transcript` through their native keystore and call `new`.
    pub fn signed_with_identity(
        user_id: Uuid,
        device_id: Uuid,
        mls_node_id: Uuid,
        nonce: [u8; PAIRING_NONCE_BYTES],
        identity: &links_identity::IdentitySeed,
    ) -> Result<Self, CoreError> {
        let public_key = identity.public_key();
        let unsigned = Self::new(
            user_id,
            device_id,
            mls_node_id,
            public_key,
            nonce,
            [0; PAIRING_SIGNATURE_BYTES],
        )?;
        let signature = identity.sign(&unsigned.signing_transcript()?);
        Self::new(
            user_id,
            device_id,
            mls_node_id,
            public_key,
            nonce,
            signature,
        )
    }

    /// Generate a fresh nonce for one pairing attempt.
    pub fn generate_nonce() -> Result<[u8; PAIRING_NONCE_BYTES], CoreError> {
        let mut nonce = [0u8; PAIRING_NONCE_BYTES];
        getrandom::fill(&mut nonce).map_err(|_| CoreError::Provider)?;
        Ok(nonce)
    }

    /// Return the exact domain-separated bytes the new device must sign.
    pub fn signing_transcript(&self) -> Result<Vec<u8>, CoreError> {
        self.validate_fields()?;
        links_identity::device_pairing_transcript(
            self.user_id,
            self.device_id,
            self.mls_node_id,
            &self.public_key,
            &self.nonce,
        )
        .map_err(|_| CoreError::Authentication)
    }

    /// Verify the signature before displaying or sending the registration
    /// request. This does not authenticate the account; the approving device's
    /// bearer session and the server's account binding still do that.
    pub fn verify(&self) -> Result<(), CoreError> {
        let transcript = self.signing_transcript()?;
        links_identity::verify(&self.public_key, &transcript, &self.signature)
            .map_err(|_| CoreError::Authentication)
    }

    /// Serialize the canonical, QR-friendly URI. Query order and encoding are
    /// fixed so every implementation produces the same text.
    pub fn to_uri(&self) -> Result<String, CoreError> {
        self.validate_fields()?;
        let uri = format!(
            "{PAIRING_URI_PREFIX}v={PAIRING_VERSION}&user_id={}&device_id={}&mls_node_id={}&public_key={}&nonce={}&signature={}",
            self.user_id,
            self.device_id,
            self.mls_node_id,
            encode(&self.public_key),
            encode(&self.nonce),
            encode(&self.signature),
        );
        if uri.len() > MAX_PAIRING_URI_BYTES {
            return Err(CoreError::Authentication);
        }
        Ok(uri)
    }

    /// Parse only the canonical `links://connect?...` form. Percent-encoding,
    /// duplicate fields, fragments, unknown fields, and alternate UUID forms
    /// are rejected to prevent parser disagreement between clients.
    pub fn from_uri(uri: &str) -> Result<Self, CoreError> {
        if uri.len() > MAX_PAIRING_URI_BYTES
            || !uri.is_ascii()
            || !uri.starts_with(PAIRING_URI_PREFIX)
        {
            return Err(CoreError::Authentication);
        }
        let query = &uri[PAIRING_URI_PREFIX.len()..];
        let mut version = None;
        let mut user_id = None;
        let mut device_id = None;
        let mut mls_node_id = None;
        let mut public_key = None;
        let mut nonce = None;
        let mut signature = None;

        for part in query.split('&') {
            let (key, value) = part.split_once('=').ok_or(CoreError::Authentication)?;
            if value.is_empty() {
                return Err(CoreError::Authentication);
            }
            match key {
                "v" if version.is_none() => version = Some(value),
                "user_id" if user_id.is_none() => user_id = Some(value),
                "device_id" if device_id.is_none() => device_id = Some(value),
                "mls_node_id" if mls_node_id.is_none() => mls_node_id = Some(value),
                "public_key" if public_key.is_none() => public_key = Some(value),
                "nonce" if nonce.is_none() => nonce = Some(value),
                "signature" if signature.is_none() => signature = Some(value),
                _ => return Err(CoreError::Authentication),
            }
        }

        if version != Some("1") {
            return Err(CoreError::Authentication);
        }
        let payload = Self::new(
            parse_uuid(user_id.ok_or(CoreError::Authentication)?)?,
            parse_uuid(device_id.ok_or(CoreError::Authentication)?)?,
            parse_uuid(mls_node_id.ok_or(CoreError::Authentication)?)?,
            decode_fixed(public_key.ok_or(CoreError::Authentication)?)?,
            decode_fixed(nonce.ok_or(CoreError::Authentication)?)?,
            decode_fixed(signature.ok_or(CoreError::Authentication)?)?,
        )?;

        if payload.to_uri()?.as_str() != uri {
            return Err(CoreError::Authentication);
        }
        Ok(payload)
    }

    pub fn user_id(&self) -> Uuid {
        self.user_id
    }

    pub fn device_id(&self) -> Uuid {
        self.device_id
    }

    pub fn mls_node_id(&self) -> Uuid {
        self.mls_node_id
    }

    pub fn public_key(&self) -> [u8; PAIRING_PUBLIC_KEY_BYTES] {
        self.public_key
    }

    pub fn nonce(&self) -> [u8; PAIRING_NONCE_BYTES] {
        self.nonce
    }

    pub fn signature(&self) -> [u8; PAIRING_SIGNATURE_BYTES] {
        self.signature
    }

    fn validate_fields(&self) -> Result<(), CoreError> {
        if self.user_id.is_nil() || self.device_id.is_nil() || self.mls_node_id.is_nil() {
            return Err(CoreError::Authentication);
        }
        links_identity::validate_public_key(&self.public_key)
            .map_err(|_| CoreError::Authentication)
    }
}

fn encode(bytes: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(bytes)
}

fn decode_fixed<const N: usize>(value: &str) -> Result<[u8; N], CoreError> {
    if value.contains('=') {
        return Err(CoreError::Authentication);
    }
    let bytes = URL_SAFE_NO_PAD
        .decode(value)
        .map_err(|_| CoreError::Authentication)?;
    bytes
        .try_into()
        .map_err(|_| CoreError::Authentication)
}

fn parse_uuid(value: &str) -> Result<Uuid, CoreError> {
    let uuid = Uuid::parse_str(value).map_err(|_| CoreError::Authentication)?;
    if uuid.is_nil() || uuid.to_string() != value {
        return Err(CoreError::Authentication);
    }
    Ok(uuid)
}
