//! Platform-neutral desktop client foundation.
//!
//! The desktop UI and OS storage adapters live outside this crate. This crate
//! owns the paired-device identity boundary and hands public registration data,
//! the MLS credential, and a signing handle to `links-client-core`.

mod session;
mod surfaces;
mod call;
mod core;
mod host;

pub use core::{
    bind_desktop_text_session, DesktopCoreHost, RustDesktopMessagingCore,
};
pub use host::{DesktopCoreHostAdapter, DesktopCoreServices, DesktopInboxItem};

pub use call::{
    DesktopCallEpochKey, DesktopCallMediaEngine, DesktopCallMode, DesktopCallMlsKeyProvider,
    DesktopCallPlacement, DesktopCallSession, DesktopCallSignal, DesktopCallSignalKind,
    DesktopCallSignaling, DesktopCallState,
};

pub use session::{
    DesktopAccessTokenProvider, DesktopConnectionManager, DesktopConnectionState,
    DesktopEncryptedImage, DesktopEncryptedLargeFile, DesktopEvent, DesktopFrameResult,
    DesktopFrameTransport, DesktopImageCache, DesktopImageFileCache, DesktopImageMetadata,
    DesktopImageRenderer, DesktopImageUploadReceipt, DesktopImageUploader,
    DesktopLargeFileUploadReceipt, DesktopLargeFileUploader, DesktopMessagingCore,
    DesktopReceivedTextMessage, DesktopSocket, DesktopSocketFactory, DesktopTextSession,
    DESKTOP_HEARTBEAT_INTERVAL_MS, DESKTOP_IMAGE_MAX_CIPHERTEXT_BYTES, DESKTOP_IMAGE_MAX_EDGE,
    DESKTOP_IMAGE_MAX_PLAINTEXT_BYTES, DESKTOP_INITIAL_BACKOFF_MS,
    DESKTOP_LARGE_FILE_CIPHERTEXT_CHUNK_BYTES, DESKTOP_MAX_BACKOFF_MS, DESKTOP_MAX_FRAME_BYTES,
    DESKTOP_MAX_TEXT_BYTES, DESKTOP_STABLE_CONNECTION_MS,
};
pub use surfaces::DesktopSurfaceClient;
pub use links_client_core::surfaces::{SurfaceKind, SurfaceProfile, SurfaceRole};
pub use links_client_core::decentralized::{
    DecentralizedClient, DecentralizedClientPlan, DecentralizedChunkStorage,
    DecentralizedMediaRelay, DecentralizedMediaRoute, DecentralizedTransportAdapter,
};
pub use links_client_core::sandbox::{
    SandboxCryptoGrant, SandboxCryptoOperation, SandboxCryptoRequest, SandboxError,
    SandboxHost, SandboxLimits, SandboxNetworkRequest, SandboxNetworkResponse,
    SandboxNetworkRule, SandboxOutput, SandboxPermissions,
    SandboxRuntime as DesktopMiniAppSandbox,
};

use links_client_core::{
    identity::{IdentitySeed, LocalIdentity},
    pairing::{PairingPayload, PairingRegistrationResponse},
    prekeys::PreKeySigner,
    CoreError,
};
use std::sync::Arc;
use uuid::Uuid;

/// Public desktop identity state safe for a UI or durable metadata adapter.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DesktopClientSnapshot {
    pub user_id: Uuid,
    pub device_id: Uuid,
    pub mls_node_id: Uuid,
    pub registered: bool,
}

/// Inputs required to construct `links-client-core::ClientCore` and its MLS
/// engine. No private seed is included.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DesktopCoreIdentity {
    local_identity: LocalIdentity,
    mls_credential: Vec<u8>,
}

impl DesktopCoreIdentity {
    pub fn local_identity(&self) -> &LocalIdentity {
        &self.local_identity
    }

    pub fn mls_credential(&self) -> &[u8] {
        &self.mls_credential
    }
}

/// Signing handle passed to OpenMLS and PQXDH adapters. It can be cloned by
/// the desktop host without cloning or exporting the underlying seed.
#[derive(Clone)]
pub struct DesktopIdentitySigner {
    identity: Arc<IdentitySeed>,
}

impl PreKeySigner for DesktopIdentitySigner {
    fn public_key(&self) -> Result<[u8; 32], CoreError> {
        Ok(self.identity.public_key())
    }

    fn sign(&self, transcript: &[u8]) -> Result<[u8; 64], CoreError> {
        Ok(self.identity.sign(transcript))
    }
}

/// Desktop companion identity and shared-core binding boundary.
///
/// The seed stays inside this process and is never returned by the API. A
/// desktop application must replace the in-memory lifetime with an audited OS
/// keychain/provider before treating this as production key custody.
pub struct DesktopClient {
    identity: Arc<IdentitySeed>,
    user_id: Uuid,
    device_id: Uuid,
    mls_node_id: Uuid,
    mls_credential: Option<Vec<u8>>,
}

impl DesktopClient {
    /// Create a fresh desktop device/node identity for an existing account.
    pub fn new(user_id: Uuid) -> Result<Self, CoreError> {
        Self::from_seed(
            IdentitySeed::generate().map_err(|_| CoreError::Provider)?,
            user_id,
            fresh_uuid()?,
            fresh_uuid()?,
        )
    }

    /// Restore a seed supplied by an explicit desktop key provider.
    pub fn from_seed(
        identity: IdentitySeed,
        user_id: Uuid,
        device_id: Uuid,
        mls_node_id: Uuid,
    ) -> Result<Self, CoreError> {
        if user_id.is_nil() || device_id.is_nil() || mls_node_id.is_nil() {
            return Err(CoreError::Authentication);
        }
        Ok(Self {
            identity: Arc::new(identity),
            user_id,
            device_id,
            mls_node_id,
            mls_credential: None,
        })
    }

    /// Create a fresh signed pairing URI for approval by an authenticated
    /// mobile device. The URI contains no private key, seed, or bearer token.
    pub fn pairing_uri(&self) -> Result<String, CoreError> {
        let nonce = PairingPayload::generate_nonce()?;
        PairingPayload::signed_with_identity(
            self.user_id,
            self.device_id,
            self.mls_node_id,
            nonce,
            self.identity.as_ref(),
        )
        .and_then(|payload| payload.to_uri())
    }

    pub fn public_key(&self) -> [u8; 32] {
        self.identity.public_key()
    }

    /// Install only the server-validated MLS credential for this exact device.
    pub fn complete_registration(
        &mut self,
        response: PairingRegistrationResponse,
    ) -> Result<(), CoreError> {
        if response.user_id() != self.user_id
            || response.device_id() != self.device_id
            || response.mls_node_id() != self.mls_node_id
            || response.public_key() != self.public_key()
        {
            return Err(CoreError::Authentication);
        }
        self.mls_credential = Some(response.mls_credential().to_vec());
        Ok(())
    }

    pub fn is_registered(&self) -> bool {
        self.mls_credential.is_some()
    }

    pub fn mls_credential(&self) -> Result<&[u8], CoreError> {
        self.mls_credential
            .as_deref()
            .ok_or(CoreError::Authentication)
    }

    /// Return the public identity and credential inputs for ClientCore/MLS.
    pub fn core_identity(&self) -> Result<DesktopCoreIdentity, CoreError> {
        Ok(DesktopCoreIdentity {
            local_identity: LocalIdentity::new(
                self.user_id.to_string(),
                self.device_id.to_string(),
            )?,
            mls_credential: self.mls_credential()?.to_vec(),
        })
    }

    /// Return a cloneable signer handle for OpenMLS and pre-key generation.
    pub fn signer(&self) -> DesktopIdentitySigner {
        DesktopIdentitySigner {
            identity: Arc::clone(&self.identity),
        }
    }

    pub fn snapshot(&self) -> DesktopClientSnapshot {
        DesktopClientSnapshot {
            user_id: self.user_id,
            device_id: self.device_id,
            mls_node_id: self.mls_node_id,
            registered: self.is_registered(),
        }
    }
}

fn fresh_uuid() -> Result<Uuid, CoreError> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).map_err(|_| CoreError::Provider)?;
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Ok(Uuid::from_bytes(bytes))
}
