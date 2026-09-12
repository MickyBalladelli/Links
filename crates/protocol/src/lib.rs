//! Shared wire types. Generated types deliberately have no Debug implementation.
//! Message, media, receipt, and authentication bytes must never be logged.
use prost::Message as ProstMessage;
use thiserror::Error;

pub mod v1 {
    include!(concat!(env!("OUT_DIR"), "/links.v1.rs"));
}
pub const DESCRIPTOR_SET: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/links.bin"));
pub const VERSION: u32 = 1;
pub const MAX_ENVELOPE_BYTES: usize = 256 * 1024;
pub const MAX_MESSAGE_BYTES: usize = 64 * 1024;
pub const MAX_BATCH_ITEMS: usize = 100;
pub const MAX_FRAME_BYTES: usize = 1024 * 1024;
pub const MAX_RETENTION_MS: u64 = 30 * 24 * 60 * 60 * 1000;
pub const MAX_CURSOR: u64 = i64::MAX as u64;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ProtocolError {
    #[error("invalid field: {0}")]
    Invalid(&'static str),
    #[error("unsupported protocol version")]
    UnsupportedVersion,
    #[error("payload exceeds size limit")]
    TooLarge,
    #[error("malformed protobuf")]
    Malformed,
    #[error("expired envelope or retention exceeds 30 days")]
    InvalidRetention,
}

pub fn validate_id(value: &str) -> Result<(), ProtocolError> {
    let id = uuid::Uuid::parse_str(value).map_err(|_| ProtocolError::Invalid("id"))?;
    if id.is_nil() || id.hyphenated().to_string() != value {
        return Err(ProtocolError::Invalid("id"));
    }
    Ok(())
}

pub fn validate_handle(value: &str) -> Result<(), ProtocolError> {
    if !(3..=32).contains(&value.len())
        || !value.bytes().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'_')
        || !value.as_bytes()[0].is_ascii_lowercase()
    {
        return Err(ProtocolError::Invalid("handle"));
    }
    Ok(())
}

pub fn validate_user(user: &v1::User) -> Result<(), ProtocolError> {
    validate_id(&user.user_id)?;
    if let Some(handle) = &user.handle { validate_handle(handle)?; }
    let mut devices = std::collections::HashSet::new();
    for device in &user.devices {
        validate_id(&device.device_id)?;
        if device.user_id != user.user_id || !devices.insert(&device.device_id)
            || device.identity_public_key.is_empty() || device.mls_credential.is_empty()
            || device.registered_at_ms == 0
            || device.revoked_at_ms.is_some_and(|time| time < device.registered_at_ms)
        {
            return Err(ProtocolError::Invalid("device"));
        }
    }
    Ok(())
}

pub fn validate_message(message: &v1::Message) -> Result<(), ProtocolError> {
    validate_id(&message.message_id)?;
    validate_id(&message.conversation_id)?;
    validate_id(&message.sender_device_id)?;
    if message.sent_at_ms == 0 { return Err(ProtocolError::Invalid("sent_at_ms")); }
    if message.encoded_len() > MAX_MESSAGE_BYTES { return Err(ProtocolError::TooLarge); }
    match message.content.as_ref().ok_or(ProtocolError::Invalid("content"))? {
        v1::message::Content::Text(text) if text.is_empty() => Err(ProtocolError::Invalid("text")),
        v1::message::Content::Text(_) => Ok(()),
        v1::message::Content::Media(media) => {
            validate_id(&media.attachment_id)?;
            if media.mime_type.is_empty() || media.ciphertext_size_bytes == 0
                || media.content_key.is_empty() || media.nonce.is_empty()
                || media.ciphertext_sha256.len() != 32
                || media.width == Some(0) || media.height == Some(0)
            { return Err(ProtocolError::Invalid("media")); }
            // Algorithm-specific key and nonce sizes are the crypto provider's responsibility.
            Ok(())
        }
        v1::message::Content::Receipts(receipts) => {
            if !matches!(v1::receipts::Kind::try_from(receipts.kind), Ok(v1::receipts::Kind::Delivered | v1::receipts::Kind::Read))
                || receipts.observed_at_ms == 0 || receipts.message_ids.is_empty() || receipts.message_ids.len() > 100
            { return Err(ProtocolError::Invalid("receipts")); }
            let mut seen = std::collections::HashSet::new();
            for id in &receipts.message_ids {
                validate_id(id)?;
                if !seen.insert(id) { return Err(ProtocolError::Invalid("duplicate receipt")); }
            }
            Ok(())
        }
    }
}

/// Validate storage/routing shape without treating expiration as a wire error on replay.
pub fn validate_envelope(envelope: &v1::Envelope) -> Result<(), ProtocolError> {
    if envelope.protocol_version != VERSION { return Err(ProtocolError::UnsupportedVersion); }
    validate_id(&envelope.envelope_id)?;
    validate_id(&envelope.recipient_device_id)?;
    if envelope.sealed_payload.is_empty() || envelope.expires_at_ms == 0 {
        return Err(ProtocolError::Invalid("envelope"));
    }
    if envelope.encoded_len() > MAX_ENVELOPE_BYTES { return Err(ProtocolError::TooLarge); }
    Ok(())
}

/// Called when first accepting an envelope; never extend expiry on retry.
pub fn validate_enqueue(envelope: &v1::Envelope, now_ms: u64) -> Result<(), ProtocolError> {
    validate_envelope(envelope)?;
    if envelope.expires_at_ms <= now_ms || envelope.expires_at_ms - now_ms > MAX_RETENTION_MS {
        return Err(ProtocolError::InvalidRetention);
    }
    Ok(())
}

pub fn decode_envelope(bytes: &[u8]) -> Result<v1::Envelope, ProtocolError> {
    if bytes.len() > MAX_ENVELOPE_BYTES { return Err(ProtocolError::TooLarge); }
    let envelope = v1::Envelope::decode(bytes).map_err(|_| ProtocolError::Malformed)?;
    validate_envelope(&envelope)?;
    Ok(envelope)
}

pub fn decode_message(bytes: &[u8]) -> Result<v1::Message, ProtocolError> {
    if bytes.len() > MAX_MESSAGE_BYTES { return Err(ProtocolError::TooLarge); }
    let message = v1::Message::decode(bytes).map_err(|_| ProtocolError::Malformed)?;
    validate_message(&message)?;
    Ok(message)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn envelope() -> v1::Envelope {
        v1::Envelope { protocol_version: VERSION, envelope_id: "00000000-0000-4000-8000-000000000001".into(), recipient_device_id: "00000000-0000-4000-8000-000000000002".into(), expires_at_ms: 1000, sealed_payload: vec![7, 8] }
    }
    #[test]
    fn wire_roundtrip_and_unknown_fields() {
        let e = envelope();
        let mut wire = e.encode_to_vec();
        wire.extend_from_slice(&[0xa0, 0x06, 0x01]); // Unknown field 100; forward-compatible.
        assert!(decode_envelope(&wire).unwrap() == e);
        assert!(!DESCRIPTOR_SET.is_empty());
    }
    #[test]
    fn stable_receipt_wire_fixture() {
        let r = v1::Receipts { kind: 1, message_ids: vec!["id".into()], observed_at_ms: 150 };
        assert_eq!(r.encode_to_vec(), vec![8, 1, 18, 2, b'i', b'd', 24, 150, 1]);
    }
    #[test]
    fn rejects_invalid_version_size_retention_and_wire() {
        let mut e = envelope();
        assert!(validate_enqueue(&e, 999).is_ok());
        assert_eq!(validate_enqueue(&e, 1000), Err(ProtocolError::InvalidRetention));
        e.expires_at_ms = MAX_RETENTION_MS + 1;
        assert_eq!(validate_enqueue(&e, 0), Err(ProtocolError::InvalidRetention));
        e.protocol_version = 2;
        assert_eq!(validate_envelope(&e), Err(ProtocolError::UnsupportedVersion));
        e.protocol_version = 1;
        e.sealed_payload = vec![0; MAX_ENVELOPE_BYTES];
        assert_eq!(validate_envelope(&e), Err(ProtocolError::TooLarge));
        assert!(matches!(decode_envelope(&[0xff]), Err(ProtocolError::Malformed)));
    }
    #[test]
    fn strict_ids_and_handles() {
        for id in ["", "00000000-0000-0000-0000-000000000000", "00000000000040008000000000000001"] { assert!(validate_id(id).is_err()); }
        for handle in ["ab", "UPPER", "1user", "with space", "éclair"] { assert!(validate_handle(handle).is_err()); }
        assert!(validate_handle("micky_1").is_ok());
    }
    #[test]
    fn private_message_validation() {
        let mut m = v1::Message { message_id: envelope().envelope_id, conversation_id: envelope().recipient_device_id.clone(), sender_device_id: envelope().recipient_device_id, sent_at_ms: 1, content: Some(v1::message::Content::Text("hello".into())) };
        assert!(decode_message(&m.encode_to_vec()).unwrap() == m);
        m.content = None;
        assert!(validate_message(&m).is_err());
        m.content = Some(v1::message::Content::Receipts(v1::Receipts { kind: 99, message_ids: vec![m.message_id.clone()], observed_at_ms: 1 }));
        assert!(validate_message(&m).is_err());
    }
}
