//! DHT-backed discovery for self-hosted or decentralized SFU nodes.
//!
//! The DHT stores only signed public endpoint records. It never stores room
//! names, account IDs, participant lists, tokens, MLS keys, or media frames.
//! A production adapter can implement `SfuDhtClient` with libp2p Kademlia or
//! another authenticated DHT; `MemorySfuDht` is only a bounded local reference.

use super::sfu::SfuError;
use async_trait::async_trait;
use ed25519_dalek::{Signature, VerifyingKey};
use links_protocol::{self as protocol, v1};
use prost::Message;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

pub const SFU_DHT_KEY_DOMAIN: &[u8] = b"links/sfu-discovery/dht-key/v1\0";
pub const SFU_DISCOVERY_SIGNATURE_DOMAIN: &[u8] =
    b"links/sfu-discovery/record-signature/v1\0";
pub const SFU_DISCOVERY_SIGNATURE_BYTES: usize = 64;
pub const MAX_HEALTH_STALENESS_MS: u64 = 2 * 60 * 1000;

/// A node signs this record with its long-lived discovery key. The key must be
/// pinned in the trusted SFU directory before the DHT record is accepted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SfuDiscoveryRecord {
    node_id: String,
    region: String,
    websocket_url: String,
    turn_url: Option<String>,
    public_key: [u8; 32],
    sequence: u64,
    expires_at_ms: u64,
    supports_sframe: bool,
    signature: [u8; SFU_DISCOVERY_SIGNATURE_BYTES],
}

pub trait SfuNodeSigner: Send + Sync {
    fn public_key(&self) -> [u8; 32];
    fn sign(&self, transcript: &[u8]) -> Result<[u8; SFU_DISCOVERY_SIGNATURE_BYTES], SfuError>;
}

impl SfuDiscoveryRecord {
    pub fn sign<S: SfuNodeSigner + ?Sized>(
        node_id: String,
        region: String,
        websocket_url: String,
        turn_url: Option<String>,
        sequence: u64,
        expires_at_ms: u64,
        signer: &S,
        now_ms: u64,
    ) -> Result<Self, SfuError> {
        validate_record_fields(
            &node_id,
            &region,
            &websocket_url,
            turn_url.as_deref(),
            sequence,
        )?;
        validate_record_window(expires_at_ms, now_ms)?;
        let public_key = signer.public_key();
        if public_key.iter().all(|byte| *byte == 0) {
            return Err(SfuError::Invalid);
        }
        let signature = signer.sign(&record_transcript(
            &node_id,
            &region,
            &websocket_url,
            turn_url.as_deref(),
            &public_key,
            sequence,
            expires_at_ms,
            true,
        ))?;
        let record = Self {
            node_id,
            region,
            websocket_url,
            turn_url,
            public_key,
            sequence,
            expires_at_ms,
            supports_sframe: true,
            signature,
        };
        record.validate(now_ms)?;
        Ok(record)
    }

    pub fn decode_and_verify(
        bytes: &[u8],
        expected_public_key: &[u8; 32],
        now_ms: u64,
    ) -> Result<Self, SfuError> {
        let wire = protocol::decode_sfu_discovery_record(bytes)?;
        let public_key: [u8; 32] = wire
            .public_key
            .as_slice()
            .try_into()
            .map_err(|_| SfuError::InvalidSignature)?;
        if &public_key != expected_public_key {
            return Err(SfuError::UntrustedNode);
        }
        let signature: [u8; SFU_DISCOVERY_SIGNATURE_BYTES] = wire
            .signature
            .as_slice()
            .try_into()
            .map_err(|_| SfuError::InvalidSignature)?;
        let record = Self {
            node_id: wire.node_id,
            region: wire.region,
            websocket_url: wire.websocket_url,
            turn_url: (!wire.turn_url.is_empty()).then_some(wire.turn_url),
            public_key,
            sequence: wire.sequence,
            expires_at_ms: wire.expires_at_ms,
            supports_sframe: wire.supports_sframe,
            signature,
        };
        record.validate(now_ms)?;
        let verifying_key =
            VerifyingKey::from_bytes(&public_key).map_err(|_| SfuError::InvalidSignature)?;
        let signature = Signature::from_bytes(&record.signature);
        verifying_key
            .verify_strict(
                &record_transcript(
                    &record.node_id,
                    &record.region,
                    &record.websocket_url,
                    record.turn_url.as_deref(),
                    &record.public_key,
                    record.sequence,
                    record.expires_at_ms,
                    record.supports_sframe,
                ),
                &signature,
            )
            .map_err(|_| SfuError::InvalidSignature)?;
        Ok(record)
    }

    pub fn encode(&self) -> Result<Vec<u8>, SfuError> {
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

    pub fn supports_sframe(&self) -> bool {
        self.supports_sframe
    }

    fn validate(&self, now_ms: u64) -> Result<(), SfuError> {
        validate_record_fields(
            &self.node_id,
            &self.region,
            &self.websocket_url,
            self.turn_url.as_deref(),
            self.sequence,
        )?;
        validate_record_window(self.expires_at_ms, now_ms)?;
        protocol::validate_sfu_discovery_record(&self.to_wire()?)?;
        Ok(())
    }

    fn to_wire(&self) -> Result<v1::SfuDiscoveryRecord, SfuError> {
        let record = v1::SfuDiscoveryRecord {
            protocol_version: protocol::VERSION,
            node_id: self.node_id.clone(),
            region: self.region.clone(),
            websocket_url: self.websocket_url.clone(),
            turn_url: self.turn_url.clone().unwrap_or_default(),
            public_key: self.public_key.to_vec(),
            sequence: self.sequence,
            expires_at_ms: self.expires_at_ms,
            supports_sframe: self.supports_sframe,
            signature: self.signature.to_vec(),
        };
        protocol::validate_sfu_discovery_record(&record)?;
        Ok(record)
    }
}

fn validate_record_fields(
    node_id: &str,
    region: &str,
    websocket_url: &str,
    turn_url: Option<&str>,
    sequence: u64,
) -> Result<(), SfuError> {
    protocol::validate_gateway_locator(node_id).map_err(|_| SfuError::Invalid)?;
    protocol::validate_gateway_locator(region).map_err(|_| SfuError::Invalid)?;
    if websocket_url.len() > 512
        || !websocket_url.starts_with("wss://")
        || websocket_url.bytes().any(|byte| {
            byte.is_ascii_control() || byte.is_ascii_whitespace() || matches!(byte, b'?' | b'#')
        })
    {
        return Err(SfuError::Invalid);
    }
    if turn_url.is_some_and(|url| {
        url.len() > 512
            || !(url.starts_with("turn:") || url.starts_with("turns:"))
            || url.bytes().any(|byte| {
                byte.is_ascii_control() || byte.is_ascii_whitespace() || byte == b'#'
            })
    }) {
        return Err(SfuError::Invalid);
    }
    if sequence == 0 {
        return Err(SfuError::Invalid);
    }
    Ok(())
}

fn validate_record_window(expires_at_ms: u64, now_ms: u64) -> Result<(), SfuError> {
    if expires_at_ms <= now_ms
        || expires_at_ms - now_ms > protocol::MAX_SFU_DISCOVERY_TTL_MS
    {
        return Err(SfuError::ExpiredRecord);
    }
    Ok(())
}

fn record_transcript(
    node_id: &str,
    region: &str,
    websocket_url: &str,
    turn_url: Option<&str>,
    public_key: &[u8; 32],
    sequence: u64,
    expires_at_ms: u64,
    supports_sframe: bool,
) -> Vec<u8> {
    let mut transcript = SFU_DISCOVERY_SIGNATURE_DOMAIN.to_vec();
    append_field(&mut transcript, node_id.as_bytes());
    append_field(&mut transcript, region.as_bytes());
    append_field(&mut transcript, websocket_url.as_bytes());
    append_field(&mut transcript, turn_url.unwrap_or_default().as_bytes());
    append_field(&mut transcript, public_key);
    transcript.extend_from_slice(&sequence.to_be_bytes());
    transcript.extend_from_slice(&expires_at_ms.to_be_bytes());
    transcript.push(u8::from(supports_sframe));
    transcript
}

fn append_field(transcript: &mut Vec<u8>, value: &[u8]) {
    transcript.extend_from_slice(&(value.len() as u32).to_be_bytes());
    transcript.extend_from_slice(value);
}

pub fn sfu_dht_key(region: &str) -> Result<[u8; 32], SfuError> {
    protocol::validate_gateway_locator(region).map_err(|_| SfuError::Invalid)?;
    let mut hasher = Sha256::new();
    hasher.update(SFU_DHT_KEY_DOMAIN);
    hasher.update(region.as_bytes());
    let digest = hasher.finalize();
    let mut key = [0u8; 32];
    key.copy_from_slice(&digest);
    Ok(key)
}

#[async_trait]
pub trait SfuDhtClient: Send + Sync {
    async fn put(&self, key: [u8; 32], record: Vec<u8>) -> Result<(), SfuError>;
    async fn get(&self, key: [u8; 32], limit: usize) -> Result<Vec<Vec<u8>>, SfuError>;
}

#[derive(Clone, Default)]
pub struct SfuTrustDirectory {
    keys: HashMap<String, [u8; 32]>,
}

impl SfuTrustDirectory {
    pub fn new(entries: Vec<(String, [u8; 32])>) -> Result<Self, SfuError> {
        let mut keys = HashMap::with_capacity(entries.len());
        for (node_id, public_key) in entries {
            protocol::validate_gateway_locator(&node_id).map_err(|_| SfuError::Invalid)?;
            if public_key.iter().all(|byte| *byte == 0) || keys.insert(node_id, public_key).is_some()
            {
                return Err(SfuError::Invalid);
            }
        }
        Ok(Self { keys })
    }

    pub fn key_for(&self, node_id: &str) -> Option<&[u8; 32]> {
        self.keys.get(node_id)
    }
}

#[derive(Clone, Copy)]
struct HealthEntry {
    healthy: bool,
    checked_at_ms: u64,
}

pub struct SfuHealthCache {
    entries: Mutex<HashMap<String, HealthEntry>>,
}

impl Default for SfuHealthCache {
    fn default() -> Self {
        Self::new()
    }
}

impl SfuHealthCache {
    pub fn new() -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
        }
    }

    pub fn mark_healthy(
        &self,
        node_id: &str,
        healthy: bool,
        checked_at_ms: u64,
    ) -> Result<(), SfuError> {
        protocol::validate_gateway_locator(node_id).map_err(|_| SfuError::Invalid)?;
        let mut entries = self.entries.lock().map_err(|_| SfuError::DhtUnavailable)?;
        entries.insert(
            node_id.to_owned(),
            HealthEntry {
                healthy,
                checked_at_ms,
            },
        );
        Ok(())
    }

    pub fn is_healthy(&self, node_id: &str, now_ms: u64) -> bool {
        self.entries
            .lock()
            .ok()
            .and_then(|entries| entries.get(node_id).copied())
            .is_some_and(|entry| {
                entry.healthy
                    && entry.checked_at_ms <= now_ms
                    && now_ms - entry.checked_at_ms <= MAX_HEALTH_STALENESS_MS
            })
    }
}

pub struct DhtSfuDiscovery<D> {
    dht: Arc<D>,
    trust: Arc<SfuTrustDirectory>,
    health: Arc<SfuHealthCache>,
}

impl<D> DhtSfuDiscovery<D>
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
        record: &SfuDiscoveryRecord,
        now_ms: u64,
    ) -> Result<(), SfuError> {
        let expected_key = self
            .trust
            .key_for(record.node_id())
            .ok_or(SfuError::UntrustedNode)?;
        if expected_key != record.public_key() {
            return Err(SfuError::UntrustedNode);
        }
        record.validate(now_ms)?;
        self.dht
            .put(sfu_dht_key(record.region())?, record.encode()?)
            .await
    }

    /// Return only records whose signature, trust binding, expiry, region,
    /// SFrame capability, and recent external health check all pass. The
    /// highest sequence per node wins when a DHT returns stale records.
    pub async fn discover_healthy(
        &self,
        region: &str,
        now_ms: u64,
    ) -> Result<Vec<SfuDiscoveryRecord>, SfuError> {
        let key = sfu_dht_key(region)?;
        let records = self
            .dht
            .get(key, protocol::MAX_SFU_DISCOVERY_RESULTS)
            .await?;
        let mut highest: HashMap<String, SfuDiscoveryRecord> = HashMap::new();
        for bytes in records.into_iter().take(protocol::MAX_SFU_DISCOVERY_RESULTS) {
            let wire = match protocol::decode_sfu_discovery_record(&bytes) {
                Ok(wire) => wire,
                Err(_) => continue,
            };
            if wire.region != region {
                continue;
            }
            let expected_key = match self.trust.key_for(&wire.node_id) {
                Some(key) => key,
                None => continue,
            };
            let record = match SfuDiscoveryRecord::decode_and_verify(&bytes, expected_key, now_ms) {
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
            return Err(SfuError::Unavailable);
        }
        Ok(records)
    }
}

/// Bounded local DHT reference implementation. It preserves the highest
/// sequence per node and is useful for development; production should replace
/// it with an authenticated distributed DHT adapter.
pub struct MemorySfuDht {
    records: Mutex<HashMap<[u8; 32], Vec<Vec<u8>>>>,
    max_records_per_key: usize,
}

impl MemorySfuDht {
    pub fn new(max_records_per_key: usize) -> Self {
        Self {
            records: Mutex::new(HashMap::new()),
            max_records_per_key: max_records_per_key.max(1),
        }
    }
}

#[async_trait]
impl SfuDhtClient for MemorySfuDht {
    async fn put(&self, key: [u8; 32], record: Vec<u8>) -> Result<(), SfuError> {
        let decoded = protocol::decode_sfu_discovery_record(&record)?;
        if sfu_dht_key(&decoded.region)? != key {
            return Err(SfuError::Invalid);
        }
        let mut records = self.records.lock().map_err(|_| SfuError::DhtUnavailable)?;
        let bucket = records.entry(key).or_default();
        for existing_bytes in bucket.iter_mut() {
            let existing = protocol::decode_sfu_discovery_record(existing_bytes)?;
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
        if limit == 0 || limit > protocol::MAX_SFU_DISCOVERY_RESULTS {
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
