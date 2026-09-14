//! Opaque distributed delivery queue contracts.
//!
//! The first deployment target is NATS JetStream. This crate keeps the
//! transport client behind a small publisher trait so credentials, TLS and
//! the chosen NATS client stay in the deployment adapter. It never decrypts or
//! interprets sealed message bytes.
use async_trait::async_trait;
use ed25519_dalek::{Signature, VerifyingKey};
use links_gateway::{ForwardedEnvelope, ForwardedWebRtcSignal, GatewayError, RegionBus};
use links_protocol::{self as protocol, v1};
use prost::Message;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::Mutex;
use thiserror::Error;
use uuid::Uuid;

pub const DELIVERY_SUBJECT_PREFIX: &str = "links.v1.gateway";
pub const BROADCAST_SUBJECT_PREFIX: &str = "links.v1.broadcast";
pub const SIGNAL_SUBJECT_PREFIX: &str = "links.v1.webrtc";
pub const FEDERATION_RELAY_SUBJECT_PREFIX: &str = "links.v1.federation";
pub const DEFAULT_RELAY_DEDUP_ENTRIES: usize = 10_000;
const FEDERATION_SIGNATURE_DOMAIN: &[u8] = b"links/federation-relay/signature/v1\0";
const FEDERATION_BODY_DOMAIN: &[u8] = b"links/federation-relay/body/v1\0";

#[derive(Debug, Error)]
pub enum QueueError {
    #[error("invalid queue message: {0}")]
    Invalid(&'static str),
    #[error(transparent)]
    Protocol(#[from] protocol::ProtocolError),
    #[error("queue publisher unavailable")]
    Unavailable,
    #[error("queue message targets another gateway")]
    WrongDestination,
    #[error("federation relay signature invalid")]
    SignatureInvalid,
    #[error("federation relay batch expired")]
    Expired,
    #[error("federation relay publish failed for batch {batch_id}")]
    RelayPublishFailed { batch_id: String },
}

impl From<QueueError> for GatewayError {
    fn from(error: QueueError) -> Self {
        match error {
            QueueError::Protocol(protocol::ProtocolError::UnsupportedVersion) => {
                Self::UnsupportedVersion
            }
            QueueError::Protocol(protocol::ProtocolError::Invalid(_))
            | QueueError::Protocol(protocol::ProtocolError::Malformed)
            | QueueError::Protocol(protocol::ProtocolError::TooLarge)
            | QueueError::Protocol(protocol::ProtocolError::InvalidRetention)
            | QueueError::Invalid(_)
            | QueueError::WrongDestination
            | QueueError::SignatureInvalid
            | QueueError::Expired => Self::Invalid,
            QueueError::Unavailable | QueueError::RelayPublishFailed { .. } => Self::Unavailable,
        }
    }
}

/// Exact subject for one gateway. Deployments must grant publish/consume
/// permissions per subject; never use a broad `>` subscription for tenants.
pub fn delivery_subject(gateway_id: &str) -> Result<String, QueueError> {
    protocol::validate_gateway_locator(gateway_id)
        .map_err(|_| QueueError::Invalid("gateway subject"))?;
    Ok(format!(
        "{DELIVERY_SUBJECT_PREFIX}.{}.deliver",
        subject_token(gateway_id)
    ))
}

fn subject_token(gateway_id: &str) -> String {
    gateway_id
        .bytes()
        .map(|byte| match byte {
            b if b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-') => {
                char::from(byte).to_string()
            }
            b => format!("~{b:02X}"),
        })
        .collect()
}

/// Exact subject for one broadcast conversation. The conversation ID is
/// hashed before it reaches NATS, so the broker sees only an opaque routing
/// token.
pub fn broadcast_subject(conversation_id: &str) -> Result<String, QueueError> {
    protocol::validate_id(conversation_id).map_err(|_| QueueError::Invalid("broadcast subject"))?;
    let uuid = uuid::Uuid::parse_str(conversation_id)
        .map_err(|_| QueueError::Invalid("broadcast subject"))?;
    let mut hasher = Sha256::new();
    hasher.update(b"links/broadcast/subject/v1\0");
    hasher.update(uuid.as_bytes());
    let token: String = hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    Ok(format!("{BROADCAST_SUBJECT_PREFIX}.{token}.publish"))
}

/// Exact subject for transient WebRTC signaling to one gateway.
pub fn signal_subject(gateway_id: &str) -> Result<String, QueueError> {
    protocol::validate_gateway_locator(gateway_id)
        .map_err(|_| QueueError::Invalid("signal subject"))?;
    Ok(format!(
        "{SIGNAL_SUBJECT_PREFIX}.{}.deliver",
        subject_token(gateway_id)
    ))
}

/// Exact subject for a federated node relay. Node identifiers are encoded so
/// broker subjects cannot be used to smuggle wildcards or hierarchy.
pub fn federation_relay_subject(node_id: &str) -> Result<String, QueueError> {
    protocol::validate_gateway_locator(node_id)
        .map_err(|_| QueueError::Invalid("federation relay subject"))?;
    Ok(format!(
        "{FEDERATION_RELAY_SUBJECT_PREFIX}.{}.relay",
        subject_token(node_id)
    ))
}

/// Queue representation of a cross-region delivery. The serialized envelope
/// is carried byte-for-byte and is not made available as application fields.
pub struct QueueDelivery {
    source_gateway_id: String,
    destination_gateway_id: String,
    delivery: ForwardedEnvelope,
}

/// Signing boundary for a federated node identity. Production adapters should
/// keep the private key in a node HSM or another protected key service.
pub trait FederationNodeSigner: Send + Sync {
    fn sign(&self, transcript: &[u8]) -> Result<[u8; protocol::FEDERATION_SIGNATURE_BYTES], QueueError>;
}

/// A signed, bounded, opaque batch for one destination federation node.
/// Envelope bytes are validated and routed, but sealed_payload is never
/// decrypted or interpreted here.
pub struct FederationRelayBatch {
    source_node_id: String,
    destination_node_id: String,
    batch_id: String,
    expires_at_ms: u64,
    envelopes: Vec<v1::Envelope>,
    body_sha256: [u8; protocol::FEDERATION_BODY_DIGEST_BYTES],
    signature: [u8; protocol::FEDERATION_SIGNATURE_BYTES],
}

impl FederationRelayBatch {
    pub fn sign<S: FederationNodeSigner + ?Sized>(
        source_node_id: String,
        destination_node_id: String,
        batch_id: String,
        expires_at_ms: u64,
        envelopes: Vec<v1::Envelope>,
        signer: &S,
        now_ms: u64,
    ) -> Result<Self, QueueError> {
        validate_relay_window(expires_at_ms, now_ms)?;
        validate_relay_fields(&source_node_id, &destination_node_id, &batch_id)?;
        let serialized_envelopes = serialize_relay_envelopes(&envelopes, now_ms)?;
        let body_sha256 = relay_body_digest(&serialized_envelopes);
        let signature = signer.sign(&relay_signature_transcript(
            &source_node_id,
            &destination_node_id,
            &batch_id,
            expires_at_ms,
            &body_sha256,
        ))?;
        let batch = Self {
            source_node_id,
            destination_node_id,
            batch_id,
            expires_at_ms,
            envelopes,
            body_sha256,
            signature,
        };
        batch.message()?;
        Ok(batch)
    }

    pub fn decode_and_verify(
        bytes: &[u8],
        expected_source_node_id: &str,
        expected_destination_node_id: &str,
        source_public_key: &[u8; 32],
        now_ms: u64,
    ) -> Result<Self, QueueError> {
        let message = protocol::decode_federated_envelope_batch(bytes)?;
        if message.source_node_id != expected_source_node_id {
            return Err(QueueError::Invalid("federation source"));
        }
        if message.destination_node_id != expected_destination_node_id {
            return Err(QueueError::WrongDestination);
        }
        validate_relay_window(message.expires_at_ms, now_ms)?;
        let envelopes = message
            .serialized_envelopes
            .iter()
            .map(|serialized| {
                let envelope = protocol::decode_envelope(serialized)?;
                protocol::validate_enqueue(&envelope, now_ms)?;
                Ok::<v1::Envelope, QueueError>(envelope)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let body_sha256 = array_from_slice::<{ protocol::FEDERATION_BODY_DIGEST_BYTES }>(
            &message.body_sha256,
        )
        .ok_or(QueueError::SignatureInvalid)?;
        if relay_body_digest(&message.serialized_envelopes) != body_sha256 {
            return Err(QueueError::SignatureInvalid);
        }
        let signature = array_from_slice::<{ protocol::FEDERATION_SIGNATURE_BYTES }>(
            &message.signature,
        )
        .ok_or(QueueError::SignatureInvalid)?;
        let verifying_key =
            VerifyingKey::from_bytes(source_public_key).map_err(|_| QueueError::SignatureInvalid)?;
        let verifying_signature = Signature::from_bytes(&signature);
        verifying_key
            .verify_strict(
                &relay_signature_transcript(
                    &message.source_node_id,
                    &message.destination_node_id,
                    &message.batch_id,
                    message.expires_at_ms,
                    &body_sha256,
                ),
                &verifying_signature,
            )
            .map_err(|_| QueueError::SignatureInvalid)?;
        Ok(Self {
            source_node_id: message.source_node_id,
            destination_node_id: message.destination_node_id,
            batch_id: message.batch_id,
            expires_at_ms: message.expires_at_ms,
            envelopes,
            body_sha256,
            signature,
        })
    }

    pub fn encode(&self) -> Result<Vec<u8>, QueueError> {
        Ok(self.message()?.encode_to_vec())
    }

    pub fn source_node_id(&self) -> &str {
        &self.source_node_id
    }

    pub fn destination_node_id(&self) -> &str {
        &self.destination_node_id
    }

    pub fn batch_id(&self) -> &str {
        &self.batch_id
    }

    pub fn expires_at_ms(&self) -> u64 {
        self.expires_at_ms
    }

    pub fn envelopes(&self) -> &[v1::Envelope] {
        &self.envelopes
    }

    fn message(&self) -> Result<v1::FederatedEnvelopeBatch, QueueError> {
        let serialized_envelopes = self
            .envelopes
            .iter()
            .map(|envelope| envelope.encode_to_vec())
            .collect();
        let message = v1::FederatedEnvelopeBatch {
            protocol_version: protocol::VERSION,
            source_node_id: self.source_node_id.clone(),
            destination_node_id: self.destination_node_id.clone(),
            batch_id: self.batch_id.clone(),
            expires_at_ms: self.expires_at_ms,
            serialized_envelopes,
            body_sha256: self.body_sha256.to_vec(),
            signature: self.signature.to_vec(),
        };
        protocol::validate_federated_envelope_batch(&message)?;
        Ok(message)
    }
}

fn validate_relay_fields(
    source_node_id: &str,
    destination_node_id: &str,
    batch_id: &str,
) -> Result<(), QueueError> {
    protocol::validate_gateway_locator(source_node_id)
        .map_err(|_| QueueError::Invalid("federation source"))?;
    protocol::validate_gateway_locator(destination_node_id)
        .map_err(|_| QueueError::Invalid("federation destination"))?;
    if source_node_id == destination_node_id {
        return Err(QueueError::Invalid("same federation node"));
    }
    protocol::validate_id(batch_id).map_err(|_| QueueError::Invalid("federation batch id"))?;
    Ok(())
}

fn validate_relay_window(expires_at_ms: u64, now_ms: u64) -> Result<(), QueueError> {
    if expires_at_ms <= now_ms
        || expires_at_ms - now_ms > protocol::MAX_RETENTION_MS
    {
        return Err(QueueError::Expired);
    }
    Ok(())
}

fn serialize_relay_envelopes(
    envelopes: &[v1::Envelope],
    now_ms: u64,
) -> Result<Vec<Vec<u8>>, QueueError> {
    if envelopes.is_empty() || envelopes.len() > protocol::MAX_FEDERATION_BATCH_ITEMS {
        return Err(QueueError::Invalid("federation batch items"));
    }
    let mut serialized = Vec::with_capacity(envelopes.len());
    let mut total = 0usize;
    let mut envelope_ids = HashSet::with_capacity(envelopes.len());
    for envelope in envelopes {
        protocol::validate_enqueue(envelope, now_ms)?;
        if !envelope_ids.insert(envelope.envelope_id.as_str()) {
            return Err(QueueError::Invalid("duplicate relay envelope"));
        }
        let bytes = envelope.encode_to_vec();
        total = total
            .checked_add(bytes.len())
            .ok_or(QueueError::Invalid("federation batch size"))?;
        if total > protocol::MAX_QUEUE_MESSAGE_BYTES {
            return Err(QueueError::Protocol(protocol::ProtocolError::TooLarge));
        }
        serialized.push(bytes);
    }
    Ok(serialized)
}

fn relay_body_digest(serialized_envelopes: &[Vec<u8>]) -> [u8; protocol::FEDERATION_BODY_DIGEST_BYTES] {
    let mut hasher = Sha256::new();
    hasher.update(FEDERATION_BODY_DOMAIN);
    for serialized in serialized_envelopes {
        hasher.update((serialized.len() as u32).to_be_bytes());
        hasher.update(serialized);
    }
    let digest = hasher.finalize();
    let mut output = [0u8; protocol::FEDERATION_BODY_DIGEST_BYTES];
    output.copy_from_slice(&digest);
    output
}

fn relay_signature_transcript(
    source_node_id: &str,
    destination_node_id: &str,
    batch_id: &str,
    expires_at_ms: u64,
    body_sha256: &[u8; protocol::FEDERATION_BODY_DIGEST_BYTES],
) -> Vec<u8> {
    let mut transcript = FEDERATION_SIGNATURE_DOMAIN.to_vec();
    append_transcript_field(&mut transcript, source_node_id.as_bytes());
    append_transcript_field(&mut transcript, destination_node_id.as_bytes());
    append_transcript_field(&mut transcript, batch_id.as_bytes());
    transcript.extend_from_slice(&expires_at_ms.to_be_bytes());
    transcript.extend_from_slice(body_sha256);
    transcript
}

fn append_transcript_field(transcript: &mut Vec<u8>, value: &[u8]) {
    transcript.extend_from_slice(&(value.len() as u32).to_be_bytes());
    transcript.extend_from_slice(value);
}

fn array_from_slice<const N: usize>(value: &[u8]) -> Option<[u8; N]> {
    value.try_into().ok()
}

/// Bounded in-memory replay protection for a relay consumer. Production
/// deployments should back this claim operation with durable shared state if
/// multiple consumers can receive the same node's relay stream.
pub struct RelayDeduplicator {
    seen: Mutex<HashMap<String, u64>>,
    max_entries: usize,
}

impl RelayDeduplicator {
    pub fn new(max_entries: usize) -> Self {
        Self {
            seen: Mutex::new(HashMap::new()),
            max_entries: max_entries.max(1),
        }
    }

    pub fn default_capacity() -> Self {
        Self::new(DEFAULT_RELAY_DEDUP_ENTRIES)
    }

    /// Claim the complete batch before routing any envelope. Returns false
    /// when this batch or one of its envelopes was already accepted.
    pub fn claim(&self, batch: &FederationRelayBatch, now_ms: u64) -> Result<bool, QueueError> {
        validate_relay_window(batch.expires_at_ms, now_ms)?;
        let mut seen = self.seen.lock().map_err(|_| QueueError::Unavailable)?;
        seen.retain(|_, expires_at_ms| *expires_at_ms > now_ms);
        let mut keys = Vec::with_capacity(batch.envelopes.len() + 1);
        keys.push(format!(
            "batch:{}:{}:{}",
            batch.source_node_id, batch.destination_node_id, batch.batch_id
        ));
        keys.extend(batch.envelopes.iter().map(|envelope| {
            format!(
                "envelope:{}:{}:{}",
                batch.source_node_id, batch.destination_node_id, envelope.envelope_id
            )
        }));
        if keys.iter().any(|key| seen.contains_key(key)) {
            return Ok(false);
        }
        if seen.len().saturating_add(keys.len()) > self.max_entries {
            return Err(QueueError::Unavailable);
        }
        for key in keys {
            seen.insert(key, batch.expires_at_ms);
        }
        Ok(true)
    }
}

/// Verify and atomically claim a relay batch before handing its envelopes to
/// the local gateway/mailbox router. A false result is an idempotent replay.
pub fn decode_and_claim_relay_for_node(
    payload: &[u8],
    expected_source_node_id: &str,
    expected_destination_node_id: &str,
    source_public_key: &[u8; 32],
    now_ms: u64,
    deduplicator: &RelayDeduplicator,
) -> Result<Option<FederationRelayBatch>, QueueError> {
    let batch = FederationRelayBatch::decode_and_verify(
        payload,
        expected_source_node_id,
        expected_destination_node_id,
        source_public_key,
        now_ms,
    )?;
    if deduplicator.claim(&batch, now_ms)? {
        Ok(Some(batch))
    } else {
        Ok(None)
    }
}

/// Cross-region WebRTC signaling wrapper. It carries SDP/ICE only while the
/// target device has a live route and is never stored in the mailbox.
pub struct QueueWebRtcSignal {
    source_gateway_id: String,
    destination_gateway_id: String,
    delivery: v1::WebRtcSignalDelivery,
}

impl QueueWebRtcSignal {
    pub fn new(
        source_gateway_id: String,
        destination_gateway_id: String,
        delivery: v1::WebRtcSignalDelivery,
    ) -> Result<Self, QueueError> {
        let message = v1::GatewayWebRtcSignal {
            protocol_version: protocol::VERSION,
            source_gateway_id: source_gateway_id.clone(),
            destination_gateway_id: destination_gateway_id.clone(),
            delivery: Some(delivery.clone()),
        };
        protocol::validate_gateway_webrtc_signal(&message)?;
        Ok(Self {
            source_gateway_id,
            destination_gateway_id,
            delivery,
        })
    }

    pub fn encode(&self) -> Result<Vec<u8>, QueueError> {
        let message = v1::GatewayWebRtcSignal {
            protocol_version: protocol::VERSION,
            source_gateway_id: self.source_gateway_id.clone(),
            destination_gateway_id: self.destination_gateway_id.clone(),
            delivery: Some(self.delivery.clone()),
        };
        protocol::validate_gateway_webrtc_signal(&message)?;
        Ok(message.encode_to_vec())
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, QueueError> {
        if bytes.is_empty() || bytes.len() > protocol::MAX_QUEUE_MESSAGE_BYTES {
            return Err(QueueError::Invalid("signal size"));
        }
        let message = v1::GatewayWebRtcSignal::decode(bytes)
            .map_err(|_| QueueError::Protocol(protocol::ProtocolError::Malformed))?;
        let delivery = message
            .delivery
            .clone()
            .ok_or(QueueError::Invalid("signal delivery"))?;
        protocol::validate_gateway_webrtc_signal(&message)?;
        Ok(Self {
            source_gateway_id: message.source_gateway_id,
            destination_gateway_id: message.destination_gateway_id,
            delivery,
        })
    }

    pub fn into_forwarded_for(
        self,
        destination_gateway_id: &str,
    ) -> Result<ForwardedWebRtcSignal, QueueError> {
        if self.destination_gateway_id != destination_gateway_id {
            return Err(QueueError::WrongDestination);
        }
        Ok(ForwardedWebRtcSignal {
            delivery: self.delivery,
        })
    }
}

impl QueueDelivery {
    pub fn new(
        source_gateway_id: String,
        destination_gateway_id: String,
        delivery: ForwardedEnvelope,
    ) -> Result<Self, QueueError> {
        let message = v1::GatewayDelivery {
            protocol_version: protocol::VERSION,
            source_gateway_id: source_gateway_id.clone(),
            destination_gateway_id: destination_gateway_id.clone(),
            cursor: delivery.cursor,
            serialized_envelope: delivery.envelope.encode_to_vec(),
        };
        protocol::validate_gateway_delivery(&message)?;
        Ok(Self {
            source_gateway_id,
            destination_gateway_id,
            delivery,
        })
    }

    pub fn source_gateway_id(&self) -> &str {
        &self.source_gateway_id
    }

    pub fn destination_gateway_id(&self) -> &str {
        &self.destination_gateway_id
    }

    pub fn cursor(&self) -> u64 {
        self.delivery.cursor
    }

    pub fn encode(&self) -> Result<Vec<u8>, QueueError> {
        let message = v1::GatewayDelivery {
            protocol_version: protocol::VERSION,
            source_gateway_id: self.source_gateway_id.clone(),
            destination_gateway_id: self.destination_gateway_id.clone(),
            cursor: self.delivery.cursor,
            serialized_envelope: self.delivery.envelope.encode_to_vec(),
        };
        protocol::validate_gateway_delivery(&message)?;
        Ok(message.encode_to_vec())
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, QueueError> {
        let message = protocol::decode_gateway_delivery(bytes)?;
        let envelope = protocol::decode_envelope(&message.serialized_envelope)?;
        Ok(Self {
            source_gateway_id: message.source_gateway_id,
            destination_gateway_id: message.destination_gateway_id,
            delivery: ForwardedEnvelope {
                envelope,
                cursor: message.cursor,
            },
        })
    }

    /// Consumer-side destination check. Ack only after the local gateway has
    /// handed this delivery to its socket or durable replay path.
    pub fn into_forwarded_for(
        self,
        destination_gateway_id: &str,
    ) -> Result<ForwardedEnvelope, QueueError> {
        if self.destination_gateway_id != destination_gateway_id {
            return Err(QueueError::WrongDestination);
        }
        Ok(self.delivery)
    }
}

/// Adapter boundary for an authenticated NATS JetStream client. Implementors
/// must return only after the server confirms the message is durably accepted.
#[async_trait]
pub trait NatsPublisher: Send + Sync {
    async fn publish_durable(&self, subject: &str, payload: Vec<u8>) -> Result<(), QueueError>;

    /// Publish without JetStream persistence for live WebRTC signaling. The
    /// default keeps simple local adapters source-compatible; production NATS
    /// adapters must use a core NATS subject for this method.
    async fn publish_transient(&self, subject: &str, payload: Vec<u8>) -> Result<(), QueueError> {
        self.publish_durable(subject, payload).await
    }
}

/// NATS JetStream adapter for broadcast posts. It validates only the public
/// dispatch envelope and publishes its master-key ciphertext unchanged.
pub struct NatsBroadcastPublisher<P> {
    publisher: Arc<P>,
}

impl<P> NatsBroadcastPublisher<P>
where
    P: NatsPublisher + 'static,
{
    pub fn new(publisher: Arc<P>) -> Self {
        Self { publisher }
    }

    pub async fn dispatch(&self, dispatch: v1::BroadcastDispatch) -> Result<(), QueueError> {
        protocol::validate_broadcast_dispatch(&dispatch)?;
        let subject = broadcast_subject(&dispatch.conversation_id)?;
        self.publisher
            .publish_durable(&subject, dispatch.encode_to_vec())
            .await
    }
}

/// Federated relay gossip adapter. Each peer gets its own signed batch and
/// exact broker subject, so retries are safe with the same batch ID and a
/// relay worker never needs to inspect sealed payload contents.
pub struct NatsFederationRelay<P, S> {
    source_node_id: String,
    publisher: Arc<P>,
    signer: Arc<S>,
}

impl<P, S> NatsFederationRelay<P, S>
where
    P: NatsPublisher + 'static,
    S: FederationNodeSigner + 'static,
{
    pub fn new(
        source_node_id: String,
        publisher: Arc<P>,
        signer: Arc<S>,
    ) -> Result<Self, QueueError> {
        protocol::validate_gateway_locator(&source_node_id)
            .map_err(|_| QueueError::Invalid("federation source"))?;
        Ok(Self {
            source_node_id,
            publisher,
            signer,
        })
    }

    pub fn source_node_id(&self) -> &str {
        &self.source_node_id
    }

    /// Publish one batch per destination. A failed publish leaves earlier
    /// peers committed; retry with `publish_batch_to_peers` and the returned
    /// batch ID so receivers deduplicate already accepted envelopes.
    pub async fn publish_to_peers(
        &self,
        destination_node_ids: &[String],
        envelopes: Vec<v1::Envelope>,
        expires_at_ms: u64,
        now_ms: u64,
    ) -> Result<String, QueueError> {
        let batch_id = Uuid::new_v4().hyphenated().to_string();
        let result = self
            .publish_batch_to_peers(
            destination_node_ids,
            &batch_id,
            envelopes,
            expires_at_ms,
            now_ms,
        )
        .await;
        result.map(|_| batch_id.clone()).map_err(|_| {
            QueueError::RelayPublishFailed { batch_id }
        })
    }

    pub async fn publish_batch_to_peers(
        &self,
        destination_node_ids: &[String],
        batch_id: &str,
        envelopes: Vec<v1::Envelope>,
        expires_at_ms: u64,
        now_ms: u64,
    ) -> Result<(), QueueError> {
        if destination_node_ids.is_empty()
            || destination_node_ids.len() > protocol::MAX_FEDERATION_RELAY_FANOUT
        {
            return Err(QueueError::Invalid("federation relay fanout"));
        }
        let mut destinations = HashSet::with_capacity(destination_node_ids.len());
        if destination_node_ids
            .iter()
            .any(|destination| !destinations.insert(destination.as_str()))
        {
            return Err(QueueError::Invalid("duplicate federation destination"));
        }
        for destination_node_id in destination_node_ids {
            validate_relay_fields(&self.source_node_id, destination_node_id, batch_id)?;
        }
        serialize_relay_envelopes(&envelopes, now_ms)?;
        for destination_node_id in destination_node_ids {
            let batch = FederationRelayBatch::sign(
                self.source_node_id.clone(),
                destination_node_id.clone(),
                batch_id.to_owned(),
                expires_at_ms,
                envelopes.clone(),
                self.signer.as_ref(),
                now_ms,
            )?;
            let subject = federation_relay_subject(destination_node_id)?;
            self.publisher
                .publish_durable(&subject, batch.encode()?)
                .await?;
        }
        Ok(())
    }
}

/// Decode one broker broadcast dispatch before passing it to the client-core
/// receive contract. This never opens the master-key ciphertext.
pub fn decode_broadcast_dispatch(payload: &[u8]) -> Result<v1::BroadcastDispatch, QueueError> {
    if payload.is_empty() || payload.len() > protocol::MAX_QUEUE_MESSAGE_BYTES {
        return Err(if payload.len() > protocol::MAX_QUEUE_MESSAGE_BYTES {
            QueueError::Protocol(protocol::ProtocolError::TooLarge)
        } else {
            QueueError::Protocol(protocol::ProtocolError::Malformed)
        });
    }
    let dispatch = v1::BroadcastDispatch::decode(payload)
        .map_err(|_| QueueError::Protocol(protocol::ProtocolError::Malformed))?;
    protocol::validate_broadcast_dispatch(&dispatch)?;
    Ok(dispatch)
}

/// RegionBus implementation for NATS JetStream. A real adapter supplies the
/// mTLS-authenticated JetStream publisher and configures stream retention,
/// replicas and consumer acknowledgements outside this crate.
pub struct NatsRegionBus<P> {
    source_gateway_id: String,
    publisher: Arc<P>,
}

impl<P> NatsRegionBus<P>
where
    P: NatsPublisher + 'static,
{
    pub fn new(source_gateway_id: String, publisher: Arc<P>) -> Result<Self, QueueError> {
        protocol::validate_gateway_locator(&source_gateway_id)
            .map_err(|_| QueueError::Invalid("source gateway"))?;
        Ok(Self {
            source_gateway_id,
            publisher,
        })
    }

    pub fn source_gateway_id(&self) -> &str {
        &self.source_gateway_id
    }
}

#[async_trait]
impl<P> RegionBus for NatsRegionBus<P>
where
    P: NatsPublisher + 'static,
{
    async fn forward(
        &self,
        destination_gateway_id: &str,
        delivery: ForwardedEnvelope,
    ) -> Result<(), GatewayError> {
        let subject = delivery_subject(destination_gateway_id)?;
        let queue_delivery = QueueDelivery::new(
            self.source_gateway_id.clone(),
            destination_gateway_id.to_owned(),
            delivery,
        )?;
        self.publisher
            .publish_durable(&subject, queue_delivery.encode()?)
            .await
            .map_err(GatewayError::from)
    }

    async fn forward_signal(
        &self,
        destination_gateway_id: &str,
        signal: ForwardedWebRtcSignal,
    ) -> Result<(), GatewayError> {
        let subject = signal_subject(destination_gateway_id)?;
        let queued = QueueWebRtcSignal::new(
            self.source_gateway_id.clone(),
            destination_gateway_id.to_owned(),
            signal.delivery,
        )?;
        self.publisher
            .publish_transient(&subject, queued.encode()?)
            .await
            .map_err(GatewayError::from)
    }
}

/// Decode one NATS message before handing it to `Gateway::handle_forwarded`.
/// The NATS consumer must ack only after that call succeeds; malformed or
/// misrouted messages must be rejected and alerted, not retried forever.
pub fn decode_for_gateway(
    payload: &[u8],
    destination_gateway_id: &str,
) -> Result<ForwardedEnvelope, QueueError> {
    QueueDelivery::decode(payload)?.into_forwarded_for(destination_gateway_id)
}

/// Decode a transient signaling message before handing it to
/// `Gateway::handle_forwarded_signal`. The NATS consumer must not retry a
/// malformed or misrouted signal forever.
pub fn decode_signal_for_gateway(
    payload: &[u8],
    destination_gateway_id: &str,
) -> Result<ForwardedWebRtcSignal, QueueError> {
    QueueWebRtcSignal::decode(payload)?.into_forwarded_for(destination_gateway_id)
}
