use crate::{v1, ProtocolError};
use prost::Message;

pub const SYNC_COMPRESSION_ZSTD_DICTIONARY_V1: i32 = 1;
pub const SYNC_COMPRESSION_DICTIONARY_ID_V1: u32 = 1;
pub const SYNC_COMPRESSION_LEVEL: i32 = 3;

/// Application-level dictionary bytes. WebSocket permessage-deflate remains
/// disabled; this dictionary is used only inside CompressedSyncBatch.
pub const SYNC_ZSTD_DICTIONARY_V1: &[u8] =
    b"links.v1 SyncBatch QueueItem Envelope Tombstone cursor recipient_device_id \
      after_cursor next_cursor high_watermark envelope_id expires_at_ms sealed_payload \
      protocol_version acknowledged expired message_id conversation_id sender_device_id \
      sequence_id sent_at_ms content text one-to-one mailbox replay cursor state sync";

/// Validate the batch before compression and after decompression. This keeps
/// cursor and envelope checks identical on both paths.
pub fn validate_sync_batch(batch: &v1::SyncBatch) -> Result<(), ProtocolError> {
    if batch.recipient_device_id.is_empty()
        || batch.items.len() > crate::MAX_BATCH_ITEMS
        || batch.encoded_len() > crate::MAX_FRAME_BYTES - 128
        || batch.high_watermark > crate::MAX_CURSOR
        || batch.next_cursor > batch.high_watermark
    {
        return Err(ProtocolError::Invalid("sync batch"));
    }
    crate::validate_id(&batch.recipient_device_id)?;

    let mut expected = batch.after_cursor;
    for item in &batch.items {
        expected = expected
            .checked_add(1)
            .ok_or(ProtocolError::Invalid("sync cursor"))?;
        if item.cursor != expected {
            return Err(ProtocolError::Invalid("sync cursor"));
        }
        match &item.entry {
            Some(v1::queue_item::Entry::Envelope(envelope)) => {
                crate::validate_envelope(envelope)?;
                if envelope.recipient_device_id != batch.recipient_device_id {
                    return Err(ProtocolError::Invalid("sync recipient"));
                }
            }
            Some(v1::queue_item::Entry::Tombstone(tombstone))
                if matches!(
                    v1::tombstone::Reason::try_from(tombstone.reason),
                    Ok(v1::tombstone::Reason::Expired | v1::tombstone::Reason::Acknowledged)
                ) => {}
            _ => return Err(ProtocolError::Invalid("sync entry")),
        }
    }
    if batch.next_cursor != expected
        || (batch.items.is_empty() && batch.next_cursor != batch.high_watermark)
    {
        return Err(ProtocolError::Invalid("sync checkpoint"));
    }
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
pub fn compress_sync_batch(
    batch: &v1::SyncBatch,
) -> Result<v1::CompressedSyncBatch, ProtocolError> {
    validate_sync_batch(batch)?;
    let bytes = batch.encode_to_vec();
    let mut compressor =
        zstd::bulk::Compressor::with_dictionary(SYNC_COMPRESSION_LEVEL, SYNC_ZSTD_DICTIONARY_V1)
            .map_err(|_| ProtocolError::Malformed)?;
    let compressed_payload = compressor
        .compress(&bytes)
        .map_err(|_| ProtocolError::Malformed)?;
    if compressed_payload.len() > crate::MAX_FRAME_BYTES - 128 {
        return Err(ProtocolError::TooLarge);
    }
    let uncompressed_size = u32::try_from(bytes.len()).map_err(|_| ProtocolError::TooLarge)?;
    let compressed = v1::CompressedSyncBatch {
        compression: SYNC_COMPRESSION_ZSTD_DICTIONARY_V1 as u32,
        dictionary_id: SYNC_COMPRESSION_DICTIONARY_ID_V1,
        uncompressed_size,
        compressed_payload,
    };
    if compressed.encoded_len() > crate::MAX_FRAME_BYTES - 128 {
        return Err(ProtocolError::TooLarge);
    }
    Ok(compressed)
}

#[cfg(not(target_arch = "wasm32"))]
pub fn decompress_sync_batch(
    compressed: &v1::CompressedSyncBatch,
) -> Result<v1::SyncBatch, ProtocolError> {
    if compressed.compression != SYNC_COMPRESSION_ZSTD_DICTIONARY_V1 as u32
        || compressed.dictionary_id != SYNC_COMPRESSION_DICTIONARY_ID_V1
        || compressed.compressed_payload.is_empty()
        || compressed.compressed_payload.len() > crate::MAX_FRAME_BYTES - 128
        || compressed.uncompressed_size == 0
        || compressed.uncompressed_size as usize > crate::MAX_FRAME_BYTES - 128
        || compressed.encoded_len() > crate::MAX_FRAME_BYTES - 128
    {
        return Err(
            if compressed.uncompressed_size as usize > crate::MAX_FRAME_BYTES - 128
                || compressed.encoded_len() > crate::MAX_FRAME_BYTES - 128
            {
                ProtocolError::TooLarge
            } else {
                ProtocolError::Malformed
            },
        );
    }
    let mut decompressor = zstd::bulk::Decompressor::with_dictionary(SYNC_ZSTD_DICTIONARY_V1)
        .map_err(|_| ProtocolError::Malformed)?;
    let bytes = decompressor
        .decompress(
            &compressed.compressed_payload,
            compressed.uncompressed_size as usize,
        )
        .map_err(|_| ProtocolError::Malformed)?;
    if bytes.len() != compressed.uncompressed_size as usize {
        return Err(ProtocolError::Malformed);
    }
    let batch = v1::SyncBatch::decode(bytes.as_slice()).map_err(|_| ProtocolError::Malformed)?;
    validate_sync_batch(&batch)?;
    Ok(batch)
}

#[cfg(target_arch = "wasm32")]
pub fn compress_sync_batch(
    _batch: &v1::SyncBatch,
) -> Result<v1::CompressedSyncBatch, ProtocolError> {
    Err(ProtocolError::Malformed)
}

#[cfg(target_arch = "wasm32")]
pub fn decompress_sync_batch(
    _compressed: &v1::CompressedSyncBatch,
) -> Result<v1::SyncBatch, ProtocolError> {
    Err(ProtocolError::Malformed)
}
