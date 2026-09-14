//! Length-prefix framing for the gateway's WebTransport HTTP/3 adapter.
//!
//! WebTransport bidirectional streams are byte streams, while the Links
//! gateway API consumes complete protobuf frames. This codec keeps the stream
//! adapter transport-neutral and reuses the normal gateway validation after a
//! frame is assembled.

use super::GatewayError;
use links_protocol as protocol;

pub const FRAME_HEADER_BYTES: usize = 4;

pub struct FrameDecoder {
    buffer: Vec<u8>,
    expected_frame_bytes: Option<usize>,
}

impl FrameDecoder {
    pub fn new() -> Self {
        Self {
            buffer: Vec::new(),
            expected_frame_bytes: None,
        }
    }

    /// Feed one arbitrary stream chunk and return every complete protobuf
    /// frame it contains. Partial headers and bodies stay buffered.
    pub fn push(&mut self, chunk: &[u8]) -> Result<Vec<Vec<u8>>, GatewayError> {
        self.buffer.extend_from_slice(chunk);
        let mut frames = Vec::new();
        let mut offset = 0;

        loop {
            if self.expected_frame_bytes.is_none() {
                if self.buffer.len().saturating_sub(offset) < FRAME_HEADER_BYTES {
                    break;
                }
                let length = u32::from_be_bytes(
                    self.buffer[offset..offset + FRAME_HEADER_BYTES]
                        .try_into()
                        .map_err(|_| GatewayError::Invalid)?,
                ) as usize;
                if length == 0 || length > protocol::MAX_FRAME_BYTES {
                    return Err(GatewayError::Invalid);
                }
                self.expected_frame_bytes = Some(length);
                offset += FRAME_HEADER_BYTES;
            }

            let expected = self.expected_frame_bytes.ok_or(GatewayError::Invalid)?;
            if self.buffer.len().saturating_sub(offset) < expected {
                break;
            }
            frames.push(self.buffer[offset..offset + expected].to_vec());
            offset += expected;
            self.expected_frame_bytes = None;
        }

        if offset != 0 {
            self.buffer = self.buffer[offset..].to_vec();
        }
        if self.buffer.len() > protocol::MAX_FRAME_BYTES + FRAME_HEADER_BYTES {
            return Err(GatewayError::Invalid);
        }
        Ok(frames)
    }

    pub fn reset(&mut self) {
        self.buffer.clear();
        self.expected_frame_bytes = None;
    }
}

impl Default for FrameDecoder {
    fn default() -> Self {
        Self::new()
    }
}

pub fn encode_frame(frame: &[u8]) -> Result<Vec<u8>, GatewayError> {
    if frame.is_empty() || frame.len() > protocol::MAX_FRAME_BYTES {
        return Err(GatewayError::Invalid);
    }
    let length = u32::try_from(frame.len()).map_err(|_| GatewayError::Invalid)?;
    let mut encoded = Vec::with_capacity(FRAME_HEADER_BYTES + frame.len());
    encoded.extend_from_slice(&length.to_be_bytes());
    encoded.extend_from_slice(frame);
    Ok(encoded)
}
