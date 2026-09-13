//! Client-side encrypted attachment preparation.
//!
//! Attachment bytes are encrypted before upload. The returned media metadata
//! is private Message content; only the ciphertext and opaque attachment ID
//! may cross the upload boundary.

use crate::{
    crypto::SecretBytes,
    images,
    protocol::{self, v1},
    voice::{self, OpusVoiceProfile},
    CoreError,
};
use chacha20poly1305::{
    aead::{Aead, KeyInit, Payload},
    ChaCha20Poly1305, Key, Nonce,
};
use sha2::{Digest, Sha256};

pub const ATTACHMENT_KEY_BYTES: usize = 32;
pub const ATTACHMENT_NONCE_BYTES: usize = 12;
pub const ATTACHMENT_TAG_BYTES: usize = 16;
pub const VOICE_ATTACHMENT_AAD_PREFIX: &[u8] = b"links/voice-note/attachment/v1\0";
pub const IMAGE_ATTACHMENT_AAD_PREFIX: &[u8] = b"links/image/attachment/v1\0";
pub const MAX_IMAGE_BYTES: usize = 32 * 1024 * 1024;

/// A complete encrypted voice attachment. `media` is put inside the MLS
/// message; `ciphertext` is uploaded to opaque blob storage.
pub struct EncryptedVoiceNote {
    pub media: v1::MediaMetadata,
    pub ciphertext: Vec<u8>,
}

impl EncryptedVoiceNote {
    pub fn new(media: v1::MediaMetadata, ciphertext: Vec<u8>) -> Result<Self, CoreError> {
        protocol::validate_media_metadata(&media)?;
        validate_ciphertext(
            &media,
            &ciphertext,
            voice::MAX_VOICE_NOTE_BYTES + ATTACHMENT_TAG_BYTES,
        )?;
        Ok(Self { media, ciphertext })
    }
}

/// A complete encrypted image attachment. The media metadata is sent inside
/// MLS; the ciphertext alone crosses the upload boundary.
pub struct EncryptedImage {
    pub media: v1::MediaMetadata,
    pub ciphertext: Vec<u8>,
}

impl EncryptedImage {
    pub fn new(media: v1::MediaMetadata, ciphertext: Vec<u8>) -> Result<Self, CoreError> {
        validate_image_metadata(&media)?;
        validate_ciphertext(
            &media,
            &ciphertext,
            MAX_IMAGE_BYTES + ATTACHMENT_TAG_BYTES,
        )?;
        Ok(Self { media, ciphertext })
    }
}

/// Encrypt a validated Ogg Opus container with a fresh random content key.
/// The content key and nonce are returned only as private Message metadata.
pub fn encrypt_voice_note(
    attachment_id: String,
    plaintext: &[u8],
    profile: OpusVoiceProfile,
    duration_ms: u64,
) -> Result<EncryptedVoiceNote, CoreError> {
    protocol::validate_id(&attachment_id)?;
    profile.validate()?;
    if duration_ms == 0 {
        return Err(CoreError::Protocol(protocol::ProtocolError::Invalid(
            "voice duration",
        )));
    }
    let stream = voice::validate_ogg_opus(plaintext)?;
    if stream.channels != profile.channels || stream.input_sample_rate_hz != profile.sample_rate_hz {
        return Err(CoreError::Voice(voice::VoiceError::InvalidConfiguration));
    }

    let mut content_key = [0u8; ATTACHMENT_KEY_BYTES];
    let mut nonce = [0u8; ATTACHMENT_NONCE_BYTES];
    getrandom::fill(&mut content_key).map_err(|_| CoreError::Provider)?;
    getrandom::fill(&mut nonce).map_err(|_| CoreError::Provider)?;
    let ciphertext = encrypt_bytes(
        &content_key,
        &nonce,
        &attachment_id,
        plaintext,
        VOICE_ATTACHMENT_AAD_PREFIX,
    )?;
    let media = profile.media_metadata(
        attachment_id,
        ciphertext.len() as u64,
        content_key.to_vec(),
        nonce.to_vec(),
        Sha256::digest(&ciphertext).to_vec(),
        duration_ms,
    )?;
    EncryptedVoiceNote::new(media, ciphertext)
}

/// Validate the server response and decrypt an attachment only after its
/// authenticated metadata, size, digest, and Opus stream all agree.
pub fn decrypt_voice_note(
    media: &v1::MediaMetadata,
    ciphertext: &[u8],
) -> Result<SecretBytes, CoreError> {
    protocol::validate_media_metadata(media)?;
    let opus = media
        .opus
        .as_ref()
        .ok_or(CoreError::Protocol(protocol::ProtocolError::Invalid(
            "voice metadata",
        )))?;
    let profile = OpusVoiceProfile::from_proto(opus)?;
    validate_ciphertext(
        media,
        ciphertext,
        voice::MAX_VOICE_NOTE_BYTES + ATTACHMENT_TAG_BYTES,
    )?;
    let plaintext = decrypt_bytes(
        &media.content_key,
        &media.nonce,
        &media.attachment_id,
        ciphertext,
        VOICE_ATTACHMENT_AAD_PREFIX,
    )?;
    let stream = voice::validate_ogg_opus(&plaintext)?;
    if stream.channels != profile.channels || stream.input_sample_rate_hz != profile.sample_rate_hz {
        return Err(CoreError::Authentication);
    }
    Ok(SecretBytes::new(plaintext))
}

/// Encrypt a resized/transcoded image. The caller supplies RGB pixels only to
/// produce the private BlurHash; those pixels are never stored or uploaded.
pub fn encrypt_image(
    attachment_id: String,
    plaintext: &[u8],
    mime_type: String,
    width: u32,
    height: u32,
    blur_hash: String,
) -> Result<EncryptedImage, CoreError> {
    protocol::validate_id(&attachment_id)?;
    if !matches!(mime_type.as_str(), "image/webp" | "image/avif")
        || plaintext.is_empty()
        || plaintext.len() > MAX_IMAGE_BYTES
    {
        return Err(CoreError::Protocol(protocol::ProtocolError::Invalid(
            "image",
        )));
    }
    images::resized_dimensions(width, height)?;
    protocol::validate_blur_hash(&blur_hash)?;

    let mut content_key = [0u8; ATTACHMENT_KEY_BYTES];
    let mut nonce = [0u8; ATTACHMENT_NONCE_BYTES];
    getrandom::fill(&mut content_key).map_err(|_| CoreError::Provider)?;
    getrandom::fill(&mut nonce).map_err(|_| CoreError::Provider)?;
    let ciphertext = encrypt_bytes(
        &content_key,
        &nonce,
        &attachment_id,
        plaintext,
        IMAGE_ATTACHMENT_AAD_PREFIX,
    )?;
    let media = v1::MediaMetadata {
        attachment_id,
        mime_type,
        ciphertext_size_bytes: ciphertext.len() as u64,
        content_key: content_key.to_vec(),
        nonce: nonce.to_vec(),
        ciphertext_sha256: Sha256::digest(&ciphertext).to_vec(),
        width: Some(width),
        height: Some(height),
        duration_ms: None,
        blur_hash: Some(blur_hash),
        opus: None,
    };
    EncryptedImage::new(media, ciphertext)
}

/// Verify and decrypt an image only after its private metadata and ciphertext
/// receipt agree.
pub fn decrypt_image(
    media: &v1::MediaMetadata,
    ciphertext: &[u8],
) -> Result<SecretBytes, CoreError> {
    validate_image_metadata(media)?;
    validate_ciphertext(
        media,
        ciphertext,
        MAX_IMAGE_BYTES + ATTACHMENT_TAG_BYTES,
    )?;
    let plaintext = decrypt_bytes(
        &media.content_key,
        &media.nonce,
        &media.attachment_id,
        ciphertext,
        IMAGE_ATTACHMENT_AAD_PREFIX,
    )?;
    Ok(SecretBytes::new(plaintext))
}

fn validate_image_metadata(media: &v1::MediaMetadata) -> Result<(), CoreError> {
    protocol::validate_media_metadata(media)?;
    if !matches!(media.mime_type.as_str(), "image/webp" | "image/avif")
        || media.opus.is_some()
        || media.duration_ms.is_some()
        || media.width.is_none()
        || media.height.is_none()
        || media.blur_hash.is_none()
    {
        return Err(CoreError::Protocol(protocol::ProtocolError::Invalid(
            "image metadata",
        )));
    }
    let width = media.width.unwrap();
    let height = media.height.unwrap();
    let dimensions = images::resized_dimensions(width, height)?;
    if dimensions.width != width || dimensions.height != height {
        return Err(CoreError::Image(images::ImageError::InvalidDimensions));
    }
    Ok(())
}

fn validate_ciphertext(
    media: &v1::MediaMetadata,
    ciphertext: &[u8],
        maximum_ciphertext_bytes: usize,
) -> Result<(), CoreError> {
    if media.content_key.len() != ATTACHMENT_KEY_BYTES
        || media.nonce.len() != ATTACHMENT_NONCE_BYTES
        || media.ciphertext_size_bytes != ciphertext.len() as u64
        || ciphertext.len() < ATTACHMENT_TAG_BYTES
        || ciphertext.len() > maximum_ciphertext_bytes
        || Sha256::digest(ciphertext).as_slice() != media.ciphertext_sha256.as_slice()
    {
        return Err(CoreError::Authentication);
    }
    Ok(())
}

fn attachment_aad(attachment_id: &str, prefix: &[u8]) -> Vec<u8> {
    let mut aad = Vec::with_capacity(prefix.len() + attachment_id.len());
    aad.extend_from_slice(prefix);
    aad.extend_from_slice(attachment_id.as_bytes());
    aad
}

fn encrypt_bytes(
    key: &[u8],
    nonce: &[u8],
    attachment_id: &str,
    plaintext: &[u8],
    prefix: &[u8],
) -> Result<Vec<u8>, CoreError> {
    let cipher = ChaCha20Poly1305::new(Key::from_slice(key));
    cipher
        .encrypt(
            Nonce::from_slice(nonce),
            Payload {
                msg: plaintext,
                aad: &attachment_aad(attachment_id, prefix),
            },
        )
        .map_err(|_| CoreError::Provider)
}

fn decrypt_bytes(
    key: &[u8],
    nonce: &[u8],
    attachment_id: &str,
    ciphertext: &[u8],
    prefix: &[u8],
) -> Result<Vec<u8>, CoreError> {
    let cipher = ChaCha20Poly1305::new(Key::from_slice(key));
    cipher
        .decrypt(
            Nonce::from_slice(nonce),
            Payload {
                msg: ciphertext,
                aad: &attachment_aad(attachment_id, prefix),
            },
        )
        .map_err(|_| CoreError::Authentication)
}
