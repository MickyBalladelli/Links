//! Provider-neutral storage boundary for client-encrypted content-addressed
//! chunks. IPFS, Arweave, and Filecoin adapters implement the same block API.

use crate::StoreError;
use async_trait::async_trait;
use links_protocol::content_addressed::{
    validate_content_cid, verify_content_cid, CONTENT_CID_MAX_CHUNK_BYTES,
};
use sha2::{Digest, Sha256};
use std::sync::Arc;

pub const CONTENT_ADDRESSED_CONTENT_TYPE: &str = "application/octet-stream";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ContentAddressedMetadata {
    pub size_bytes: u64,
    pub sha256: [u8; 32],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContentAddressedProvider {
    Ipfs,
    Arweave,
    Filecoin,
}

/// Minimal block API for a provider adapter. The adapter maps its native
/// object/transaction/deal identifiers back to the CID supplied here.
#[async_trait]
pub trait ContentAddressedObjectClient: Send + Sync {
    async fn put_if_absent(
        &self,
        cid: &str,
        body: &[u8],
        metadata: ContentAddressedMetadata,
    ) -> Result<(), StoreError>;

    async fn get(&self, cid: &str) -> Result<Vec<u8>, StoreError>;
}

#[async_trait]
pub trait EncryptedChunkStore: Send + Sync {
    async fn upload(&self, cid: &str, ciphertext: &[u8]) -> Result<(), StoreError>;

    async fn download(&self, cid: &str) -> Result<Vec<u8>, StoreError>;
}

pub struct ContentAddressedStore<C> {
    client: Arc<C>,
}

impl<C> ContentAddressedStore<C>
where
    C: ContentAddressedObjectClient + 'static,
{
    pub fn new(client: Arc<C>) -> Self {
        Self { client }
    }

    pub fn metadata(cid: &str, ciphertext: &[u8]) -> Result<ContentAddressedMetadata, StoreError> {
        validate_chunk(cid, ciphertext)?;
        let sha256 = Sha256::digest(ciphertext)
            .as_slice()
            .try_into()
            .map_err(|_| StoreError::CorruptObject)?;
        Ok(ContentAddressedMetadata {
            size_bytes: ciphertext.len() as u64,
            sha256,
        })
    }

    pub fn validate_cid(cid: &str) -> Result<(), StoreError> {
        validate_content_cid(cid).map_err(StoreError::from)
    }
}

#[async_trait]
impl<C> EncryptedChunkStore for ContentAddressedStore<C>
where
    C: ContentAddressedObjectClient + 'static,
{
    async fn upload(&self, cid: &str, ciphertext: &[u8]) -> Result<(), StoreError> {
        let metadata = Self::metadata(cid, ciphertext)?;
        self.client.put_if_absent(cid, ciphertext, metadata).await
    }

    async fn download(&self, cid: &str) -> Result<Vec<u8>, StoreError> {
        validate_content_cid(cid).map_err(StoreError::from)?;
        let ciphertext = self.client.get(cid).await?;
        verify_content_cid(cid, &ciphertext).map_err(|_| StoreError::CorruptObject)?;
        Ok(ciphertext)
    }
}

fn validate_chunk(cid: &str, ciphertext: &[u8]) -> Result<(), StoreError> {
    validate_content_cid(cid).map_err(StoreError::from)?;
    if ciphertext.is_empty() || ciphertext.len() > CONTENT_CID_MAX_CHUNK_BYTES {
        return Err(StoreError::Invalid);
    }
    verify_content_cid(cid, ciphertext).map_err(|_| StoreError::CorruptObject)
}
