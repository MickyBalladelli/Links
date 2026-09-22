//! Ciphertext-only mailbox contract used by the PostgreSQL adapter and future
//! ScyllaDB/DynamoDB adapters. No plaintext API exists here.
use crate::StoreError;
use async_trait::async_trait;
use links_protocol::{self, v1};

pub struct AppendRequest {
    envelope: v1::Envelope,
    accepted_at_ms: u64,
}
impl AppendRequest {
    pub fn new(envelope: v1::Envelope, now_ms: u64) -> Result<Self, StoreError> {
        links_protocol::validate_enqueue(&envelope, now_ms)?;
        Ok(Self {
            envelope,
            accepted_at_ms: now_ms,
        })
    }
    pub fn envelope(&self) -> &v1::Envelope {
        &self.envelope
    }
    pub fn accepted_at_ms(&self) -> u64 {
        self.accepted_at_ms
    }
}
#[derive(Debug, PartialEq, Eq)]
pub struct AppendResult {
    pub cursor: u64,
    pub duplicate: bool,
}

pub const MAX_PURGE_BATCH: u32 = 1_000;

pub struct ReadRequest {
    device_id: String,
    after_cursor: u64,
    limit: u32,
    now_ms: u64,
}
impl ReadRequest {
    pub fn new(
        device_id: String,
        after_cursor: u64,
        limit: u32,
        now_ms: u64,
    ) -> Result<Self, StoreError> {
        links_protocol::validate_id(&device_id)?;
        if after_cursor > links_protocol::MAX_CURSOR
            || limit == 0
            || limit > links_protocol::MAX_BATCH_ITEMS as u32
        {
            return Err(StoreError::Invalid);
        }
        Ok(Self {
            device_id,
            after_cursor,
            limit,
            now_ms,
        })
    }
    pub fn device_id(&self) -> &str {
        &self.device_id
    }
    pub fn after_cursor(&self) -> u64 {
        self.after_cursor
    }
    pub fn limit(&self) -> u32 {
        self.limit
    }
    pub fn now_ms(&self) -> u64 {
        self.now_ms
    }
}

#[async_trait]
pub trait EncryptedPayloadStore: Send + Sync {
    /// Atomic append + positive, contiguous per-device cursor allocation.
    /// Identical (device, envelope_id, content) retries return the original cursor.
    /// Different content with the same key conflicts. Retry never extends expiry.
    /// Deduplication fingerprints/tombstones survive payload deletion for 30 days
    /// from original acceptance. Fail before cursor exhaustion, never wrap/reuse.
    async fn append(&self, request: AppendRequest) -> Result<AppendResult, StoreError>;

    /// Strongly ordered replay. Expired/deleted entries become tombstones, not gaps.
    /// Cap encoded batch at MAX_FRAME_BYTES - 128 as well as the item limit.
    /// Expired cursors return CursorExpired, never silently skip or reset.
    /// Hide expired ciphertext immediately even when backend TTL GC is delayed.
    async fn read(&self, request: ReadRequest) -> Result<v1::SyncBatch, StoreError>;

    /// Idempotent cumulative delivery confirmation and purge; reject
    /// acknowledgement beyond the high watermark. Only call for the
    /// authenticated device and after its durable local decrypt/inbox commit.
    /// Delete ciphertext, retain cursor tombstones/fingerprints for the replay
    /// window. E2EE delivery/read receipts are not visible to this store.
    async fn acknowledge(
        &self,
        device_id: &str,
        through_cursor: u64,
        now_ms: u64,
    ) -> Result<(), StoreError>;

    /// Enforce the <=30 day payload lifetime. Production requires scheduled GC and
    /// backup/replication policies, not a promise that backend TTL is instantaneous.
    async fn purge_expired(&self, now_ms: u64, limit: u32) -> Result<u64, StoreError>;

    /// Remember the newest MLS welcome for one recipient conversation. A
    /// later session replays it before any ciphertext from that conversation.
    async fn put_pending_mls_bootstrap(
        &self,
        recipient_device_id: &str,
        bootstrap: &v1::MlsBootstrap,
    ) -> Result<(), StoreError> {
        let _ = (recipient_device_id, bootstrap);
        Ok(())
    }

    /// Welcomes still required before this device can decrypt its mailbox.
    async fn pending_mls_bootstraps(
        &self,
        recipient_device_id: &str,
    ) -> Result<Vec<v1::MlsBootstrap>, StoreError> {
        let _ = recipient_device_id;
        Ok(Vec::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const ID: &str = "00000000-0000-4000-8000-000000000001";
    #[test]
    fn request_boundaries() {
        assert!(ReadRequest::new(ID.into(), 0, 100, 1).is_ok());
        assert!(ReadRequest::new(ID.into(), 0, 101, 1).is_err());
        assert!(ReadRequest::new(ID.into(), 0, 0, 1).is_err());
        assert!(ReadRequest::new(ID.into(), u64::MAX, 1, 1).is_err());
        let envelope = v1::Envelope {
            protocol_version: 1,
            envelope_id: ID.into(),
            recipient_device_id: ID.into(),
            expires_at_ms: 10,
            sealed_payload: vec![1],
        };
        assert!(AppendRequest::new(envelope.clone(), 1).is_ok());
        assert!(AppendRequest::new(envelope, 10).is_err());
    }
}
