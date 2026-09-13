use crate::{
    protocol::{self, v1},
    CoreError,
};
use prost::Message;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncState {
    device_id: String,
    cursor: u64,
}
/// Opaque validated checkpoint. Persist messages/MLS changes and this checkpoint
/// in one host transaction, then commit it in memory and emit a queue ack.
#[derive(Debug)]
pub struct SyncAdvance {
    device_id: String,
    previous: u64,
    next: u64,
}
impl SyncAdvance {
    pub fn previous_cursor(&self) -> u64 {
        self.previous
    }

    pub fn next_cursor(&self) -> u64 {
        self.next
    }

    pub fn device_id(&self) -> &str {
        &self.device_id
    }
}
impl SyncState {
    pub fn restore(device_id: String, durable_cursor: u64) -> Result<Self, CoreError> {
        protocol::validate_id(&device_id)?;
        if durable_cursor > protocol::MAX_CURSOR {
            return Err(CoreError::InvalidSync);
        }
        Ok(Self {
            device_id,
            cursor: durable_cursor,
        })
    }
    pub fn cursor(&self) -> u64 {
        self.cursor
    }
    pub fn device_id(&self) -> &str {
        &self.device_id
    }
    pub fn prepare(&self, batch: &v1::SyncBatch) -> Result<SyncAdvance, CoreError> {
        if batch.recipient_device_id != self.device_id
            || batch.after_cursor != self.cursor
            || batch.items.len() > protocol::MAX_BATCH_ITEMS
            || batch.encoded_len() > protocol::MAX_FRAME_BYTES - 128
            || batch.high_watermark > protocol::MAX_CURSOR
            || batch.next_cursor > batch.high_watermark
        {
            return Err(CoreError::InvalidSync);
        }
        let mut cursor = self.cursor;
        for item in &batch.items {
            cursor = cursor.checked_add(1).ok_or(CoreError::InvalidSync)?;
            if item.cursor != cursor {
                return Err(CoreError::InvalidSync);
            }
            match &item.entry {
                Some(v1::queue_item::Entry::Envelope(e)) => {
                    protocol::validate_envelope(e)?;
                    if e.recipient_device_id != self.device_id {
                        return Err(CoreError::InvalidSync);
                    }
                }
                Some(v1::queue_item::Entry::Tombstone(t))
                    if matches!(
                        v1::tombstone::Reason::try_from(t.reason),
                        Ok(v1::tombstone::Reason::Expired | v1::tombstone::Reason::Acknowledged)
                    ) => {}
                _ => return Err(CoreError::InvalidSync),
            }
        }
        if cursor != batch.next_cursor || (batch.items.is_empty() && cursor != batch.high_watermark)
        {
            return Err(CoreError::InvalidSync);
        }
        Ok(SyncAdvance {
            device_id: self.device_id.clone(),
            previous: self.cursor,
            next: cursor,
        })
    }
    /// Host calls this only AFTER its durable transaction succeeds.
    pub fn commit(&mut self, advance: SyncAdvance) -> Result<v1::QueueAck, CoreError> {
        if advance.device_id != self.device_id || advance.previous != self.cursor {
            return Err(CoreError::InvalidSync);
        }
        self.cursor = advance.next;
        Ok(v1::QueueAck {
            through_cursor: self.cursor,
        })
    }
}

/// Decode either a plain or negotiated compressed server sync body. The
/// compressed path is bounded by the protocol layer before protobuf decode.
pub fn decode_sync_batch_body(
    body: &v1::server_frame::Body,
) -> Result<v1::SyncBatch, CoreError> {
    match body {
        v1::server_frame::Body::Batch(batch) => {
            protocol::validate_sync_batch(batch)?;
            Ok(batch.clone())
        }
        v1::server_frame::Body::CompressedBatch(compressed) => {
            Ok(protocol::decompress_sync_batch(compressed)?)
        }
        _ => Err(CoreError::InvalidSync),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn batch() -> v1::SyncBatch {
        v1::SyncBatch {
            recipient_device_id: "00000000-0000-4000-8000-000000000001".into(),
            after_cursor: 0,
            next_cursor: 1,
            high_watermark: 1,
            items: vec![v1::QueueItem {
                cursor: 1,
                entry: Some(v1::queue_item::Entry::Tombstone(v1::Tombstone {
                    reason: 1,
                })),
            }],
        }
    }
    #[test]
    fn checkpoints_only_advance_on_commit() {
        let b = batch();
        let mut state = SyncState::restore(b.recipient_device_id.clone(), 0).unwrap();
        let advance = state.prepare(&b).unwrap();
        let stale = state.prepare(&b).unwrap();
        assert_eq!(state.cursor(), 0);
        assert_eq!(state.commit(advance).unwrap().through_cursor, 1);
        assert!(state.commit(stale).is_err());
        assert!(state.prepare(&b).is_err());
    }
    #[test]
    fn rejects_gaps_wrong_devices_empty_progress_and_unknown_entries() {
        for mutation in 0..5 {
            let mut b = batch();
            let state = SyncState::restore(b.recipient_device_id.clone(), 0).unwrap();
            match mutation {
                0 => b.items[0].cursor = 2,
                1 => b.recipient_device_id = "different".into(),
                2 => b.items.clear(),
                3 => b.items[0].entry = None,
                _ => b.high_watermark = 0,
            }
            assert!(state.prepare(&b).is_err());
        }
    }
    #[test]
    fn idle_batch_is_valid() {
        let mut b = batch();
        b.items.clear();
        b.next_cursor = 0;
        b.high_watermark = 0;
        assert!(SyncState::restore(b.recipient_device_id.clone(), 0)
            .unwrap()
            .prepare(&b)
            .is_ok());
    }
}
