//! Content addressing for client-encrypted attachment chunks.
//!
//! CIDs are computed over ciphertext, never plaintext. The first provider
//! profile is CIDv1 with the raw codec and SHA-256 multihash, encoded with
//! lowercase base32 so it can be used as an `ipfs://CID` URI.

use sha2::{Digest, Sha256};

pub const CONTENT_CID_VERSION: u8 = 1;
pub const CONTENT_CID_RAW_CODEC: u8 = 0x55;
pub const CONTENT_CID_SHA256_CODE: u8 = 0x12;
pub const CONTENT_CID_DIGEST_BYTES: usize = 32;
pub const CONTENT_CID_MAX_CHUNK_BYTES: usize = 256 * 1024;
pub const MAX_CONTENT_ADDRESSED_CHUNKS: usize = 1_024;
pub const IPFS_URI_PREFIX: &str = "ipfs://";

const CID_BINARY_BYTES: usize = 1 + 1 + 2 + CONTENT_CID_DIGEST_BYTES;
const CID_STRING_BYTES: usize = 1 + 58;

pub fn cid_for_bytes(bytes: &[u8]) -> Result<String, super::ProtocolError> {
    if bytes.is_empty() {
        return Err(super::ProtocolError::Invalid("content chunk"));
    }
    if bytes.len() > CONTENT_CID_MAX_CHUNK_BYTES {
        return Err(super::ProtocolError::TooLarge);
    }
    let digest = Sha256::digest(bytes);
    let mut binary = [0u8; CID_BINARY_BYTES];
    binary[0] = CONTENT_CID_VERSION;
    binary[1] = CONTENT_CID_RAW_CODEC;
    binary[2] = CONTENT_CID_SHA256_CODE;
    binary[3] = CONTENT_CID_DIGEST_BYTES as u8;
    binary[4..].copy_from_slice(&digest);
    Ok(format!("b{}", base32_encode(&binary)))
}

pub fn validate_content_cid(cid: &str) -> Result<(), super::ProtocolError> {
    if cid.len() != CID_STRING_BYTES || !cid.starts_with('b') {
        return Err(super::ProtocolError::Invalid("content cid"));
    }
    let decoded = base32_decode(&cid[1..]).ok_or(super::ProtocolError::Invalid("content cid"))?;
    if decoded.len() != CID_BINARY_BYTES
        || decoded[0] != CONTENT_CID_VERSION
        || decoded[1] != CONTENT_CID_RAW_CODEC
        || decoded[2] != CONTENT_CID_SHA256_CODE
        || decoded[3] != CONTENT_CID_DIGEST_BYTES as u8
    {
        return Err(super::ProtocolError::Invalid("content cid"));
    }
    Ok(())
}

pub fn verify_content_cid(cid: &str, bytes: &[u8]) -> Result<(), super::ProtocolError> {
    validate_content_cid(cid)?;
    if cid_for_bytes(bytes)?.as_str() != cid {
        return Err(super::ProtocolError::Invalid("content cid bytes"));
    }
    Ok(())
}

pub fn ipfs_uri_for_cid(cid: &str) -> Result<String, super::ProtocolError> {
    validate_content_cid(cid)?;
    Ok(format!("{IPFS_URI_PREFIX}{cid}"))
}

pub fn cid_from_ipfs_uri(uri: &str) -> Result<&str, super::ProtocolError> {
    let cid = uri
        .strip_prefix(IPFS_URI_PREFIX)
        .ok_or(super::ProtocolError::Invalid("ipfs uri"))?;
    if cid.contains('/') || cid.contains('?') || cid.contains('#') {
        return Err(super::ProtocolError::Invalid("ipfs uri"));
    }
    validate_content_cid(cid)?;
    Ok(cid)
}

fn base32_encode(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 32] = b"abcdefghijklmnopqrstuvwxyz234567";
    let mut output = String::with_capacity((bytes.len() * 8).div_ceil(5));
    let mut accumulator = 0u16;
    let mut bits = 0u8;
    for byte in bytes {
        accumulator = (accumulator << 8) | u16::from(*byte);
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            output.push(ALPHABET[((accumulator >> bits) & 0x1f) as usize] as char);
            accumulator &= if bits == 0 { 0 } else { (1u16 << bits) - 1 };
        }
    }
    if bits > 0 {
        output.push(ALPHABET[((accumulator << (5 - bits)) & 0x1f) as usize] as char);
    }
    output
}

fn base32_decode(value: &str) -> Option<Vec<u8>> {
    let mut output = Vec::with_capacity(value.len() * 5 / 8);
    let mut accumulator = 0u16;
    let mut bits = 0u8;
    for byte in value.bytes() {
        let digit = match byte {
            b'a'..=b'z' => byte - b'a',
            b'2'..=b'7' => byte - b'2' + 26,
            _ => return None,
        };
        accumulator = (accumulator << 5) | u16::from(digit);
        bits += 5;
        while bits >= 8 {
            bits -= 8;
            output.push(((accumulator >> bits) & 0xff) as u8);
            accumulator &= if bits == 0 { 0 } else { (1u16 << bits) - 1 };
        }
    }
    if bits > 0 && (accumulator & ((1 << bits) - 1)) != 0 {
        return None;
    }
    Some(output)
}
