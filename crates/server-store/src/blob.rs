//! S3-compatible storage for client-encrypted attachment blobs.
//!
//! This adapter never receives plaintext media or private MediaMetadata.
//! Providers must implement conditional object creation and return object
//! metadata so retries cannot silently replace a ciphertext blob.

use crate::StoreError;
use async_trait::async_trait;
use links_protocol::validate_id;
use sha2::{Digest, Sha256};
use std::sync::Arc;

pub const MAX_BLOB_BYTES: usize = 32 * 1024 * 1024;
pub const BLOB_KEY_PREFIX: &str = "links/v1/blobs";
pub const BLOB_CONTENT_TYPE: &str = "application/octet-stream";
pub const BLOB_CACHE_CONTROL: &str = "public, max-age=2592000, immutable";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ObjectMetadata {
    pub size_bytes: u64,
    pub sha256: [u8; 32],
    pub content_type: &'static str,
    pub cache_control: &'static str,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BlobReceipt {
    pub attachment_id: [u8; 16],
    pub size_bytes: u64,
    pub sha256: [u8; 32],
}

impl BlobReceipt {
    pub fn new(
        attachment_id: &str,
        size_bytes: u64,
        sha256: [u8; 32],
    ) -> Result<Self, StoreError> {
        validate_id(attachment_id)?;
        let attachment_id = uuid::Uuid::parse_str(attachment_id)
            .map_err(|_| StoreError::Invalid)?
            .into_bytes();
        if size_bytes == 0 || size_bytes > MAX_BLOB_BYTES as u64 {
            return Err(StoreError::Invalid);
        }
        Ok(Self {
            attachment_id,
            size_bytes,
            sha256,
        })
    }

    pub fn attachment_id(&self) -> String {
        uuid::Uuid::from_bytes(self.attachment_id)
            .hyphenated()
            .to_string()
    }

    fn metadata(self) -> ObjectMetadata {
        ObjectMetadata {
            size_bytes: self.size_bytes,
            sha256: self.sha256,
            content_type: BLOB_CONTENT_TYPE,
            cache_control: BLOB_CACHE_CONTROL,
        }
    }
}

pub struct BlobUpload {
    attachment_id: String,
    ciphertext: Vec<u8>,
    receipt: BlobReceipt,
}

impl BlobUpload {
    pub fn new(attachment_id: String, ciphertext: Vec<u8>) -> Result<Self, StoreError> {
        validate_id(&attachment_id)?;
        if ciphertext.is_empty() || ciphertext.len() > MAX_BLOB_BYTES {
            return Err(StoreError::Invalid);
        }
        let size_bytes = u64::try_from(ciphertext.len()).map_err(|_| StoreError::Invalid)?;
        let sha256 = Sha256::digest(&ciphertext)
            .as_slice()
            .try_into()
            .map_err(|_| StoreError::Invalid)?;
        let receipt = BlobReceipt::new(&attachment_id, size_bytes, sha256)?;
        Ok(Self {
            attachment_id,
            ciphertext,
            receipt,
        })
    }

    pub fn attachment_id(&self) -> &str {
        &self.attachment_id
    }

    pub fn ciphertext(&self) -> &[u8] {
        &self.ciphertext
    }

    pub fn receipt(&self) -> BlobReceipt {
        self.receipt
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PutObjectResult {
    Created,
    AlreadyExists(ObjectMetadata),
}

/// Minimal S3-compatible client boundary.
///
/// put_if_absent must map to an S3 If-None-Match: * request (or an equivalent
/// conditional write). The implementation must use TLS, private credentials,
/// bounded timeouts, and never log keys, bodies, or metadata digests.
#[async_trait]
pub trait S3CompatibleObjectClient: Send + Sync {
    async fn put_if_absent(
        &self,
        key: &str,
        body: &[u8],
        metadata: ObjectMetadata,
    ) -> Result<PutObjectResult, StoreError>;

    async fn get(&self, key: &str) -> Result<Vec<u8>, StoreError>;

    async fn delete(&self, key: &str) -> Result<(), StoreError>;
}

#[async_trait]
pub trait EncryptedBlobStore: Send + Sync {
    async fn upload(&self, upload: BlobUpload) -> Result<BlobReceipt, StoreError>;

    /// Verify downloaded bytes against the private receipt from MediaMetadata.
    async fn download(&self, receipt: &BlobReceipt) -> Result<Vec<u8>, StoreError>;

    async fn delete(&self, attachment_id: &str) -> Result<(), StoreError>;
}

pub struct S3CompatibleBlobStore<C> {
    client: Arc<C>,
}

impl<C> S3CompatibleBlobStore<C>
where
    C: S3CompatibleObjectClient + 'static,
{
    pub fn new(client: Arc<C>) -> Self {
        Self { client }
    }

    pub fn object_key(attachment_id: &str) -> Result<String, StoreError> {
        validate_id(attachment_id)?;
        Ok(format!("{BLOB_KEY_PREFIX}/{attachment_id}"))
    }
}

#[async_trait]
impl<C> EncryptedBlobStore for S3CompatibleBlobStore<C>
where
    C: S3CompatibleObjectClient + 'static,
{
    async fn upload(&self, upload: BlobUpload) -> Result<BlobReceipt, StoreError> {
        let key = Self::object_key(upload.attachment_id())?;
        let receipt = upload.receipt();
        let expected = receipt.metadata();
        let result = self
            .client
            .put_if_absent(&key, upload.ciphertext(), expected)
            .await?;
        match result {
            PutObjectResult::Created => Ok(receipt),
            PutObjectResult::AlreadyExists(existing) if existing == expected => Ok(receipt),
            PutObjectResult::AlreadyExists(_) => Err(StoreError::Conflict),
        }
    }

    async fn download(&self, receipt: &BlobReceipt) -> Result<Vec<u8>, StoreError> {
        let key = Self::object_key(&receipt.attachment_id())?;
        let body = self.client.get(&key).await?;
        let actual_size = u64::try_from(body.len()).map_err(|_| StoreError::CorruptObject)?;
        let actual_sha256: [u8; 32] = Sha256::digest(&body)
            .as_slice()
            .try_into()
            .map_err(|_| StoreError::CorruptObject)?;
        if actual_size != receipt.size_bytes || actual_sha256 != receipt.sha256 {
            return Err(StoreError::CorruptObject);
        }
        Ok(body)
    }

    async fn delete(&self, attachment_id: &str) -> Result<(), StoreError> {
        let key = Self::object_key(attachment_id)?;
        self.client.delete(&key).await
    }
}
