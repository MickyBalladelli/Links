//! Shared client contract for decentralized transport, storage, and media.
//!
//! Hosts provide network adapters. This module keeps the security rules in one
//! place: envelope bytes stay opaque, encrypted chunks are verified by CID,
//! and media routes must be trusted, fresh, and SFrame-capable.

use crate::{protocol, CoreError};
use async_trait::async_trait;
use links_identity::verify;
use std::sync::Arc;
use thiserror::Error;
use uuid::Uuid;

pub const MAX_DECENTRALIZED_ENDPOINTS: usize = 8;
pub const MAX_DECENTRALIZED_MEDIA_RELAYS: usize = 16;
pub const MAX_DECENTRALIZED_LEASE_MS: u64 = protocol::MAX_SFU_DISCOVERY_TTL_MS;
pub const MEDIA_RELAY_OPEN_MODE: u32 = 1;
pub const MEDIA_RELAY_TOKEN_MODE: u32 = 2;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum DecentralizedError {
    #[error("invalid decentralized client route")]
    InvalidRoute,
    #[error("decentralized transport is unavailable")]
    TransportUnavailable,
    #[error("decentralized storage is unavailable")]
    StorageUnavailable,
    #[error("decentralized relay is not trusted")]
    UntrustedRelay,
    #[error("decentralized relay record is expired")]
    ExpiredRelay,
    #[error("decentralized relay token is invalid")]
    InvalidRelayToken,
    #[error("decentralized media relay is unavailable")]
    MediaUnavailable,
    #[error("decentralized chunk integrity check failed")]
    IntegrityFailure,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DecentralizedMediaRelay {
    pub node_id: String,
    pub region: String,
    pub websocket_url: String,
    pub turn_url: Option<String>,
    pub mode: u32,
    pub price_units_per_minute: u64,
    pub max_bitrate_kbps: u32,
    pub expires_at_ms: u64,
    pub supports_sframe: bool,
    pub public_key: [u8; 32],
    pub signature: [u8; 64],
}

impl DecentralizedMediaRelay {
    /// Verify a DHT record before passing it to the native/browser media host.
    pub fn from_signed_record(
        record: &protocol::v1::MediaRelayRecord,
        trusted_public_key: &[u8; 32],
        now_ms: u64,
    ) -> Result<Self, CoreError> {
        protocol::validate_media_relay_record(record)?;
        let public_key: [u8; 32] = record
            .public_key
            .as_slice()
            .try_into()
            .map_err(|_| CoreError::Authentication)?;
        let signature: [u8; 64] = record
            .signature
            .as_slice()
            .try_into()
            .map_err(|_| CoreError::Authentication)?;
        if &public_key != trusted_public_key {
            return Err(CoreError::Authentication);
        }
        if record.expires_at_ms <= now_ms
            || record.expires_at_ms - now_ms > MAX_DECENTRALIZED_LEASE_MS
        {
            return Err(CoreError::Decentralized(DecentralizedError::ExpiredRelay));
        }
        verify(
            trusted_public_key,
            &protocol::media_relay_record_signature_transcript(record),
            &record.signature,
        )
        .map_err(|_| CoreError::Decentralized(DecentralizedError::UntrustedRelay))?;
        Ok(Self {
            node_id: record.node_id.clone(),
            region: record.region.clone(),
            websocket_url: record.websocket_url.clone(),
            turn_url: (!record.turn_url.is_empty()).then_some(record.turn_url.clone()),
            mode: record.mode,
            price_units_per_minute: record.price_units_per_minute,
            max_bitrate_kbps: record.max_bitrate_kbps,
            expires_at_ms: record.expires_at_ms,
            supports_sframe: record.supports_sframe,
            public_key,
            signature,
        })
    }

    pub fn requires_token(&self) -> bool {
        self.mode == MEDIA_RELAY_TOKEN_MODE
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DecentralizedClientPlan {
    pub region: String,
    pub transport_endpoints: Vec<String>,
    pub storage_gateways: Vec<String>,
    pub media_relays: Vec<DecentralizedMediaRelay>,
}

impl DecentralizedClientPlan {
    pub fn new(
        region: String,
        transport_endpoints: Vec<String>,
        storage_gateways: Vec<String>,
        media_relays: Vec<DecentralizedMediaRelay>,
    ) -> Result<Self, CoreError> {
        protocol::validate_gateway_locator(&region)
            .map_err(|_| CoreError::Decentralized(DecentralizedError::InvalidRoute))?;
        if transport_endpoints.is_empty()
            || transport_endpoints.len() > MAX_DECENTRALIZED_ENDPOINTS
            || storage_gateways.is_empty()
            || storage_gateways.len() > MAX_DECENTRALIZED_ENDPOINTS
            || media_relays.len() > MAX_DECENTRALIZED_MEDIA_RELAYS
            || media_relays.iter().any(|relay| {
                protocol::validate_gateway_locator(&relay.node_id).is_err()
                    || !valid_endpoint(&relay.websocket_url, "wss://")
                    || relay.turn_url.as_deref().is_some_and(|url| !valid_turn_url(url))
                    || relay.region != region
                    || relay.mode != MEDIA_RELAY_OPEN_MODE
                        && relay.mode != MEDIA_RELAY_TOKEN_MODE
                    || relay.mode == MEDIA_RELAY_OPEN_MODE
                        && relay.price_units_per_minute != 0
                    || relay.mode == MEDIA_RELAY_TOKEN_MODE
                        && relay.price_units_per_minute == 0
                    || relay.max_bitrate_kbps == 0
                    || relay.expires_at_ms == 0
                    || !relay.supports_sframe
                    || relay.public_key.iter().all(|byte| *byte == 0)
            })
            || transport_endpoints.iter().any(|endpoint| !valid_endpoint(endpoint, "wss://"))
            || storage_gateways.iter().any(|gateway| !valid_endpoint(gateway, "https://"))
        {
            return Err(CoreError::Decentralized(DecentralizedError::InvalidRoute));
        }
        Ok(Self {
            region,
            transport_endpoints,
            storage_gateways,
            media_relays,
        })
    }

    pub fn transport_endpoint(&self, attempt: usize) -> &str {
        &self.transport_endpoints[attempt % self.transport_endpoints.len()]
    }

    pub fn storage_gateway(&self, attempt: usize) -> &str {
        &self.storage_gateways[attempt % self.storage_gateways.len()]
    }

    /// Select open relays first. Token relay admission remains bound to the
    /// selected node and opaque session; the authority verifies the signature.
    pub fn select_media_relay(
        &self,
        session_id: Uuid,
        duration_ms: u64,
        access_token: Option<Vec<u8>>,
        now_ms: u64,
    ) -> Result<DecentralizedMediaRoute, CoreError> {
        if session_id.is_nil()
            || duration_ms == 0
            || duration_ms > MAX_DECENTRALIZED_LEASE_MS
        {
            return Err(CoreError::Decentralized(DecentralizedError::InvalidRoute));
        }
        if let Some(relay) = self
            .media_relays
            .iter()
            .find(|relay| {
                relay.mode == MEDIA_RELAY_OPEN_MODE && relay.expires_at_ms > now_ms
            })
        {
            return Ok(DecentralizedMediaRoute {
                relay: relay.clone(),
                session_id,
                duration_ms,
                access_token: None,
            });
        }
        let token = access_token.filter(|token| !token.is_empty());
        for relay in &self.media_relays {
            if relay.mode != MEDIA_RELAY_TOKEN_MODE {
                continue;
            }
            if relay.expires_at_ms <= now_ms {
                continue;
            }
            let Some(token) = token.as_ref() else { continue };
            let wire = protocol::decode_media_relay_access_token(token)
                .map_err(|_| CoreError::Decentralized(DecentralizedError::InvalidRelayToken))?;
            if wire.relay_node_id != relay.node_id
                || wire.session_id != session_id.hyphenated().to_string()
                || wire.expires_at_ms <= now_ms
                || wire.max_duration_ms < duration_ms
            {
                continue;
            }
            return Ok(DecentralizedMediaRoute {
                relay: relay.clone(),
                session_id,
                duration_ms,
                access_token: Some(token.clone()),
            });
        }
        Err(CoreError::Decentralized(DecentralizedError::MediaUnavailable))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DecentralizedMediaRoute {
    pub relay: DecentralizedMediaRelay,
    pub session_id: Uuid,
    pub duration_ms: u64,
    pub access_token: Option<Vec<u8>>,
}

#[async_trait]
pub trait DecentralizedTransportAdapter: Send {
    async fn publish_opaque(&mut self, endpoint: &str, envelope: &[u8]) -> Result<(), CoreError>;
    async fn replay_opaque(
        &mut self,
        endpoint: &str,
        after_cursor: u64,
        limit: u32,
    ) -> Result<Vec<Vec<u8>>, CoreError>;
}

#[async_trait]
pub trait DecentralizedChunkStorage: Send + Sync {
    async fn upload_ciphertext(
        &self,
        gateway: &str,
        cid: &str,
        ciphertext: &[u8],
    ) -> Result<(), CoreError>;
    async fn download_ciphertext(&self, gateway: &str, cid: &str)
        -> Result<Vec<u8>, CoreError>;
}

pub struct DecentralizedClient<T, S> {
    plan: DecentralizedClientPlan,
    transport: T,
    storage: Arc<S>,
}

impl<T, S> DecentralizedClient<T, S> {
    pub fn new(plan: DecentralizedClientPlan, transport: T, storage: Arc<S>) -> Self {
        Self {
            plan,
            transport,
            storage,
        }
    }

    pub fn plan(&self) -> &DecentralizedClientPlan {
        &self.plan
    }

    pub fn transport_mut(&mut self) -> &mut T {
        &mut self.transport
    }
}

impl<T, S> DecentralizedClient<T, S>
where
    T: DecentralizedTransportAdapter,
    S: DecentralizedChunkStorage + 'static,
{
    pub async fn publish_opaque_envelope(&mut self, envelope: &[u8]) -> Result<(), CoreError> {
        if envelope.is_empty() || envelope.len() > protocol::MAX_ENVELOPE_BYTES {
            return Err(CoreError::Decentralized(DecentralizedError::InvalidRoute));
        }
        for attempt in 0..self.plan.transport_endpoints.len() {
            if self
                .transport
                .publish_opaque(self.plan.transport_endpoint(attempt), envelope)
                .await
                .is_ok()
            {
                return Ok(());
            }
        }
        Err(CoreError::Decentralized(DecentralizedError::TransportUnavailable))
    }

    pub async fn replay_opaque(
        &mut self,
        after_cursor: u64,
        limit: u32,
    ) -> Result<Vec<Vec<u8>>, CoreError> {
        if after_cursor > protocol::MAX_CURSOR
            || limit == 0
            || limit as usize > protocol::MAX_BATCH_ITEMS
        {
            return Err(CoreError::InvalidSync);
        }
        for attempt in 0..self.plan.transport_endpoints.len() {
            let batch = match self
                .transport
                .replay_opaque(self.plan.transport_endpoint(attempt), after_cursor, limit)
                .await
            {
                Ok(batch) => batch,
                Err(_) => continue,
            };
            if batch.len() > limit as usize
                || batch.iter().any(|bytes| protocol::decode_envelope(bytes).is_err())
            {
                return Err(CoreError::InvalidSync);
            }
            return Ok(batch);
        }
        Err(CoreError::Decentralized(DecentralizedError::TransportUnavailable))
    }

    pub async fn upload_ciphertext_chunk(
        &self,
        cid: &str,
        ciphertext: &[u8],
    ) -> Result<(), CoreError> {
        protocol::content_addressed::verify_content_cid(cid, ciphertext)
            .map_err(|_| CoreError::Decentralized(DecentralizedError::IntegrityFailure))?;
        for attempt in 0..self.plan.storage_gateways.len() {
            if self
                .storage
                .upload_ciphertext(
                    self.plan.storage_gateway(attempt),
                    cid,
                    ciphertext,
                )
                .await
                .is_ok()
            {
                return Ok(());
            }
        }
        Err(CoreError::Decentralized(DecentralizedError::StorageUnavailable))
    }

    pub async fn download_ciphertext_chunk(&self, cid: &str) -> Result<Vec<u8>, CoreError> {
        protocol::content_addressed::validate_content_cid(cid)?;
        for attempt in 0..self.plan.storage_gateways.len() {
            let ciphertext = match self
                .storage
                .download_ciphertext(self.plan.storage_gateway(attempt), cid)
                .await
            {
                Ok(ciphertext) => ciphertext,
                Err(_) => continue,
            };
            if protocol::content_addressed::verify_content_cid(cid, &ciphertext).is_ok() {
                return Ok(ciphertext);
            }
            return Err(CoreError::Decentralized(DecentralizedError::IntegrityFailure));
        }
        Err(CoreError::Decentralized(DecentralizedError::StorageUnavailable))
    }
}

fn valid_endpoint(value: &str, scheme: &str) -> bool {
    !value.is_empty()
        && value.len() <= 512
        && value.starts_with(scheme)
        && !value
            .bytes()
            .any(|byte| byte.is_ascii_control() || byte.is_ascii_whitespace() || byte == b'#')
}

fn valid_turn_url(value: &str) -> bool {
    value.len() <= 512
        && (value.starts_with("turn:") || value.starts_with("turns:"))
        && !value
            .bytes()
            .any(|byte| byte.is_ascii_control() || byte.is_ascii_whitespace() || byte == b'#')
}
