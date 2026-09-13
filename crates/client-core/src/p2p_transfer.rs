//! Authenticated, resumable file transfer over an already-authenticated
//! WebRTC DataChannel.
//!
//! This protocol carries ciphertext bytes only. MLS/signaling delivers the
//! attachment metadata and channel binding; the DataChannel never receives a
//! content key and no relay needs to inspect file contents.

use sha2::{Digest, Sha256};
use std::cmp::min;
use thiserror::Error;
use uuid::Uuid;

pub const P2P_TRANSFER_VERSION: u8 = 1;
pub const P2P_TRANSFER_CHUNK_BYTES: usize = 256 * 1024;
pub const P2P_TRANSFER_MAX_FRAME_BYTES: usize = P2P_TRANSFER_CHUNK_BYTES + 66;
pub const P2P_TRANSFER_MAGIC: &[u8; 4] = b"LDT1";

const FRAME_HEADER_BYTES: usize = 6;
const UUID_BYTES: usize = 16;
const DIGEST_BYTES: usize = 32;
const BEGIN_BYTES: usize = FRAME_HEADER_BYTES + UUID_BYTES + UUID_BYTES + 8 + 4 + DIGEST_BYTES;
const OFFSET_BYTES: usize = FRAME_HEADER_BYTES + UUID_BYTES + 8;
const CHUNK_HEADER_BYTES: usize = FRAME_HEADER_BYTES + UUID_BYTES + 8 + 4 + DIGEST_BYTES;
const FINISH_BYTES: usize = FRAME_HEADER_BYTES + UUID_BYTES + DIGEST_BYTES;
const COMPLETE_BYTES: usize = FRAME_HEADER_BYTES + UUID_BYTES + 1 + DIGEST_BYTES;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum P2pTransferError {
    #[error("invalid P2P transfer manifest")]
    InvalidManifest,
    #[error("invalid P2P transfer frame")]
    InvalidFrame,
    #[error("unexpected P2P transfer frame")]
    UnexpectedFrame,
    #[error("P2P transfer chunk is out of order")]
    OutOfOrder,
    #[error("P2P transfer integrity check failed")]
    IntegrityFailure,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct P2pTransferManifest {
    pub transfer_id: Uuid,
    pub attachment_id: Uuid,
    pub total_bytes: u64,
    pub chunk_bytes: u32,
    pub ciphertext_sha256: [u8; DIGEST_BYTES],
}

impl P2pTransferManifest {
    pub fn new(
        transfer_id: Uuid,
        attachment_id: Uuid,
        total_bytes: u64,
        ciphertext_sha256: [u8; DIGEST_BYTES],
    ) -> Result<Self, P2pTransferError> {
        let manifest = Self {
            transfer_id,
            attachment_id,
            total_bytes,
            chunk_bytes: P2P_TRANSFER_CHUNK_BYTES as u32,
            ciphertext_sha256,
        };
        manifest.validate()?;
        Ok(manifest)
    }

    pub fn validate(&self) -> Result<(), P2pTransferError> {
        if self.transfer_id.is_nil()
            || self.attachment_id.is_nil()
            || self.total_bytes == 0
            || self.chunk_bytes == 0
            || self.chunk_bytes as usize > P2P_TRANSFER_CHUNK_BYTES
        {
            return Err(P2pTransferError::InvalidManifest);
        }
        Ok(())
    }

    fn validate_offset(&self, offset: u64) -> Result<(), P2pTransferError> {
        if offset > self.total_bytes
            || (offset != self.total_bytes && offset % self.chunk_bytes as u64 != 0)
        {
            return Err(P2pTransferError::InvalidFrame);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum P2pCompletion {
    Complete,
    IntegrityFailed,
    Rejected,
}

impl P2pCompletion {
    fn wire_value(self) -> u8 {
        match self {
            Self::Complete => 1,
            Self::IntegrityFailed => 2,
            Self::Rejected => 3,
        }
    }

    fn from_wire(value: u8) -> Result<Self, P2pTransferError> {
        match value {
            1 => Ok(Self::Complete),
            2 => Ok(Self::IntegrityFailed),
            3 => Ok(Self::Rejected),
            _ => Err(P2pTransferError::InvalidFrame),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum P2pTransferFrame {
    Begin(P2pTransferManifest),
    Resume {
        transfer_id: Uuid,
        next_offset: u64,
    },
    Chunk {
        transfer_id: Uuid,
        offset: u64,
        bytes: Vec<u8>,
        sha256: [u8; DIGEST_BYTES],
    },
    Finish {
        transfer_id: Uuid,
        ciphertext_sha256: [u8; DIGEST_BYTES],
    },
    Ack {
        transfer_id: Uuid,
        next_offset: u64,
    },
    Complete {
        transfer_id: Uuid,
        status: P2pCompletion,
        ciphertext_sha256: [u8; DIGEST_BYTES],
    },
}

impl P2pTransferFrame {
    pub fn encode(&self) -> Result<Vec<u8>, P2pTransferError> {
        let mut output = Vec::new();
        output.extend_from_slice(P2P_TRANSFER_MAGIC);
        output.push(P2P_TRANSFER_VERSION);
        output.push(self.kind());
        match self {
            Self::Begin(manifest) => {
                manifest.validate()?;
                output.extend_from_slice(manifest.transfer_id.as_bytes());
                output.extend_from_slice(manifest.attachment_id.as_bytes());
                output.extend_from_slice(&manifest.total_bytes.to_be_bytes());
                output.extend_from_slice(&manifest.chunk_bytes.to_be_bytes());
                output.extend_from_slice(&manifest.ciphertext_sha256);
            }
            Self::Resume {
                transfer_id,
                next_offset,
            }
            | Self::Ack {
                transfer_id,
                next_offset,
            } => {
                if transfer_id.is_nil() {
                    return Err(P2pTransferError::InvalidFrame);
                }
                output.extend_from_slice(transfer_id.as_bytes());
                output.extend_from_slice(&next_offset.to_be_bytes());
            }
            Self::Chunk {
                transfer_id,
                offset,
                bytes,
                sha256,
            } => {
                if transfer_id.is_nil()
                    || bytes.is_empty()
                    || bytes.len() > P2P_TRANSFER_CHUNK_BYTES
                    || bytes.len() > u32::MAX as usize
                    || *offset > u64::MAX - bytes.len() as u64
                {
                    return Err(P2pTransferError::InvalidFrame);
                }
                if Sha256::digest(bytes).as_slice() != sha256 {
                    return Err(P2pTransferError::IntegrityFailure);
                }
                output.extend_from_slice(transfer_id.as_bytes());
                output.extend_from_slice(&offset.to_be_bytes());
                output.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
                output.extend_from_slice(sha256);
                output.extend_from_slice(bytes);
            }
            Self::Finish {
                transfer_id,
                ciphertext_sha256,
            } => {
                if transfer_id.is_nil() {
                    return Err(P2pTransferError::InvalidFrame);
                }
                output.extend_from_slice(transfer_id.as_bytes());
                output.extend_from_slice(ciphertext_sha256);
            }
            Self::Complete {
                transfer_id,
                status,
                ciphertext_sha256,
            } => {
                if transfer_id.is_nil() {
                    return Err(P2pTransferError::InvalidFrame);
                }
                output.extend_from_slice(transfer_id.as_bytes());
                output.push(status.wire_value());
                output.extend_from_slice(ciphertext_sha256);
            }
        }
        if output.len() > P2P_TRANSFER_MAX_FRAME_BYTES {
            return Err(P2pTransferError::InvalidFrame);
        }
        Ok(output)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, P2pTransferError> {
        if bytes.len() > P2P_TRANSFER_MAX_FRAME_BYTES || bytes.len() < FRAME_HEADER_BYTES {
            return Err(P2pTransferError::InvalidFrame);
        }
        if &bytes[..4] != P2P_TRANSFER_MAGIC || bytes[4] != P2P_TRANSFER_VERSION {
            return Err(P2pTransferError::InvalidFrame);
        }
        let mut cursor = FrameCursor::new(&bytes[FRAME_HEADER_BYTES..]);
        let frame = match bytes[5] {
            1 => {
                if bytes.len() != BEGIN_BYTES {
                    return Err(P2pTransferError::InvalidFrame);
                }
                let manifest = P2pTransferManifest {
                    transfer_id: uuid(&cursor.read_array()?),
                    attachment_id: uuid(&cursor.read_array()?),
                    total_bytes: cursor.read_u64()?,
                    chunk_bytes: cursor.read_u32()?,
                    ciphertext_sha256: cursor.read_array()?,
                };
                manifest.validate()?;
                Self::Begin(manifest)
            }
            2 | 5 => {
                if bytes.len() != OFFSET_BYTES {
                    return Err(P2pTransferError::InvalidFrame);
                }
                let transfer_id = uuid(&cursor.read_array()?);
                let next_offset = cursor.read_u64()?;
                if transfer_id.is_nil() {
                    return Err(P2pTransferError::InvalidFrame);
                }
                if bytes[5] == 2 {
                    Self::Resume {
                        transfer_id,
                        next_offset,
                    }
                } else {
                    Self::Ack {
                        transfer_id,
                        next_offset,
                    }
                }
            }
            3 => {
                if bytes.len() < CHUNK_HEADER_BYTES {
                    return Err(P2pTransferError::InvalidFrame);
                }
                let transfer_id = uuid(&cursor.read_array()?);
                let offset = cursor.read_u64()?;
                let length = cursor.read_u32()? as usize;
                let sha256 = cursor.read_array()?;
                if transfer_id.is_nil()
                    || length == 0
                    || length > P2P_TRANSFER_CHUNK_BYTES
                    || cursor.remaining() != length
                {
                    return Err(P2pTransferError::InvalidFrame);
                }
                let payload = cursor.read(length)?.to_vec();
                if Sha256::digest(&payload).as_slice() != sha256 {
                    return Err(P2pTransferError::IntegrityFailure);
                }
                Self::Chunk {
                    transfer_id,
                    offset,
                    bytes: payload,
                    sha256,
                }
            }
            4 => {
                if bytes.len() != FINISH_BYTES {
                    return Err(P2pTransferError::InvalidFrame);
                }
                let transfer_id = uuid(&cursor.read_array()?);
                if transfer_id.is_nil() {
                    return Err(P2pTransferError::InvalidFrame);
                }
                Self::Finish {
                    transfer_id,
                    ciphertext_sha256: cursor.read_array()?,
                }
            }
            6 => {
                if bytes.len() != COMPLETE_BYTES {
                    return Err(P2pTransferError::InvalidFrame);
                }
                let transfer_id = uuid(&cursor.read_array()?);
                if transfer_id.is_nil() {
                    return Err(P2pTransferError::InvalidFrame);
                }
                Self::Complete {
                    transfer_id,
                    status: P2pCompletion::from_wire(cursor.read_u8()?)?,
                    ciphertext_sha256: cursor.read_array()?,
                }
            }
            _ => return Err(P2pTransferError::InvalidFrame),
        };
        if cursor.remaining() != 0 {
            return Err(P2pTransferError::InvalidFrame);
        }
        Ok(frame)
    }

    fn kind(&self) -> u8 {
        match self {
            Self::Begin(_) => 1,
            Self::Resume { .. } => 2,
            Self::Chunk { .. } => 3,
            Self::Finish { .. } => 4,
            Self::Ack { .. } => 5,
            Self::Complete { .. } => 6,
        }
    }
}

/// Sender state. The host reads ciphertext from its file and passes one exact
/// next chunk to `next_chunk`; this avoids loading an uncapped transfer in RAM.
pub struct P2pTransferSender {
    manifest: P2pTransferManifest,
    next_offset: u64,
}

impl P2pTransferSender {
    pub fn new(manifest: P2pTransferManifest) -> Result<Self, P2pTransferError> {
        manifest.validate()?;
        Ok(Self {
            manifest,
            next_offset: 0,
        })
    }

    pub fn manifest(&self) -> &P2pTransferManifest {
        &self.manifest
    }

    pub fn begin(&self) -> P2pTransferFrame {
        P2pTransferFrame::Begin(self.manifest.clone())
    }

    pub fn accept_resume(&mut self, next_offset: u64) -> Result<(), P2pTransferError> {
        self.manifest.validate_offset(next_offset)?;
        self.next_offset = next_offset;
        Ok(())
    }

    pub fn next_offset(&self) -> u64 {
        self.next_offset
    }

    pub fn next_chunk(&mut self, bytes: Vec<u8>) -> Result<P2pTransferFrame, P2pTransferError> {
        let expected = min(
            self.manifest.chunk_bytes as u64,
            self.manifest.total_bytes - self.next_offset,
        ) as usize;
        if bytes.len() != expected || expected == 0 {
            return Err(P2pTransferError::InvalidFrame);
        }
        let sha256 = Sha256::digest(&bytes)
            .as_slice()
            .try_into()
            .map_err(|_| P2pTransferError::InvalidFrame)?;
        let frame = P2pTransferFrame::Chunk {
            transfer_id: self.manifest.transfer_id,
            offset: self.next_offset,
            bytes,
            sha256,
        };
        self.next_offset += expected as u64;
        Ok(frame)
    }

    pub fn finish(&self) -> Result<P2pTransferFrame, P2pTransferError> {
        if self.next_offset != self.manifest.total_bytes {
            return Err(P2pTransferError::UnexpectedFrame);
        }
        Ok(P2pTransferFrame::Finish {
            transfer_id: self.manifest.transfer_id,
            ciphertext_sha256: self.manifest.ciphertext_sha256,
        })
    }

    pub fn accept_complete(
        &self,
        frame: &P2pTransferFrame,
    ) -> Result<P2pCompletion, P2pTransferError> {
        let P2pTransferFrame::Complete {
            transfer_id,
            status,
            ciphertext_sha256,
        } = frame
        else {
            return Err(P2pTransferError::UnexpectedFrame);
        };
        if *transfer_id != self.manifest.transfer_id
            || *ciphertext_sha256 != self.manifest.ciphertext_sha256
        {
            return Err(P2pTransferError::IntegrityFailure);
        }
        Ok(*status)
    }
}

/// Receiver state. Callers persist each validated chunk before committing it
/// so a reconnect can resume from `next_offset` without acknowledging data
/// that is only in memory.
pub struct P2pTransferReceiver {
    manifest: P2pTransferManifest,
    next_offset: u64,
    hasher: Sha256,
}

impl P2pTransferReceiver {
    pub fn new(manifest: P2pTransferManifest) -> Result<Self, P2pTransferError> {
        manifest.validate()?;
        Ok(Self {
            manifest,
            next_offset: 0,
            hasher: Sha256::new(),
        })
    }

    pub fn manifest(&self) -> &P2pTransferManifest {
        &self.manifest
    }

    pub fn resume(&self) -> P2pTransferFrame {
        P2pTransferFrame::Resume {
            transfer_id: self.manifest.transfer_id,
            next_offset: self.next_offset,
        }
    }

    pub fn next_offset(&self) -> u64 {
        self.next_offset
    }

    pub fn validate_chunk<'a>(
        &self,
        frame: &'a P2pTransferFrame,
    ) -> Result<&'a [u8], P2pTransferError> {
        let P2pTransferFrame::Chunk {
            transfer_id,
            offset,
            bytes,
            sha256,
        } = frame
        else {
            return Err(P2pTransferError::UnexpectedFrame);
        };
        let expected = min(
            self.manifest.chunk_bytes as u64,
            self.manifest.total_bytes - self.next_offset,
        );
        if *transfer_id != self.manifest.transfer_id
            || *offset != self.next_offset
            || bytes.len() as u64 != expected
            || Sha256::digest(bytes).as_slice() != sha256
        {
            return Err(if *offset != self.next_offset {
                P2pTransferError::OutOfOrder
            } else {
                P2pTransferError::IntegrityFailure
            });
        }
        Ok(bytes)
    }

    /// Commit only after the host has durably written the bytes to its
    /// temporary ciphertext file.
    pub fn commit_chunk(
        &mut self,
        frame: &P2pTransferFrame,
    ) -> Result<P2pTransferFrame, P2pTransferError> {
        let bytes = self.validate_chunk(frame)?;
        self.hasher.update(bytes);
        self.next_offset += bytes.len() as u64;
        Ok(P2pTransferFrame::Ack {
            transfer_id: self.manifest.transfer_id,
            next_offset: self.next_offset,
        })
    }

    /// Rebuild the hash state from a persisted prefix after process restart.
    /// Call repeatedly with sequential chunks read from the temporary file.
    pub fn restore_prefix(&mut self, prefix: &[u8]) -> Result<(), P2pTransferError> {
        let expected = min(
            self.manifest.chunk_bytes as u64,
            self.manifest.total_bytes - self.next_offset,
        ) as usize;
        if prefix.len() != expected || expected == 0 {
            return Err(P2pTransferError::InvalidFrame);
        }
        self.hasher.update(prefix);
        self.next_offset += prefix.len() as u64;
        Ok(())
    }

    pub fn complete(&self, frame: &P2pTransferFrame) -> Result<P2pTransferFrame, P2pTransferError> {
        let P2pTransferFrame::Finish {
            transfer_id,
            ciphertext_sha256,
        } = frame
        else {
            return Err(P2pTransferError::UnexpectedFrame);
        };
        let actual: [u8; DIGEST_BYTES] = self.hasher.clone().finalize().into();
        let status = if *transfer_id != self.manifest.transfer_id
            || *ciphertext_sha256 != self.manifest.ciphertext_sha256
            || self.next_offset != self.manifest.total_bytes
            || actual != self.manifest.ciphertext_sha256
        {
            P2pCompletion::IntegrityFailed
        } else {
            P2pCompletion::Complete
        };
        Ok(P2pTransferFrame::Complete {
            transfer_id: self.manifest.transfer_id,
            status,
            ciphertext_sha256: self.manifest.ciphertext_sha256,
        })
    }
}

struct FrameCursor<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> FrameCursor<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn remaining(&self) -> usize {
        self.bytes.len() - self.offset
    }

    fn read(&mut self, length: usize) -> Result<&'a [u8], P2pTransferError> {
        let end = self
            .offset
            .checked_add(length)
            .ok_or(P2pTransferError::InvalidFrame)?;
        if end > self.bytes.len() {
            return Err(P2pTransferError::InvalidFrame);
        }
        let bytes = &self.bytes[self.offset..end];
        self.offset = end;
        Ok(bytes)
    }

    fn read_array<const N: usize>(&mut self) -> Result<[u8; N], P2pTransferError> {
        self.read(N)?
            .try_into()
            .map_err(|_| P2pTransferError::InvalidFrame)
    }

    fn read_u8(&mut self) -> Result<u8, P2pTransferError> {
        Ok(self.read_array::<1>()?[0])
    }

    fn read_u32(&mut self) -> Result<u32, P2pTransferError> {
        Ok(u32::from_be_bytes(self.read_array()?))
    }

    fn read_u64(&mut self) -> Result<u64, P2pTransferError> {
        Ok(u64::from_be_bytes(self.read_array()?))
    }
}

fn uuid(bytes: &[u8; UUID_BYTES]) -> Uuid {
    Uuid::from_bytes(*bytes)
}
