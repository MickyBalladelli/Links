//! WASM-facing Web client identity and paired-device bootstrap.
//!
//! The Web host owns UI, QR image rendering, browser storage policy, and
//! network adapters. This crate keeps the identity seed in WASM memory and
//! delegates pairing format and MLS credential validation to client-core.

use links_client_core::{
    identity::IdentitySeed,
    pairing::{PairingPayload, PairingRegistrationResponse},
    protocol, CoreError,
};
use uuid::Uuid;
use wasm_bindgen::prelude::*;

fn js_error(error: impl std::fmt::Display) -> JsValue {
    JsValue::from_str(&error.to_string())
}

fn canonical_uuid(value: &str) -> Result<Uuid, CoreError> {
    protocol::validate_id(value)?;
    Uuid::parse_str(value).map_err(|_| CoreError::Authentication)
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
}
