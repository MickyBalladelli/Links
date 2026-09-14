//! Shared wire types. Generated types deliberately have no Debug implementation.
//! Message, media, receipt, and authentication bytes must never be logged.
use prost::Message as ProstMessage;
use thiserror::Error;

pub mod v1 {
    include!(concat!(env!("OUT_DIR"), "/links.v1.rs"));
}
pub mod contact_psi;
pub mod privacy_pass;
pub mod proof_of_work;
pub mod sync_compression;
pub use sync_compression::{
    compress_sync_batch, decompress_sync_batch, validate_sync_batch,
    SYNC_COMPRESSION_DICTIONARY_ID_V1, SYNC_COMPRESSION_LEVEL, SYNC_COMPRESSION_ZSTD_DICTIONARY_V1,
    SYNC_ZSTD_DICTIONARY_V1,
};
pub const DESCRIPTOR_SET: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/links.bin"));
pub const VERSION: u32 = 1;
pub const MAX_ENVELOPE_BYTES: usize = 256 * 1024;
pub const MAX_MESSAGE_BYTES: usize = 64 * 1024;
pub const MAX_BATCH_ITEMS: usize = 100;
pub const MAX_FANOUT_DEVICES: usize = 100;
pub const MAX_FRAME_BYTES: usize = 1024 * 1024;
pub const MAX_QUEUE_MESSAGE_BYTES: usize = 512 * 1024;
pub const MAX_PREKEY_UPLOAD_BYTES: usize = 256 * 1024;
pub const MAX_ONE_TIME_PREKEYS: usize = 100;
pub const OPUS_MIN_BITRATE_KBPS: u32 = 16;
pub const OPUS_MAX_BITRATE_KBPS: u32 = 24;
pub const OPUS_SAMPLE_RATE_HZ: [u32; 5] = [8_000, 12_000, 16_000, 24_000, 48_000];
pub const OPUS_CHANNELS: [u32; 2] = [1, 2];
pub const OPUS_FRAME_DURATION_MS: u32 = 20;
pub const BLUR_HASH_LENGTH: usize = 28;
pub const ML_KEM_768_PUBLIC_KEY_BYTES: usize = 1184;
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
        || !value
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'_')
        || !value.as_bytes()[0].is_ascii_lowercase()
    {
        return Err(ProtocolError::Invalid("handle"));
    }
    Ok(())
}

pub fn validate_blur_hash(value: &str) -> Result<(), ProtocolError> {
    const BASE83: &[u8; 83] =
        b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz#$%*+,-.:;=?@[]^_{|}~";
    if value.len() != BLUR_HASH_LENGTH
        || !value.is_ascii()
        || !value.bytes().all(|byte| BASE83.contains(&byte))
        || value.as_bytes()[0] != b'L'
    {
        return Err(ProtocolError::Invalid("blur_hash"));
    }
    Ok(())
}

pub fn validate_user(user: &v1::User) -> Result<(), ProtocolError> {
    validate_id(&user.user_id)?;
    if let Some(handle) = &user.handle {
        validate_handle(handle)?;
    }
    let mut devices = std::collections::HashSet::new();
    for device in &user.devices {
        validate_id(&device.device_id)?;
        if device.user_id != user.user_id
            || !devices.insert(&device.device_id)
            || device.identity_public_key.is_empty()
            || device.mls_credential.is_empty()
            || device.registered_at_ms == 0
            || device
                .revoked_at_ms
                .is_some_and(|time| time < device.registered_at_ms)
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
    if message.sent_at_ms == 0 {
        return Err(ProtocolError::Invalid("sent_at_ms"));
    }
    if message.sequence_id == 0 || message.sequence_id > MAX_CURSOR {
        return Err(ProtocolError::Invalid("sequence_id"));
    }
    if message.encoded_len() > MAX_MESSAGE_BYTES {
        return Err(ProtocolError::TooLarge);
    }
    match message
        .content
        .as_ref()
        .ok_or(ProtocolError::Invalid("content"))?
    {
        v1::message::Content::Text(text) if text.is_empty() => Err(ProtocolError::Invalid("text")),
        v1::message::Content::Text(_) => Ok(()),
        v1::message::Content::Media(media) => validate_media_metadata(media),
        v1::message::Content::Receipts(receipts) => {
            if !matches!(
                v1::receipts::Kind::try_from(receipts.kind),
                Ok(v1::receipts::Kind::Delivered | v1::receipts::Kind::Read)
            ) || receipts.observed_at_ms == 0
                || receipts.message_ids.is_empty()
                || receipts.message_ids.len() > 100
            {
                return Err(ProtocolError::Invalid("receipts"));
            }
            let mut seen = std::collections::HashSet::new();
            for id in &receipts.message_ids {
                validate_id(id)?;
                if !seen.insert(id) {
                    return Err(ProtocolError::Invalid("duplicate receipt"));
                }
            }
            Ok(())
        }
        v1::message::Content::BroadcastPost(post) => validate_broadcast_post(post),
    }
}

pub fn validate_broadcast_post(post: &v1::BroadcastPost) -> Result<(), ProtocolError> {
    validate_id(&post.post_id)?;
    validate_id(&post.admin_device_id)?;
    if post.admin_public_key.len() != 32
        || post.payload.is_empty()
        || post.payload.len() > MAX_MESSAGE_BYTES
        || post.signature.len() != 64
    {
        return Err(ProtocolError::Invalid("broadcast post"));
    }
    Ok(())
}

/// Validate the public broker wrapper while leaving the broadcast payload
/// opaque. Only the client holding the broadcast master key can open it.
pub fn validate_broadcast_dispatch(dispatch: &v1::BroadcastDispatch) -> Result<(), ProtocolError> {
    if dispatch.protocol_version != VERSION {
        return Err(ProtocolError::UnsupportedVersion);
    }
    validate_id(&dispatch.conversation_id)?;
    validate_id(&dispatch.post_id)?;
    if dispatch.epoch > MAX_CURSOR || dispatch.ciphertext.is_empty() {
        return Err(ProtocolError::Invalid("broadcast dispatch"));
    }
    if dispatch.encoded_len() > MAX_QUEUE_MESSAGE_BYTES {
        return Err(ProtocolError::TooLarge);
    }
    Ok(())
}

pub fn validate_opus_audio_metadata(audio: &v1::OpusAudioMetadata) -> Result<(), ProtocolError> {
    if !matches!(
        v1::opus_audio_metadata::Container::try_from(audio.container),
        Ok(v1::opus_audio_metadata::Container::Ogg | v1::opus_audio_metadata::Container::Opus)
    ) || !(OPUS_MIN_BITRATE_KBPS..=OPUS_MAX_BITRATE_KBPS).contains(&audio.bitrate_kbps)
        || !OPUS_SAMPLE_RATE_HZ.contains(&audio.sample_rate_hz)
        || !OPUS_CHANNELS.contains(&audio.channels)
        || audio.frame_duration_ms != OPUS_FRAME_DURATION_MS
    {
        return Err(ProtocolError::Invalid("opus audio"));
    }
    Ok(())
}

pub fn validate_media_metadata(media: &v1::MediaMetadata) -> Result<(), ProtocolError> {
    validate_id(&media.attachment_id)?;
    if media.mime_type.is_empty()
        || media.ciphertext_size_bytes == 0
        || media.content_key.is_empty()
        || media.nonce.is_empty()
        || media.ciphertext_sha256.len() != 32
        || media.width == Some(0)
        || media.height == Some(0)
    {
        return Err(ProtocolError::Invalid("media"));
    }
    if let Some(blur_hash) = media.blur_hash.as_deref() {
        validate_blur_hash(blur_hash)?;
    }
    match (media.original_size_bytes, media.encryption_chunk_bytes) {
        (None, None) => {}
        (Some(original_size), Some(chunk_size))
            if original_size > 0 && chunk_size > 0 && chunk_size <= 256 * 1024 => {}
        _ => return Err(ProtocolError::Invalid("chunked media metadata")),
    }
    if let Some(opus) = media.opus.as_ref() {
        validate_opus_audio_metadata(opus)?;
        let expected_mime = match v1::opus_audio_metadata::Container::try_from(opus.container) {
            Ok(v1::opus_audio_metadata::Container::Ogg) => "audio/ogg",
            Ok(v1::opus_audio_metadata::Container::Opus) => "audio/ogg; codecs=opus",
            _ => return Err(ProtocolError::Invalid("opus container")),
        };
        if media.mime_type != expected_mime
            || media.duration_ms.is_none_or(|duration| duration == 0)
        {
            return Err(ProtocolError::Invalid("opus media"));
        }
    }
    // Algorithm-specific key and nonce sizes are the crypto provider's responsibility.
    Ok(())
}

/// Validate storage/routing shape without treating expiration as a wire error on replay.
pub fn validate_envelope(envelope: &v1::Envelope) -> Result<(), ProtocolError> {
    if envelope.protocol_version != VERSION {
        return Err(ProtocolError::UnsupportedVersion);
    }
    validate_id(&envelope.envelope_id)?;
    validate_id(&envelope.recipient_device_id)?;
    if envelope.sealed_payload.is_empty() || envelope.expires_at_ms == 0 {
        return Err(ProtocolError::Invalid("envelope"));
    }
    if envelope.encoded_len() > MAX_ENVELOPE_BYTES {
        return Err(ProtocolError::TooLarge);
    }
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
    if bytes.len() > MAX_ENVELOPE_BYTES {
        return Err(ProtocolError::TooLarge);
    }
    let envelope = v1::Envelope::decode(bytes).map_err(|_| ProtocolError::Malformed)?;
    validate_envelope(&envelope)?;
    Ok(envelope)
}

/// Validate the cross-gateway queue wrapper. This checks only routing and
/// protobuf boundaries; sealed payload bytes remain opaque to the server.
pub fn validate_gateway_delivery(delivery: &v1::GatewayDelivery) -> Result<(), ProtocolError> {
    if delivery.protocol_version != VERSION {
        return Err(ProtocolError::UnsupportedVersion);
    }
    validate_gateway_locator(&delivery.source_gateway_id)?;
    validate_gateway_locator(&delivery.destination_gateway_id)?;
    if delivery.source_gateway_id == delivery.destination_gateway_id {
        return Err(ProtocolError::Invalid("same gateway"));
    }
    if delivery.cursor == 0 || delivery.cursor > MAX_CURSOR {
        return Err(ProtocolError::Invalid("queue cursor"));
    }
    if delivery.serialized_envelope.is_empty() {
        return Err(ProtocolError::Invalid("queue envelope"));
    }
    if delivery.encoded_len() > MAX_QUEUE_MESSAGE_BYTES {
        return Err(ProtocolError::TooLarge);
    }
    decode_envelope(&delivery.serialized_envelope)?;
    Ok(())
}

pub fn decode_gateway_delivery(bytes: &[u8]) -> Result<v1::GatewayDelivery, ProtocolError> {
    if bytes.is_empty() || bytes.len() > MAX_QUEUE_MESSAGE_BYTES {
        return Err(if bytes.len() > MAX_QUEUE_MESSAGE_BYTES {
            ProtocolError::TooLarge
        } else {
            ProtocolError::Malformed
        });
    }
    let delivery = v1::GatewayDelivery::decode(bytes).map_err(|_| ProtocolError::Malformed)?;
    validate_gateway_delivery(&delivery)?;
    Ok(delivery)
}

pub fn decode_message(bytes: &[u8]) -> Result<v1::Message, ProtocolError> {
    if bytes.len() > MAX_MESSAGE_BYTES {
        return Err(ProtocolError::TooLarge);
    }
    let message = v1::Message::decode(bytes).map_err(|_| ProtocolError::Malformed)?;
    validate_message(&message)?;
    Ok(message)
}

pub fn validate_gateway_locator(value: &str) -> Result<(), ProtocolError> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b':' | b'_' | b'-' | b'.'))
    {
        return Err(ProtocolError::Invalid("gateway locator"));
    }
    Ok(())
}

pub fn validate_prekey_upload(upload: &v1::PreKeyUpload) -> Result<(), ProtocolError> {
    if upload.protocol_version != VERSION {
        return Err(ProtocolError::UnsupportedVersion);
    }
    validate_id(&upload.device_id)?;
    validate_id(&upload.upload_id)?;
    if upload.profile_revision == 0
        || upload.profile_revision > MAX_CURSOR
        || upload.encoded_len() > MAX_PREKEY_UPLOAD_BYTES
        || upload.one_time_curve_prekeys.len() > MAX_ONE_TIME_PREKEYS
        || upload.one_time_kem_prekeys.len() > MAX_ONE_TIME_PREKEYS
    {
        return Err(ProtocolError::Invalid("prekey upload"));
    }
    let profile = upload
        .profile
        .as_ref()
        .ok_or(ProtocolError::Invalid("prekey profile"))?;
    validate_prekey_profile(profile)?;
    let signed_id = profile
        .signed_prekey
        .as_ref()
        .and_then(|key| key.prekey.as_ref())
        .map(|key| key.id)
        .ok_or(ProtocolError::Invalid("signed prekey"))?;
    let last_resort_id = profile
        .last_resort_kem_prekey
        .as_ref()
        .map(|key| key.id)
        .ok_or(ProtocolError::Invalid("last-resort KEM prekey"))?;
    let mut curve_ids = std::collections::HashSet::new();
    curve_ids.insert(signed_id);
    for key in &upload.one_time_curve_prekeys {
        validate_curve_prekey(key)?;
        if !curve_ids.insert(key.id) {
            return Err(ProtocolError::Invalid("duplicate curve prekey"));
        }
    }
    let mut kem_ids = std::collections::HashSet::new();
    kem_ids.insert(last_resort_id);
    for key in &upload.one_time_kem_prekeys {
        validate_kem_prekey(key, true)?;
        if !kem_ids.insert(key.id) {
            return Err(ProtocolError::Invalid("duplicate KEM prekey"));
        }
    }
    Ok(())
}

pub fn validate_prekey_bundle(bundle: &v1::PreKeyBundle) -> Result<(), ProtocolError> {
    if bundle.protocol_version != VERSION {
        return Err(ProtocolError::UnsupportedVersion);
    }
    validate_id(&bundle.device_id)?;
    if bundle.profile_revision == 0 || bundle.profile_revision > MAX_CURSOR {
        return Err(ProtocolError::Invalid("prekey bundle"));
    }
    let profile = bundle
        .profile
        .as_ref()
        .ok_or(ProtocolError::Invalid("prekey profile"))?;
    validate_prekey_profile(profile)?;
    let signed_id = profile
        .signed_prekey
        .as_ref()
        .and_then(|key| key.prekey.as_ref())
        .map(|key| key.id)
        .ok_or(ProtocolError::Invalid("signed prekey"))?;
    if let Some(key) = &bundle.one_time_curve_prekey {
        validate_curve_prekey(key)?;
        if key.id == signed_id {
            return Err(ProtocolError::Invalid("duplicate curve prekey"));
        }
    }
    let kem = bundle
        .kem_prekey
        .as_ref()
        .ok_or(ProtocolError::Invalid("KEM prekey"))?;
    validate_kem_prekey(kem, kem.one_time)?;
    let last_resort = profile
        .last_resort_kem_prekey
        .as_ref()
        .ok_or(ProtocolError::Invalid("last-resort KEM prekey"))?;
    if (!kem.one_time && kem != last_resort) || (kem.one_time && kem.id == last_resort.id) {
        return Err(ProtocolError::Invalid("KEM prekey selection"));
    }
    Ok(())
}

fn validate_prekey_profile(profile: &v1::PreKeyProfile) -> Result<(), ProtocolError> {
    let identity = profile
        .identity
        .as_ref()
        .ok_or(ProtocolError::Invalid("PQXDH identity"))?;
    if identity.signing_key.len() != 32
        || identity.dh_key.len() != 32
        || identity.dh_key.iter().all(|byte| *byte == 0)
        || identity.binding_signature.len() != 64
    {
        return Err(ProtocolError::Invalid("PQXDH identity"));
    }
    let signed = profile
        .signed_prekey
        .as_ref()
        .ok_or(ProtocolError::Invalid("signed prekey"))?;
    validate_curve_prekey(
        signed
            .prekey
            .as_ref()
            .ok_or(ProtocolError::Invalid("signed prekey"))?,
    )?;
    if signed.signature.len() != 64 {
        return Err(ProtocolError::Invalid("signed prekey signature"));
    }
    validate_kem_prekey(
        profile
            .last_resort_kem_prekey
            .as_ref()
            .ok_or(ProtocolError::Invalid("last-resort KEM prekey"))?,
        false,
    )
}

fn validate_curve_prekey(key: &v1::CurvePreKey) -> Result<(), ProtocolError> {
    if key.id == 0
        || key.id > MAX_CURSOR
        || key.public_key.len() != 32
        || key.public_key.iter().all(|byte| *byte == 0)
    {
        return Err(ProtocolError::Invalid("curve prekey"));
    }
    Ok(())
}

fn validate_kem_prekey(key: &v1::KemPreKey, one_time: bool) -> Result<(), ProtocolError> {
    if key.id == 0
        || key.id > MAX_CURSOR
        || key.one_time != one_time
        || key.public_key.len() != ML_KEM_768_PUBLIC_KEY_BYTES
        || key.signature.len() != 64
    {
        return Err(ProtocolError::Invalid("KEM prekey"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn envelope() -> v1::Envelope {
        v1::Envelope {
            protocol_version: VERSION,
            envelope_id: "00000000-0000-4000-8000-000000000001".into(),
            recipient_device_id: "00000000-0000-4000-8000-000000000002".into(),
            expires_at_ms: 1000,
            sealed_payload: vec![7, 8],
        }
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
        let r = v1::Receipts {
            kind: 1,
            message_ids: vec!["id".into()],
            observed_at_ms: 150,
        };
        assert_eq!(r.encode_to_vec(), vec![8, 1, 18, 2, b'i', b'd', 24, 150, 1]);
    }
    #[test]
    fn rejects_invalid_version_size_retention_and_wire() {
        let mut e = envelope();
        assert!(validate_enqueue(&e, 999).is_ok());
        assert_eq!(
            validate_enqueue(&e, 1000),
            Err(ProtocolError::InvalidRetention)
        );
        e.expires_at_ms = MAX_RETENTION_MS + 1;
        assert_eq!(
            validate_enqueue(&e, 0),
            Err(ProtocolError::InvalidRetention)
        );
        e.protocol_version = 2;
        assert_eq!(
            validate_envelope(&e),
            Err(ProtocolError::UnsupportedVersion)
        );
        e.protocol_version = 1;
        e.sealed_payload = vec![0; MAX_ENVELOPE_BYTES];
        assert_eq!(validate_envelope(&e), Err(ProtocolError::TooLarge));
        assert!(matches!(
            decode_envelope(&[0xff]),
            Err(ProtocolError::Malformed)
        ));
    }
    #[test]
    fn strict_ids_and_handles() {
        for id in [
            "",
            "00000000-0000-0000-0000-000000000000",
            "00000000000040008000000000000001",
        ] {
            assert!(validate_id(id).is_err());
        }
        for handle in ["ab", "UPPER", "1user", "with space", "éclair"] {
            assert!(validate_handle(handle).is_err());
        }
        assert!(validate_handle("micky_1").is_ok());
    }
    #[test]
    fn private_message_validation() {
        let mut m = v1::Message {
            message_id: envelope().envelope_id,
            conversation_id: envelope().recipient_device_id.clone(),
            sender_device_id: envelope().recipient_device_id,
            sent_at_ms: 1,
            sequence_id: 1,
            content: Some(v1::message::Content::Text("hello".into())),
        };
        assert!(decode_message(&m.encode_to_vec()).unwrap() == m);
        m.content = None;
        assert!(validate_message(&m).is_err());
        m.content = Some(v1::message::Content::Receipts(v1::Receipts {
            kind: 99,
            message_ids: vec![m.message_id.clone()],
            observed_at_ms: 1,
        }));
        assert!(validate_message(&m).is_err());
    }
}
