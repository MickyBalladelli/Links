//! Opaque distributed delivery queue contracts.
//!
//! The first deployment target is NATS JetStream. This crate keeps the
//! transport client behind a small publisher trait so credentials, TLS and
//! the chosen NATS client stay in the deployment adapter. It never decrypts or
//! interprets sealed message bytes.
use async_trait::async_trait;
use links_gateway::{ForwardedEnvelope, GatewayError, RegionBus};
use links_protocol::{self as protocol, v1};
use prost::Message;
use std::sync::Arc;
use thiserror::Error;

pub const DELIVERY_SUBJECT_PREFIX: &str = "links.v1.gateway";

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
            | QueueError::WrongDestination => Self::Invalid,
            QueueError::Unavailable => Self::Unavailable,
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

/// Queue representation of a cross-region delivery. The serialized envelope
/// is carried byte-for-byte and is not made available as application fields.
pub struct QueueDelivery {
    source_gateway_id: String,
    destination_gateway_id: String,
    delivery: ForwardedEnvelope,
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
