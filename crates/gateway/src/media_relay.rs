//! Open and token-incentivized media relay routing for WebRTC calls.
//!
//! Relay records are signed and discoverable through the existing region-scoped
//! DHT boundary. Open relays need no admission token. Token relays require a
//! short-lived, relay-bound credit token and return a signed usage receipt.
//! Both modes use the same SFrame-only media policy: relays forward RTP
//! headers, but never receive media keys or decrypt encoded media.

use super::sfu::{SfuError, SfuMediaPolicy};
use super::sfu_discovery::{SfuDhtClient, SfuHealthCache, SfuTrustDirectory};
use ed25519_dalek::{Signature, VerifyingKey};
use links_protocol::{self as protocol, v1};
use prost::Message;
use sha2::{Digest, Sha256};
use std::{collections::HashMap, fmt};
use std::sync::{Arc, Mutex};
use thiserror::Error;
use uuid::Uuid;

pub const MEDIA_RELAY_OPEN_MODE: u32 = 1;
pub const MEDIA_RELAY_TOKEN_MODE: u32 = 2;
pub const MEDIA_RELAY_DHT_KEY_DOMAIN: &[u8] = b"links/media-relay/dht-key/v1\0";
pub const MEDIA_RELAY_RECORD_SIGNATURE_DOMAIN: &[u8] =
    b"links/media-relay/record-signature/v1\0";
pub const MEDIA_RELAY_TOKEN_SIGNATURE_DOMAIN: &[u8] =
    b"links/media-relay/token-signature/v1\0";
pub const MEDIA_RELAY_USAGE_SIGNATURE_DOMAIN: &[u8] = b"links/media-relay/usage/v1\0";
pub const MAX_MEDIA_RELAY_LEASE_MS: u64 = protocol::MAX_SFU_DISCOVERY_TTL_MS;
pub const MAX_MEDIA_RELAY_LOOKUP_RESULTS: usize = protocol::MAX_SFU_DISCOVERY_RESULTS;
pub const CREDIT_UNIT_MS: u64 = 60_000;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum MediaRelayError {
    #[error("invalid media relay input")]
    Invalid,
    #[error("media relay DHT is unavailable")]
    DhtUnavailable,
    #[error("media relay node is not trusted")]
    UntrustedNode,
    #[error("media relay signature is invalid")]
    InvalidSignature,
    #[error("media relay record is expired")]
    Expired,
    #[error("no eligible media relay is available")]
    Unavailable,
    #[error("media relay access token is invalid")]
    InvalidToken,
    #[error("media relay credit is insufficient")]
    InsufficientCredit,
    #[error(transparent)]
    Protocol(#[from] protocol::ProtocolError),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MediaRelayMode {
    Open,
    TokenIncentivized,
}

impl MediaRelayMode {
    fn from_wire(value: u32) -> Result<Self, MediaRelayError> {
        match value {
            MEDIA_RELAY_OPEN_MODE => Ok(Self::Open),
            MEDIA_RELAY_TOKEN_MODE => Ok(Self::TokenIncentivized),
            _ => Err(MediaRelayError::Invalid),
        }
    }

    fn wire(self) -> u32 {
        match self {
            Self::Open => MEDIA_RELAY_OPEN_MODE,
            Self::TokenIncentivized => MEDIA_RELAY_TOKEN_MODE,
        }
    }
}

pub trait MediaRelayNodeSigner: Send + Sync {
    fn public_key(&self) -> [u8; 32];
    fn sign(&self, transcript: &[u8]) -> Result<[u8; 64], MediaRelayError>;
}

pub trait RelayTokenSigner: Send + Sync {
    fn sign(&self, transcript: &[u8]) -> Result<[u8; 64], MediaRelayError>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MediaRelayRecord {
    node_id: String,
    region: String,
    websocket_url: String,
    turn_url: Option<String>,
    public_key: [u8; 32],
    sequence: u64,
    expires_at_ms: u64,
    mode: MediaRelayMode,
    price_units_per_minute: u64,
    max_bitrate_kbps: u32,
    supports_sframe: bool,
    signature: [u8; 64],
}

impl MediaRelayRecord {
    #[allow(clippy::too_many_arguments)]
    pub fn sign<S: MediaRelayNodeSigner + ?Sized>(
        node_id: String,
        region: String,
        websocket_url: String,
        turn_url: Option<String>,
        sequence: u64,
        expires_at_ms: u64,
        mode: MediaRelayMode,
        price_units_per_minute: u64,
        max_bitrate_kbps: u32,
        signer: &S,
        now_ms: u64,
    ) -> Result<Self, MediaRelayError> {
        let public_key = signer.public_key();
        let record = Self {
            node_id,
            region,
            websocket_url,
            turn_url,
            public_key,
            sequence,
            expires_at_ms,
            mode,
            price_units_per_minute,
            max_bitrate_kbps,
            supports_sframe: true,
            signature: [0; 64],
        };
        record.validate_unsigned(now_ms)?;
        let signature = signer.sign(&record_transcript(&record))?;
        let record = Self { signature, ..record };
        record.validate(now_ms)?;
        Ok(record)
    }

    pub fn decode_and_verify(
        bytes: &[u8],
        expected_public_key: &[u8; 32],
        now_ms: u64,
    ) -> Result<Self, MediaRelayError> {
        let wire = protocol::decode_media_relay_record(bytes)?;
        let public_key: [u8; 32] = wire
            .public_key
            .as_slice()
            .try_into()
            .map_err(|_| MediaRelayError::InvalidSignature)?;
        if &public_key != expected_public_key {
            return Err(MediaRelayError::UntrustedNode);
        }
        let signature: [u8; 64] = wire
            .signature
            .as_slice()
            .try_into()
            .map_err(|_| MediaRelayError::InvalidSignature)?;
        let record = Self {
            node_id: wire.node_id,
            region: wire.region,
            websocket_url: wire.websocket_url,
            turn_url: (!wire.turn_url.is_empty()).then_some(wire.turn_url),
            public_key,
            sequence: wire.sequence,
            expires_at_ms: wire.expires_at_ms,
            mode: MediaRelayMode::from_wire(wire.mode)?,
            price_units_per_minute: wire.price_units_per_minute,
            max_bitrate_kbps: wire.max_bitrate_kbps,
            supports_sframe: wire.supports_sframe,
            signature,
        };
        record.validate(now_ms)?;
        let verifying_key =
            VerifyingKey::from_bytes(&public_key).map_err(|_| MediaRelayError::InvalidSignature)?;
        verifying_key
            .verify_strict(&record_transcript(&record), &Signature::from_bytes(&signature))
            .map_err(|_| MediaRelayError::InvalidSignature)?;
        Ok(record)
    }

    pub fn encode(&self) -> Result<Vec<u8>, MediaRelayError> {
        Ok(self.to_wire()?.encode_to_vec())
    }

    pub fn node_id(&self) -> &str {
        &self.node_id
    }

    pub fn region(&self) -> &str {
        &self.region
    }

    pub fn websocket_url(&self) -> &str {
        &self.websocket_url
    }

    pub fn turn_url(&self) -> Option<&str> {
        self.turn_url.as_deref()
    }

    pub fn public_key(&self) -> &[u8; 32] {
        &self.public_key
    }

    pub fn sequence(&self) -> u64 {
        self.sequence
    }

    pub fn expires_at_ms(&self) -> u64 {
        self.expires_at_ms
    }

    pub fn mode(&self) -> MediaRelayMode {
        self.mode
    }

    pub fn price_units_per_minute(&self) -> u64 {
        self.price_units_per_minute
    }

    pub fn max_bitrate_kbps(&self) -> u32 {
        self.max_bitrate_kbps
    }

    pub fn supports_sframe(&self) -> bool {
        self.supports_sframe
    }

    pub fn media_policy(&self) -> SfuMediaPolicy {
        SfuMediaPolicy::encrypted_sframe()
    }

    fn validate_unsigned(&self, now_ms: u64) -> Result<(), MediaRelayError> {
        protocol::validate_gateway_locator(&self.node_id).map_err(|_| MediaRelayError::Invalid)?;
        protocol::validate_gateway_locator(&self.region).map_err(|_| MediaRelayError::Invalid)?;
        if !valid_websocket_url(&self.websocket_url)
            || self.turn_url.as_deref().is_some_and(|url| !valid_turn_url(url))
        {
            return Err(MediaRelayError::Invalid);
        }
        if self.public_key.iter().all(|byte| *byte == 0) || self.sequence == 0 {
            return Err(MediaRelayError::Invalid);
        }
        if self.expires_at_ms <= now_ms
            || self.expires_at_ms - now_ms > MAX_MEDIA_RELAY_LEASE_MS
        {
            return Err(MediaRelayError::Expired);
        }
        if self.mode == MediaRelayMode::Open {
            if self.price_units_per_minute != 0 {
                return Err(MediaRelayError::Invalid);
            }
        } else if self.price_units_per_minute == 0 {
            return Err(MediaRelayError::Invalid);
        }
        if self.max_bitrate_kbps == 0 || !self.supports_sframe {
            return Err(MediaRelayError::Invalid);
        }
        Ok(())
    }

    fn validate(&self, now_ms: u64) -> Result<(), MediaRelayError> {
        self.validate_unsigned(now_ms)?;
        protocol::validate_media_relay_record(&self.to_wire()?)?;
        Ok(())
    }

    fn to_wire(&self) -> Result<v1::MediaRelayRecord, MediaRelayError> {
        let record = v1::MediaRelayRecord {
            protocol_version: protocol::VERSION,
            node_id: self.node_id.clone(),
            region: self.region.clone(),
            websocket_url: self.websocket_url.clone(),
            turn_url: self.turn_url.clone().unwrap_or_default(),
            public_key: self.public_key.to_vec(),
            sequence: self.sequence,
            expires_at_ms: self.expires_at_ms,
            mode: self.mode.wire(),
            price_units_per_minute: self.price_units_per_minute,
            max_bitrate_kbps: self.max_bitrate_kbps,
            supports_sframe: self.supports_sframe,
            signature: self.signature.to_vec(),
        };
        protocol::validate_media_relay_record(&record)?;
        Ok(record)
    }
}

fn valid_websocket_url(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 512
        && value.starts_with("wss://")
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

fn append_field(transcript: &mut Vec<u8>, value: &[u8]) {
    transcript.extend_from_slice(&(value.len() as u32).to_be_bytes());
    transcript.extend_from_slice(value);
}

fn record_transcript(record: &MediaRelayRecord) -> Vec<u8> {
    let mut transcript = MEDIA_RELAY_RECORD_SIGNATURE_DOMAIN.to_vec();
    append_field(&mut transcript, record.node_id.as_bytes());
    append_field(&mut transcript, record.region.as_bytes());
    append_field(&mut transcript, record.websocket_url.as_bytes());
    append_field(
        &mut transcript,
        record.turn_url.as_deref().unwrap_or_default().as_bytes(),
    );
    append_field(&mut transcript, &record.public_key);
    transcript.extend_from_slice(&record.sequence.to_be_bytes());
    transcript.extend_from_slice(&record.expires_at_ms.to_be_bytes());
    transcript.extend_from_slice(&record.mode.wire().to_be_bytes());
    transcript.extend_from_slice(&record.price_units_per_minute.to_be_bytes());
    transcript.extend_from_slice(&record.max_bitrate_kbps.to_be_bytes());
    transcript.push(u8::from(record.supports_sframe));
    transcript
}

pub fn media_relay_dht_key(region: &str) -> Result<[u8; 32], MediaRelayError> {
    protocol::validate_gateway_locator(region).map_err(|_| MediaRelayError::Invalid)?;
    let mut hasher = Sha256::new();
    hasher.update(MEDIA_RELAY_DHT_KEY_DOMAIN);
    hasher.update(region.as_bytes());
    let digest = hasher.finalize();
    let mut key = [0; 32];
    key.copy_from_slice(&digest);
    Ok(key)
}

pub struct DhtMediaRelayDiscovery<D> {
    dht: Arc<D>,
    trust: Arc<SfuTrustDirectory>,
    health: Arc<SfuHealthCache>,
}

impl<D> DhtMediaRelayDiscovery<D>
where
    D: SfuDhtClient + 'static,
{
    pub fn new(
        dht: Arc<D>,
        trust: Arc<SfuTrustDirectory>,
        health: Arc<SfuHealthCache>,
    ) -> Self {
        Self { dht, trust, health }
    }

    pub fn health_cache(&self) -> Arc<SfuHealthCache> {
        Arc::clone(&self.health)
    }

    pub async fn publish(
        &self,
        record: &MediaRelayRecord,
        now_ms: u64,
    ) -> Result<(), MediaRelayError> {
        let expected_key = self
            .trust
            .key_for(record.node_id())
            .ok_or(MediaRelayError::UntrustedNode)?;
        if expected_key != record.public_key() {
            return Err(MediaRelayError::UntrustedNode);
        }
        record.validate(now_ms)?;
        self.dht
            .put(media_relay_dht_key(record.region())?, record.encode()?)
            .await
            .map_err(map_dht_error)
    }

    pub async fn discover_healthy(
        &self,
        region: &str,
        now_ms: u64,
    ) -> Result<Vec<MediaRelayRecord>, MediaRelayError> {
        let bytes = self
            .dht
            .get(media_relay_dht_key(region)?, MAX_MEDIA_RELAY_LOOKUP_RESULTS)
            .await
            .map_err(map_dht_error)?;
        let mut highest: HashMap<String, MediaRelayRecord> = HashMap::new();
        for bytes in bytes.into_iter().take(MAX_MEDIA_RELAY_LOOKUP_RESULTS) {
            let wire = match protocol::decode_media_relay_record(&bytes) {
                Ok(record) => record,
                Err(_) => continue,
            };
            if wire.region != region {
                continue;
            }
            let expected_key = match self.trust.key_for(&wire.node_id) {
                Some(key) => key,
                None => continue,
            };
            let record = match MediaRelayRecord::decode_and_verify(&bytes, expected_key, now_ms) {
                Ok(record) => record,
                Err(_) => continue,
            };
            if !self.health.is_healthy(record.node_id(), now_ms) {
                continue;
            }
            let replace = highest
                .get(record.node_id())
                .is_none_or(|existing| record.sequence() > existing.sequence());
            if replace {
                highest.insert(record.node_id().to_owned(), record);
            }
        }
        let mut records: Vec<_> = highest.into_values().collect();
        records.sort_by(|left, right| {
            right
                .sequence()
                .cmp(&left.sequence())
                .then_with(|| left.node_id().cmp(right.node_id()))
        });
        if records.is_empty() {
            return Err(MediaRelayError::Unavailable);
        }
        Ok(records)
    }
}

fn map_dht_error(error: SfuError) -> MediaRelayError {
    match error {
        SfuError::Protocol(error) => MediaRelayError::Protocol(error),
        SfuError::UntrustedNode => MediaRelayError::UntrustedNode,
        SfuError::ExpiredRecord => MediaRelayError::Expired,
        _ => MediaRelayError::DhtUnavailable,
    }
}

/// Bounded local relay-DHT reference implementation. Production deployments
/// should provide an authenticated Kademlia or relay-directory adapter for
/// `SfuDhtClient` instead.
pub struct MemoryMediaRelayDht {
    records: Mutex<HashMap<[u8; 32], Vec<Vec<u8>>>>,
    max_records_per_key: usize,
}

impl MemoryMediaRelayDht {
    pub fn new(max_records_per_key: usize) -> Self {
        Self {
            records: Mutex::new(HashMap::new()),
            max_records_per_key: max_records_per_key.max(1),
        }
    }
}

#[async_trait::async_trait]
impl SfuDhtClient for MemoryMediaRelayDht {
    async fn put(&self, key: [u8; 32], record: Vec<u8>) -> Result<(), SfuError> {
        let decoded = protocol::decode_media_relay_record(&record)?;
        let expected_key = media_relay_dht_key(&decoded.region).map_err(|_| SfuError::Invalid)?;
        if expected_key != key {
            return Err(SfuError::Invalid);
        }
        let mut records = self.records.lock().map_err(|_| SfuError::DhtUnavailable)?;
        let bucket = records.entry(key).or_default();
        for existing_bytes in bucket.iter_mut() {
            let existing = protocol::decode_media_relay_record(existing_bytes)?;
            if existing.node_id == decoded.node_id {
                if decoded.sequence >= existing.sequence {
                    *existing_bytes = record;
                }
                return Ok(());
            }
        }
        if bucket.len() >= self.max_records_per_key {
            return Err(SfuError::DhtUnavailable);
        }
        bucket.push(record);
        Ok(())
    }

    async fn get(&self, key: [u8; 32], limit: usize) -> Result<Vec<Vec<u8>>, SfuError> {
        if limit == 0 || limit > MAX_MEDIA_RELAY_LOOKUP_RESULTS {
            return Err(SfuError::Invalid);
        }
        let records = self.records.lock().map_err(|_| SfuError::DhtUnavailable)?;
        Ok(records
            .get(&key)
            .into_iter()
            .flat_map(|bucket| bucket.iter().take(limit).cloned())
            .collect())
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct MediaRelayAccessToken {
    token_id: Uuid,
    relay_node_id: String,
    session_id: Uuid,
    expires_at_ms: u64,
    max_bytes: u64,
    max_duration_ms: u64,
    credit_units: u64,
    signature: [u8; 64],
}

impl fmt::Debug for MediaRelayAccessToken {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("MediaRelayAccessToken(REDACTED)")
    }
}

impl MediaRelayAccessToken {
    #[allow(clippy::too_many_arguments)]
    pub fn issue<S: RelayTokenSigner + ?Sized>(
        token_id: Uuid,
        relay_node_id: String,
        session_id: Uuid,
        expires_at_ms: u64,
        max_bytes: u64,
        max_duration_ms: u64,
        credit_units: u64,
        signer: &S,
        now_ms: u64,
    ) -> Result<Self, MediaRelayError> {
        let token = Self {
            token_id,
            relay_node_id,
            session_id,
            expires_at_ms,
            max_bytes,
            max_duration_ms,
            credit_units,
            signature: [0; 64],
        };
        token.validate(now_ms)?;
        let signature = signer.sign(&token_transcript(&token))?;
        let token = Self { signature, ..token };
        token.validate_wire()?;
        Ok(token)
    }

    pub fn decode_and_verify(
        bytes: &[u8],
        authority_public_key: &[u8; 32],
        expected_relay_node_id: &str,
        expected_session_id: Uuid,
        now_ms: u64,
    ) -> Result<Self, MediaRelayError> {
        let wire = protocol::decode_media_relay_access_token(bytes)?;
        let token = Self::from_wire(wire)?;
        token.verify(
            authority_public_key,
            expected_relay_node_id,
            expected_session_id,
            now_ms,
        )?;
        Ok(token)
    }

    pub fn encode(&self) -> Result<Vec<u8>, MediaRelayError> {
        Ok(self.to_wire()?.encode_to_vec())
    }

    fn verify(
        &self,
        authority_public_key: &[u8; 32],
        expected_relay_node_id: &str,
        expected_session_id: Uuid,
        now_ms: u64,
    ) -> Result<(), MediaRelayError> {
        self.validate(now_ms)?;
        if self.relay_node_id != expected_relay_node_id || self.session_id != expected_session_id {
            return Err(MediaRelayError::InvalidToken);
        }
        let verifying_key = VerifyingKey::from_bytes(authority_public_key)
            .map_err(|_| MediaRelayError::InvalidSignature)?;
        verifying_key
            .verify_strict(
                &token_transcript(self),
                &Signature::from_bytes(&self.signature),
            )
            .map_err(|_| MediaRelayError::InvalidSignature)
    }

    pub fn token_id(&self) -> Uuid {
        self.token_id
    }

    pub fn relay_node_id(&self) -> &str {
        &self.relay_node_id
    }

    pub fn session_id(&self) -> Uuid {
        self.session_id
    }

    pub fn expires_at_ms(&self) -> u64 {
        self.expires_at_ms
    }

    pub fn max_bytes(&self) -> u64 {
        self.max_bytes
    }

    pub fn max_duration_ms(&self) -> u64 {
        self.max_duration_ms
    }

    pub fn credit_units(&self) -> u64 {
        self.credit_units
    }

    fn validate(&self, now_ms: u64) -> Result<(), MediaRelayError> {
        if self.token_id.is_nil()
            || self.session_id.is_nil()
            || self.relay_node_id.is_empty()
            || self.expires_at_ms <= now_ms
            || self.expires_at_ms - now_ms > MAX_MEDIA_RELAY_LEASE_MS
            || self.max_bytes == 0
            || self.max_duration_ms == 0
            || self.max_duration_ms > MAX_MEDIA_RELAY_LEASE_MS
            || self.credit_units == 0
        {
            return Err(if self.expires_at_ms <= now_ms
                || self.expires_at_ms - now_ms > MAX_MEDIA_RELAY_LEASE_MS
            {
                MediaRelayError::Expired
            } else {
                MediaRelayError::Invalid
            });
        }
        protocol::validate_gateway_locator(&self.relay_node_id)
            .map_err(|_| MediaRelayError::Invalid)?;
        Ok(())
    }

    fn validate_wire(&self) -> Result<(), MediaRelayError> {
        protocol::validate_media_relay_access_token(&self.to_wire()?)?;
        Ok(())
    }

    fn from_wire(wire: v1::MediaRelayAccessToken) -> Result<Self, MediaRelayError> {
        let signature: [u8; 64] = wire
            .signature
            .as_slice()
            .try_into()
            .map_err(|_| MediaRelayError::InvalidSignature)?;
        Ok(Self {
            token_id: Uuid::parse_str(&wire.token_id).map_err(|_| MediaRelayError::InvalidToken)?,
            relay_node_id: wire.relay_node_id,
            session_id: Uuid::parse_str(&wire.session_id)
                .map_err(|_| MediaRelayError::InvalidToken)?,
            expires_at_ms: wire.expires_at_ms,
            max_bytes: wire.max_bytes,
            max_duration_ms: wire.max_duration_ms,
            credit_units: wire.credit_units,
            signature,
        })
    }

    fn to_wire(&self) -> Result<v1::MediaRelayAccessToken, MediaRelayError> {
        let token = v1::MediaRelayAccessToken {
            protocol_version: protocol::VERSION,
            token_id: self.token_id.hyphenated().to_string(),
            relay_node_id: self.relay_node_id.clone(),
            session_id: self.session_id.hyphenated().to_string(),
            expires_at_ms: self.expires_at_ms,
            max_bytes: self.max_bytes,
            max_duration_ms: self.max_duration_ms,
            credit_units: self.credit_units,
            signature: self.signature.to_vec(),
        };
        protocol::validate_media_relay_access_token(&token)?;
        Ok(token)
    }
}

fn token_transcript(token: &MediaRelayAccessToken) -> Vec<u8> {
    let mut transcript = MEDIA_RELAY_TOKEN_SIGNATURE_DOMAIN.to_vec();
    append_field(&mut transcript, token.token_id.hyphenated().to_string().as_bytes());
    append_field(&mut transcript, token.relay_node_id.as_bytes());
    append_field(&mut transcript, token.session_id.hyphenated().to_string().as_bytes());
    transcript.extend_from_slice(&token.expires_at_ms.to_be_bytes());
    transcript.extend_from_slice(&token.max_bytes.to_be_bytes());
    transcript.extend_from_slice(&token.max_duration_ms.to_be_bytes());
    transcript.extend_from_slice(&token.credit_units.to_be_bytes());
    transcript
}

pub fn required_credit_units(
    price_units_per_minute: u64,
    duration_ms: u64,
) -> Result<u64, MediaRelayError> {
    if price_units_per_minute == 0
        || duration_ms == 0
        || duration_ms > MAX_MEDIA_RELAY_LEASE_MS
    {
        return Err(MediaRelayError::Invalid);
    }
    let minutes = duration_ms
        .checked_add(CREDIT_UNIT_MS - 1)
        .ok_or(MediaRelayError::Invalid)?
        / CREDIT_UNIT_MS;
    price_units_per_minute
        .checked_mul(minutes)
        .ok_or(MediaRelayError::Invalid)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MediaRelayRoute {
    record: MediaRelayRecord,
    session_id: Uuid,
    admission: Option<MediaRelayAccessToken>,
    media_policy: SfuMediaPolicy,
}

impl MediaRelayRoute {
    pub fn record(&self) -> &MediaRelayRecord {
        &self.record
    }

    pub fn session_id(&self) -> Uuid {
        self.session_id
    }

    pub fn admission_token(&self) -> Option<&MediaRelayAccessToken> {
        self.admission.as_ref()
    }

    pub fn media_policy(&self) -> SfuMediaPolicy {
        self.media_policy
    }
}

pub struct MediaRelayRouter<D> {
    discovery: DhtMediaRelayDiscovery<D>,
    authority_public_key: [u8; 32],
}

impl<D> MediaRelayRouter<D>
where
    D: SfuDhtClient + 'static,
{
    pub fn new(
        discovery: DhtMediaRelayDiscovery<D>,
        authority_public_key: [u8; 32],
    ) -> Result<Self, MediaRelayError> {
        if authority_public_key.iter().all(|byte| *byte == 0) {
            return Err(MediaRelayError::Invalid);
        }
        Ok(Self {
            discovery,
            authority_public_key,
        })
    }

    pub async fn select(
        &self,
        region: &str,
        session_id: Uuid,
        duration_ms: u64,
        admission: Option<MediaRelayAccessToken>,
        now_ms: u64,
    ) -> Result<MediaRelayRoute, MediaRelayError> {
        if session_id.is_nil()
            || duration_ms == 0
            || duration_ms > MAX_MEDIA_RELAY_LEASE_MS
        {
            return Err(MediaRelayError::Invalid);
        }
        let records = self.discovery.discover_healthy(region, now_ms).await?;
        let mut saw_token_relay = false;
        let mut saw_insufficient_credit = false;
        if let Some(record) = records
            .iter()
            .find(|record| record.mode() == MediaRelayMode::Open)
        {
            return Ok(MediaRelayRoute {
                media_policy: record.media_policy(),
                record: record.clone(),
                session_id,
                admission: None,
            });
        }
        for record in records {
            saw_token_relay = true;
            let Some(token) = admission.as_ref() else {
                saw_insufficient_credit = true;
                continue;
            };
            if token
                .verify(
                    &self.authority_public_key,
                    record.node_id(),
                    session_id,
                    now_ms,
                )
                .is_err()
            {
                continue;
            }
            let required = required_credit_units(record.price_units_per_minute(), duration_ms)?;
            if token.credit_units() < required || token.max_duration_ms() < duration_ms {
                saw_insufficient_credit = true;
                continue;
            }
            return Ok(MediaRelayRoute {
                media_policy: record.media_policy(),
                record,
                session_id,
                admission: Some(token.clone()),
            });
        }
        if saw_token_relay && saw_insufficient_credit {
            Err(MediaRelayError::InsufficientCredit)
        } else {
            Err(MediaRelayError::Unavailable)
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MediaRelayUsageReceipt {
    token_id: Uuid,
    relay_node_id: String,
    session_id: Uuid,
    bytes_relayed: u64,
    duration_ms: u64,
    charged_units: u64,
    issued_at_ms: u64,
    signature: [u8; 64],
}

impl MediaRelayUsageReceipt {
    pub fn sign<S: MediaRelayNodeSigner + ?Sized>(
        token: &MediaRelayAccessToken,
        relay_node_id: String,
        bytes_relayed: u64,
        duration_ms: u64,
        price_units_per_minute: u64,
        issued_at_ms: u64,
        signer: &S,
        now_ms: u64,
    ) -> Result<Self, MediaRelayError> {
        if token.relay_node_id() != relay_node_id
            || bytes_relayed == 0
            || bytes_relayed > token.max_bytes()
            || duration_ms == 0
            || duration_ms > token.max_duration_ms()
            || issued_at_ms > now_ms
        {
            return Err(MediaRelayError::Invalid);
        }
        let charged_units = required_credit_units(price_units_per_minute, duration_ms)?;
        if charged_units > token.credit_units() {
            return Err(MediaRelayError::InsufficientCredit);
        }
        let receipt = Self {
            token_id: token.token_id(),
            relay_node_id,
            session_id: token.session_id(),
            bytes_relayed,
            duration_ms,
            charged_units,
            issued_at_ms,
            signature: [0; 64],
        };
        let signature = signer.sign(&usage_transcript(&receipt))?;
        let receipt = Self { signature, ..receipt };
        receipt.validate_wire()?;
        Ok(receipt)
    }

    pub fn decode_and_verify(
        bytes: &[u8],
        expected_public_key: &[u8; 32],
        expected_token_id: Uuid,
        expected_relay_node_id: &str,
        expected_session_id: Uuid,
        now_ms: u64,
    ) -> Result<Self, MediaRelayError> {
        let wire = protocol::decode_media_relay_usage_receipt(bytes)?;
        let receipt = Self::from_wire(wire)?;
        if receipt.token_id != expected_token_id
            || receipt.relay_node_id != expected_relay_node_id
            || receipt.session_id != expected_session_id
            || receipt.issued_at_ms > now_ms
        {
            return Err(MediaRelayError::Invalid);
        }
        let verifying_key = VerifyingKey::from_bytes(expected_public_key)
            .map_err(|_| MediaRelayError::InvalidSignature)?;
        verifying_key
            .verify_strict(
                &usage_transcript(&receipt),
                &Signature::from_bytes(&receipt.signature),
            )
            .map_err(|_| MediaRelayError::InvalidSignature)?;
        Ok(receipt)
    }

    pub fn encode(&self) -> Result<Vec<u8>, MediaRelayError> {
        Ok(self.to_wire()?.encode_to_vec())
    }

    pub fn token_id(&self) -> Uuid {
        self.token_id
    }

    pub fn relay_node_id(&self) -> &str {
        &self.relay_node_id
    }

    pub fn session_id(&self) -> Uuid {
        self.session_id
    }

    pub fn bytes_relayed(&self) -> u64 {
        self.bytes_relayed
    }

    pub fn duration_ms(&self) -> u64 {
        self.duration_ms
    }

    pub fn charged_units(&self) -> u64 {
        self.charged_units
    }

    fn from_wire(wire: v1::MediaRelayUsageReceipt) -> Result<Self, MediaRelayError> {
        let signature: [u8; 64] = wire
            .signature
            .as_slice()
            .try_into()
            .map_err(|_| MediaRelayError::InvalidSignature)?;
        Ok(Self {
            token_id: Uuid::parse_str(&wire.token_id).map_err(|_| MediaRelayError::InvalidToken)?,
            relay_node_id: wire.relay_node_id,
            session_id: Uuid::parse_str(&wire.session_id)
                .map_err(|_| MediaRelayError::InvalidToken)?,
            bytes_relayed: wire.bytes_relayed,
            duration_ms: wire.duration_ms,
            charged_units: wire.charged_units,
            issued_at_ms: wire.issued_at_ms,
            signature,
        })
    }

    fn validate_wire(&self) -> Result<(), MediaRelayError> {
        protocol::validate_media_relay_usage_receipt(&self.to_wire()?)?;
        Ok(())
    }

    fn to_wire(&self) -> Result<v1::MediaRelayUsageReceipt, MediaRelayError> {
        let receipt = v1::MediaRelayUsageReceipt {
            protocol_version: protocol::VERSION,
            token_id: self.token_id.hyphenated().to_string(),
            relay_node_id: self.relay_node_id.clone(),
            session_id: self.session_id.hyphenated().to_string(),
            bytes_relayed: self.bytes_relayed,
            duration_ms: self.duration_ms,
            charged_units: self.charged_units,
            issued_at_ms: self.issued_at_ms,
            signature: self.signature.to_vec(),
        };
        protocol::validate_media_relay_usage_receipt(&receipt)?;
        Ok(receipt)
    }
}

fn usage_transcript(receipt: &MediaRelayUsageReceipt) -> Vec<u8> {
    let mut transcript = MEDIA_RELAY_USAGE_SIGNATURE_DOMAIN.to_vec();
    append_field(&mut transcript, receipt.token_id.hyphenated().to_string().as_bytes());
    append_field(&mut transcript, receipt.relay_node_id.as_bytes());
    append_field(
        &mut transcript,
        receipt.session_id.hyphenated().to_string().as_bytes(),
    );
    transcript.extend_from_slice(&receipt.bytes_relayed.to_be_bytes());
    transcript.extend_from_slice(&receipt.duration_ms.to_be_bytes());
    transcript.extend_from_slice(&receipt.charged_units.to_be_bytes());
    transcript.extend_from_slice(&receipt.issued_at_ms.to_be_bytes());
    transcript
}
