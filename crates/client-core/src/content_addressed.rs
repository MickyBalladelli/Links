//! Client-side encrypted content-addressed attachment chunks.
//!
//! The existing large-file encryptor produces independently authenticated
//! ciphertext chunks. This module binds each chunk to a CID after encryption;
//! providers only receive those ciphertext bytes and their content address.

use crate::{attachments::LargeFileEncryptor, attachments::LARGE_FILE_CIPHERTEXT_CHUNK_BYTES, protocol::v1, CoreError};
use sha2::{Digest, Sha256};
use std::io::Write;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContentAddressedChunkReference {
    pub index: u64,
    pub cid: String,
    pub ipfs_uri: String,
    pub ciphertext_size_bytes: u32,
}

pub struct ContentAddressedChunk {
    pub reference: ContentAddressedChunkReference,
    pub ciphertext: Vec<u8>,
}

pub struct EncryptedContentAddressedLargeFile {
    pub media: v1::MediaMetadata,
    pub chunks: Vec<ContentAddressedChunkReference>,
}

/// Streaming wrapper around `LargeFileEncryptor`. Upload each returned chunk
/// immediately to IPFS, Arweave, or Filecoin-backed storage; only references
/// remain in the final private media metadata.
pub struct ContentAddressedLargeFileEncryptor {
    inner: LargeFileEncryptor,
    references: Vec<ContentAddressedChunkReference>,
}

impl ContentAddressedLargeFileEncryptor {
    pub fn new(
        attachment_id: String,
        mime_type: String,
        width: Option<u32>,
        height: Option<u32>,
        duration_ms: Option<u64>,
    ) -> Result<Self, CoreError> {
        Ok(Self {
            inner: LargeFileEncryptor::new(
                attachment_id,
                mime_type,
                width,
                height,
                duration_ms,
            )?,
            references: Vec::new(),
        })
    }

    pub fn encrypt_chunk(&mut self, plaintext: &[u8]) -> Result<ContentAddressedChunk, CoreError> {
        let index = self.references.len() as u64;
        let ciphertext = self.inner.encrypt_chunk(plaintext)?;
        let cid = crate::protocol::content_addressed::cid_for_bytes(&ciphertext)?;
        let ipfs_uri = crate::protocol::content_addressed::ipfs_uri_for_cid(&cid)?;
        let ciphertext_size_bytes =
            u32::try_from(ciphertext.len()).map_err(|_| CoreError::Provider)?;
        let reference = ContentAddressedChunkReference {
            index,
            cid,
            ipfs_uri,
            ciphertext_size_bytes,
        };
        self.references.push(reference.clone());
        Ok(ContentAddressedChunk {
            reference,
            ciphertext,
        })
    }

    pub fn finish(mut self) -> Result<EncryptedContentAddressedLargeFile, CoreError> {
        let mut encrypted = self.inner.finish()?;
        if self.references.is_empty()
            || self.references.len()
                > crate::protocol::content_addressed::MAX_CONTENT_ADDRESSED_CHUNKS
        {
            return Err(CoreError::Protocol(crate::protocol::ProtocolError::Invalid(
                "content-addressed chunks",
            )));
        }
        encrypted.media.chunk_cids = self
            .references
            .iter()
            .map(|reference| reference.cid.clone())
            .collect();
        validate_content_addressed_media(&encrypted.media)?;
        Ok(EncryptedContentAddressedLargeFile {
            media: encrypted.media,
            chunks: self.references,
        })
    }
}

pub fn validate_content_addressed_media(
    media: &v1::MediaMetadata,
) -> Result<&[String], CoreError> {
    crate::attachments::validate_large_file_metadata(media)?;
    if media.chunk_cids.is_empty() {
        return Err(CoreError::Protocol(crate::protocol::ProtocolError::Invalid(
            "content-addressed media",
        )));
    }
    for cid in &media.chunk_cids {
        crate::protocol::content_addressed::validate_content_cid(cid)?;
    }
    Ok(&media.chunk_cids)
}

pub fn content_addressed_chunk_uri(
    media: &v1::MediaMetadata,
    chunk_index: usize,
) -> Result<String, CoreError> {
    let cids = validate_content_addressed_media(media)?;
    let cid = cids.get(chunk_index).ok_or(CoreError::Authentication)?;
    Ok(crate::protocol::content_addressed::ipfs_uri_for_cid(cid)?)
}

/// Verify every downloaded ciphertext block and the complete ciphertext digest
/// before releasing any plaintext to the destination writer.
pub fn decrypt_content_addressed_chunks<W: Write>(
    media: &v1::MediaMetadata,
    chunks: &[Vec<u8>],
    mut destination: W,
) -> Result<(), CoreError> {
    let cids = validate_content_addressed_media(media)?;
    if chunks.len() != cids.len() {
        return Err(CoreError::Authentication);
    }
    let mut ciphertext_hash = Sha256::new();
    for (chunk_index, (cid, ciphertext)) in cids.iter().zip(chunks).enumerate() {
        let expected_size = expected_ciphertext_chunk_size(media, chunk_index)?;
        if ciphertext.len() != expected_size {
            return Err(CoreError::Authentication);
        }
        crate::protocol::content_addressed::verify_content_cid(cid, ciphertext)?;
        ciphertext_hash.update(ciphertext);
    }
    if ciphertext_hash.finalize().as_slice() != media.ciphertext_sha256.as_slice() {
        return Err(CoreError::Authentication);
    }
    for (chunk_index, ciphertext) in chunks.iter().enumerate() {
        let plaintext = crate::attachments::decrypt_large_file_chunk(
            media,
            chunk_index as u64,
            ciphertext,
        )?;
        destination
            .write_all(&plaintext)
            .map_err(|_| CoreError::Provider)?;
    }
    Ok(())
}

fn expected_ciphertext_chunk_size(
    media: &v1::MediaMetadata,
    chunk_index: usize,
) -> Result<usize, CoreError> {
    let original_size = media.original_size_bytes.ok_or(CoreError::Authentication)?;
    let chunk_bytes = media
        .encryption_chunk_bytes
        .map(|bytes| bytes as usize)
        .ok_or(CoreError::Authentication)?;
    if chunk_bytes != LARGE_FILE_CIPHERTEXT_CHUNK_BYTES {
        return Err(CoreError::Authentication);
    }
    let plaintext_chunk_bytes = chunk_bytes
        .checked_sub(crate::attachments::ATTACHMENT_TAG_BYTES)
        .ok_or(CoreError::Authentication)?;
    let offset = (chunk_index as u64)
        .checked_mul(plaintext_chunk_bytes as u64)
        .ok_or(CoreError::Authentication)?;
    let remaining = original_size
        .checked_sub(offset)
        .ok_or(CoreError::Authentication)?;
    Ok(remaining.min(plaintext_chunk_bytes as u64) as usize
        + crate::attachments::ATTACHMENT_TAG_BYTES)
}
