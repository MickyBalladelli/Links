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
use std::io::{Read, Write};
use zeroize::Zeroizing;

pub const ATTACHMENT_KEY_BYTES: usize = 32;
pub const ATTACHMENT_NONCE_BYTES: usize = 12;
pub const ATTACHMENT_TAG_BYTES: usize = 16;
pub const VOICE_ATTACHMENT_AAD_PREFIX: &[u8] = b"links/voice-note/attachment/v1\0";
pub const IMAGE_ATTACHMENT_AAD_PREFIX: &[u8] = b"links/image/attachment/v1\0";
pub const LARGE_FILE_ATTACHMENT_AAD_PREFIX: &[u8] = b"links/large-file/attachment/v1\0";
pub const MAX_IMAGE_BYTES: usize = 32 * 1024 * 1024;
pub const LARGE_FILE_CIPHERTEXT_CHUNK_BYTES: usize = 256 * 1024;
pub const LARGE_FILE_PLAINTEXT_CHUNK_BYTES: usize =
    LARGE_FILE_CIPHERTEXT_CHUNK_BYTES - ATTACHMENT_TAG_BYTES;

/// The private metadata and final digest for a chunk-encrypted attachment.
/// The ciphertext itself is streamed separately over blob or WebRTC transport.
pub struct EncryptedLargeFile {
    pub media: v1::MediaMetadata,
}

/// Bounded streaming encryptor for MP4 and other large files. It never stores
/// the complete plaintext or ciphertext in memory.
pub struct LargeFileEncryptor {
    attachment_id: String,
    mime_type: String,
    width: Option<u32>,
    height: Option<u32>,
    duration_ms: Option<u64>,
    content_key: Zeroizing<[u8; ATTACHMENT_KEY_BYTES]>,
    nonce: [u8; ATTACHMENT_NONCE_BYTES],
    plaintext_size: u64,
    ciphertext_size: u64,
    ciphertext_sha256: Sha256,
    chunk_index: u64,
    finished: bool,
}

impl LargeFileEncryptor {
    /// Create a video/file encryptor with a fresh attachment key and nonce.
    pub fn new(
        attachment_id: String,
        mime_type: String,
        width: Option<u32>,
        height: Option<u32>,
        duration_ms: Option<u64>,
    ) -> Result<Self, CoreError> {
        protocol::validate_id(&attachment_id)?;
        let valid_shape = match mime_type.as_str() {
            "video/mp4" => {
                width.is_some_and(|width| width > 0)
                    && height.is_some_and(|height| height > 0)
                    && duration_ms.is_some_and(|duration| duration > 0)
            }
            "application/octet-stream" => {
                width.is_none() && height.is_none() && duration_ms.is_none()
            }
            _ => false,
        };
        if !valid_shape {
            return Err(CoreError::Protocol(protocol::ProtocolError::Invalid(
                "large file",
            )));
        }
        let mut content_key = Zeroizing::new([0u8; ATTACHMENT_KEY_BYTES]);
        let mut nonce = [0u8; ATTACHMENT_NONCE_BYTES];
        getrandom::fill(content_key.as_mut()).map_err(|_| CoreError::Provider)?;
        getrandom::fill(&mut nonce).map_err(|_| CoreError::Provider)?;
        Ok(Self {
            attachment_id,
            mime_type,
            width,
            height,
            duration_ms,
            content_key,
            nonce,
            plaintext_size: 0,
            ciphertext_size: 0,
            ciphertext_sha256: Sha256::new(),
            chunk_index: 0,
            finished: false,
        })
    }

    /// Encrypt one plaintext chunk. The returned chunk is at most 256 KiB.
    pub fn encrypt_chunk(&mut self, plaintext: &[u8]) -> Result<Vec<u8>, CoreError> {
        if self.finished
            || plaintext.is_empty()
            || plaintext.len() > LARGE_FILE_PLAINTEXT_CHUNK_BYTES
        {
            return Err(CoreError::Protocol(protocol::ProtocolError::Invalid(
                "large file chunk",
            )));
        }
        let ciphertext = encrypt_chunk(
            &self.content_key[..],
            &self.nonce,
            &self.attachment_id,
            self.chunk_index,
            plaintext,
        )?;
        self.plaintext_size = self
            .plaintext_size
            .checked_add(plaintext.len() as u64)
            .ok_or(CoreError::Provider)?;
        self.ciphertext_size = self
            .ciphertext_size
            .checked_add(ciphertext.len() as u64)
            .ok_or(CoreError::Provider)?;
        self.ciphertext_sha256.update(&ciphertext);
        self.chunk_index = self.chunk_index.checked_add(1).ok_or(CoreError::Provider)?;
        Ok(ciphertext)
    }

    /// Encrypt a source stream to a ciphertext sink using bounded buffers.
    pub fn encrypt_reader<R: Read, W: Write>(
        &mut self,
        mut source: R,
        mut destination: W,
    ) -> Result<EncryptedLargeFile, CoreError> {
        let mut buffer = vec![0u8; LARGE_FILE_PLAINTEXT_CHUNK_BYTES];
        let mut buffered = 0;
        loop {
            let read = source
                .read(&mut buffer[buffered..])
                .map_err(|_| CoreError::Provider)?;
            if read == 0 {
                if buffered > 0 {
                    let ciphertext = self.encrypt_chunk(&buffer[..buffered])?;
                    destination
                        .write_all(&ciphertext)
                        .map_err(|_| CoreError::Provider)?;
                }
                break;
            }
            buffered += read;
            if buffered == buffer.len() {
                let ciphertext = self.encrypt_chunk(&buffer)?;
                destination
                    .write_all(&ciphertext)
                    .map_err(|_| CoreError::Provider)?;
                buffered = 0;
            }
        }
        destination.flush().map_err(|_| CoreError::Provider)?;
        self.finish()
    }

    pub fn finish(&mut self) -> Result<EncryptedLargeFile, CoreError> {
        if self.finished || self.plaintext_size == 0 {
            return Err(CoreError::Protocol(protocol::ProtocolError::Invalid(
                "large file",
            )));
        }
        self.finished = true;
        let media = chunked_media_metadata(
            &self.attachment_id,
            &self.mime_type,
            self.width,
            self.height,
            self.duration_ms,
            self.plaintext_size,
            self.ciphertext_size,
            &self.content_key[..],
            &self.nonce,
            self.ciphertext_sha256.clone().finalize().as_slice(),
        )?;
        Ok(EncryptedLargeFile { media })
    }
}

/// Stream-decrypt a chunk-encrypted attachment after validating its private
/// metadata and complete ciphertext digest.
pub fn decrypt_large_file<R: Read, W: Write>(
    media: &v1::MediaMetadata,
    mut source: R,
    mut destination: W,
) -> Result<(), CoreError> {
    validate_large_file_metadata(media)?;
    let original_size = media.original_size_bytes.unwrap();
    let chunk_count = original_size
        .checked_add(LARGE_FILE_PLAINTEXT_CHUNK_BYTES as u64 - 1)
        .and_then(|size| size.checked_div(LARGE_FILE_PLAINTEXT_CHUNK_BYTES as u64))
        .ok_or(CoreError::Authentication)?;
    let expected_ciphertext = original_size
        .checked_add(
            chunk_count
                .checked_mul(ATTACHMENT_TAG_BYTES as u64)
                .ok_or(CoreError::Authentication)?,
        )
        .ok_or(CoreError::Authentication)?;
    if expected_ciphertext != media.ciphertext_size_bytes {
        return Err(CoreError::Authentication);
    }

    let mut ciphertext_hash = Sha256::new();
    let mut ciphertext_offset = 0u64;
    let mut buffer = vec![0u8; LARGE_FILE_CIPHERTEXT_CHUNK_BYTES];
    let nonce: [u8; ATTACHMENT_NONCE_BYTES] = media
        .nonce
        .as_slice()
        .try_into()
        .map_err(|_| CoreError::Authentication)?;
    for chunk_index in 0..chunk_count {
        let chunk_start = chunk_index
            .checked_mul(LARGE_FILE_PLAINTEXT_CHUNK_BYTES as u64)
            .ok_or(CoreError::Authentication)?;
        let remaining_plaintext = original_size
            .checked_sub(chunk_start)
            .ok_or(CoreError::Authentication)?;
        let plaintext_size =
            remaining_plaintext.min(LARGE_FILE_PLAINTEXT_CHUNK_BYTES as u64) as usize;
        let ciphertext_size = plaintext_size + ATTACHMENT_TAG_BYTES;
        source
            .read_exact(&mut buffer[..ciphertext_size])
            .map_err(|_| CoreError::Authentication)?;
        ciphertext_hash.update(&buffer[..ciphertext_size]);
        let plaintext = decrypt_chunk(
            &media.content_key,
            &nonce,
            &media.attachment_id,
            chunk_index,
            &buffer[..ciphertext_size],
        )?;
        destination
            .write_all(&plaintext)
            .map_err(|_| CoreError::Provider)?;
        ciphertext_offset = ciphertext_offset
            .checked_add(ciphertext_size as u64)
            .ok_or(CoreError::Authentication)?;
    }
    if ciphertext_offset != media.ciphertext_size_bytes
        || ciphertext_hash.finalize().as_slice() != media.ciphertext_sha256.as_slice()
    {
        return Err(CoreError::Authentication);
    }
    let mut trailing = [0u8; 1];
    if source
        .read(&mut trailing)
        .map_err(|_| CoreError::Authentication)?
        != 0
    {
        return Err(CoreError::Authentication);
    }
    Ok(())
}

/// Decrypt one ciphertext chunk for a streaming receiver. The caller still
/// verifies the complete ciphertext digest before publishing the output.
pub fn decrypt_large_file_chunk(
    media: &v1::MediaMetadata,
    chunk_index: u64,
    ciphertext: &[u8],
) -> Result<Vec<u8>, CoreError> {
    validate_large_file_metadata(media)?;
    let original_size = media.original_size_bytes.unwrap();
    let chunk_count = original_size
        .checked_add(LARGE_FILE_PLAINTEXT_CHUNK_BYTES as u64 - 1)
        .and_then(|size| size.checked_div(LARGE_FILE_PLAINTEXT_CHUNK_BYTES as u64))
        .ok_or(CoreError::Authentication)?;
    if chunk_index >= chunk_count {
        return Err(CoreError::Authentication);
    }
    let chunk_start = chunk_index
        .checked_mul(LARGE_FILE_PLAINTEXT_CHUNK_BYTES as u64)
        .ok_or(CoreError::Authentication)?;
    let plaintext_size = original_size
        .checked_sub(chunk_start)
        .ok_or(CoreError::Authentication)?
        .min(LARGE_FILE_PLAINTEXT_CHUNK_BYTES as u64) as usize;
    if ciphertext.len() != plaintext_size + ATTACHMENT_TAG_BYTES {
        return Err(CoreError::Authentication);
    }
    let nonce: [u8; ATTACHMENT_NONCE_BYTES] = media
        .nonce
        .as_slice()
        .try_into()
        .map_err(|_| CoreError::Authentication)?;
    decrypt_chunk(
        &media.content_key,
        &nonce,
        &media.attachment_id,
        chunk_index,
        ciphertext,
    )
}

fn encrypt_chunk(
    key: &[u8],
    nonce: &[u8; ATTACHMENT_NONCE_BYTES],
    attachment_id: &str,
    chunk_index: u64,
    plaintext: &[u8],
) -> Result<Vec<u8>, CoreError> {
    let cipher = ChaCha20Poly1305::new(Key::from_slice(key));
    let chunk_nonce = chunk_nonce(nonce, chunk_index);
    let aad = chunk_aad(attachment_id, chunk_index);
    cipher
        .encrypt(
            Nonce::from_slice(&chunk_nonce),
            Payload {
                msg: plaintext,
                aad: &aad,
            },
        )
        .map_err(|_| CoreError::Provider)
}

fn decrypt_chunk(
    key: &[u8],
    nonce: &[u8; ATTACHMENT_NONCE_BYTES],
    attachment_id: &str,
    chunk_index: u64,
    ciphertext: &[u8],
) -> Result<Vec<u8>, CoreError> {
    let cipher = ChaCha20Poly1305::new(Key::from_slice(key));
    let chunk_nonce = chunk_nonce(nonce, chunk_index);
    let aad = chunk_aad(attachment_id, chunk_index);
    cipher
        .decrypt(
            Nonce::from_slice(&chunk_nonce),
            Payload {
                msg: ciphertext,
                aad: &aad,
            },
        )
        .map_err(|_| CoreError::Authentication)
}

fn chunk_nonce(
    base: &[u8; ATTACHMENT_NONCE_BYTES],
    chunk_index: u64,
) -> [u8; ATTACHMENT_NONCE_BYTES] {
    let mut nonce = *base;
    for (offset, byte) in chunk_index.to_be_bytes().iter().enumerate() {
        nonce[ATTACHMENT_NONCE_BYTES - 8 + offset] ^= byte;
    }
    nonce
}

fn chunk_aad(attachment_id: &str, chunk_index: u64) -> Vec<u8> {
    let mut aad =
        Vec::with_capacity(LARGE_FILE_ATTACHMENT_AAD_PREFIX.len() + attachment_id.len() + 8);
    aad.extend_from_slice(LARGE_FILE_ATTACHMENT_AAD_PREFIX);
    aad.extend_from_slice(attachment_id.as_bytes());
    aad.extend_from_slice(&chunk_index.to_be_bytes());
    aad
}

fn chunked_media_metadata(
    attachment_id: &str,
    mime_type: &str,
    width: Option<u32>,
    height: Option<u32>,
    duration_ms: Option<u64>,
    plaintext_size: u64,
    ciphertext_size: u64,
    content_key: &[u8],
    nonce: &[u8; ATTACHMENT_NONCE_BYTES],
    ciphertext_sha256: &[u8],
) -> Result<v1::MediaMetadata, CoreError> {
    if plaintext_size == 0
        || ciphertext_size == 0
        || content_key.len() != ATTACHMENT_KEY_BYTES
        || ciphertext_sha256.len() != 32
    {
        return Err(CoreError::Authentication);
    }
    let media = v1::MediaMetadata {
        attachment_id: attachment_id.to_owned(),
        mime_type: mime_type.to_owned(),
        ciphertext_size_bytes: ciphertext_size,
        content_key: content_key.to_vec(),
        nonce: nonce.to_vec(),
        ciphertext_sha256: ciphertext_sha256.to_vec(),
        width,
        height,
        duration_ms,
        blur_hash: None,
        opus: None,
        original_size_bytes: Some(plaintext_size),
        encryption_chunk_bytes: Some(LARGE_FILE_CIPHERTEXT_CHUNK_BYTES as u32),
        chunk_cids: Vec::new(),
    };
    validate_large_file_metadata(&media)?;
    Ok(media)
}

pub fn validate_large_file_metadata(media: &v1::MediaMetadata) -> Result<(), CoreError> {
    protocol::validate_media_metadata(media)?;
    if !matches!(
        media.mime_type.as_str(),
        "video/mp4" | "application/octet-stream"
    ) || media.blur_hash.is_some()
        || media.opus.is_some()
        || media.original_size_bytes.is_none()
        || media.encryption_chunk_bytes != Some(LARGE_FILE_CIPHERTEXT_CHUNK_BYTES as u32)
        || media.content_key.len() != ATTACHMENT_KEY_BYTES
        || media.nonce.len() != ATTACHMENT_NONCE_BYTES
        || media.ciphertext_sha256.len() != 32
    {
        return Err(CoreError::Authentication);
    }
    if media.mime_type == "video/mp4"
        && (media.width.is_none()
            || media.height.is_none()
            || media.duration_ms.is_none_or(|duration| duration == 0))
    {
        return Err(CoreError::Authentication);
    }
    if media.mime_type == "application/octet-stream"
        && (media.width.is_some() || media.height.is_some() || media.duration_ms.is_some())
    {
        return Err(CoreError::Authentication);
    }
    let original_size = media.original_size_bytes.unwrap();
    let chunk_count = original_size
        .checked_add(LARGE_FILE_PLAINTEXT_CHUNK_BYTES as u64 - 1)
        .and_then(|size| size.checked_div(LARGE_FILE_PLAINTEXT_CHUNK_BYTES as u64))
        .ok_or(CoreError::Authentication)?;
    let expected_ciphertext = original_size
        .checked_add(
            chunk_count
                .checked_mul(ATTACHMENT_TAG_BYTES as u64)
                .ok_or(CoreError::Authentication)?,
        )
        .ok_or(CoreError::Authentication)?;
    if expected_ciphertext != media.ciphertext_size_bytes {
        return Err(CoreError::Authentication);
    }
    Ok(())
}

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
        validate_ciphertext(&media, &ciphertext, MAX_IMAGE_BYTES + ATTACHMENT_TAG_BYTES)?;
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
    if stream.channels != profile.channels || stream.input_sample_rate_hz != profile.sample_rate_hz
    {
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
    if stream.channels != profile.channels || stream.input_sample_rate_hz != profile.sample_rate_hz
    {
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
        original_size_bytes: None,
        encryption_chunk_bytes: None,
        chunk_cids: Vec::new(),
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
    validate_ciphertext(media, ciphertext, MAX_IMAGE_BYTES + ATTACHMENT_TAG_BYTES)?;
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
