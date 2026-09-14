//! Hashcash-style proof-of-work for unverified one-to-one connection starts.
//!
//! The challenge is public and short-lived. The client searches for a nonce
//! whose domain-separated SHA-256 digest has the requested number of leading
//! zero bits. The server verifies one digest and never needs to retain the
//! client IP or a plaintext account challenge.

use sha2::{Digest, Sha256};
use thiserror::Error;

pub const VERSION: u32 = 1;
pub const CHALLENGE_BYTES: usize = 32;
pub const MIN_DIFFICULTY_BITS: u8 = 12;
pub const MAX_DIFFICULTY_BITS: u8 = 24;
pub const DEFAULT_DIFFICULTY_BITS: u8 = 18;

const DOMAIN: &[u8] = b"links/chat-request-pow/v1\0";

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ProofOfWorkError {
    #[error("invalid proof-of-work difficulty")]
    InvalidDifficulty,
    #[error("invalid proof-of-work challenge")]
    InvalidChallenge,
    #[error("invalid proof-of-work solution")]
    InvalidSolution,
    #[error("proof-of-work search exhausted")]
    Exhausted,
}

pub fn validate_difficulty(difficulty_bits: u8) -> Result<(), ProofOfWorkError> {
    if (MIN_DIFFICULTY_BITS..=MAX_DIFFICULTY_BITS).contains(&difficulty_bits) {
        Ok(())
    } else {
        Err(ProofOfWorkError::InvalidDifficulty)
    }
}

pub fn digest(challenge: &[u8; CHALLENGE_BYTES], nonce: u64) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(DOMAIN);
    hasher.update(challenge);
    hasher.update(nonce.to_be_bytes());
    hasher.finalize().into()
}

pub fn leading_zero_bits(digest: &[u8; 32]) -> u16 {
    let mut total = 0u16;
    for byte in digest {
        let zeroes = byte.leading_zeros() as u16;
        total += zeroes;
        if zeroes < 8 {
            break;
        }
    }
    total
}

pub fn verify(
    challenge: &[u8; CHALLENGE_BYTES],
    difficulty_bits: u8,
    nonce: u64,
) -> Result<(), ProofOfWorkError> {
    validate_difficulty(difficulty_bits)?;
    if leading_zero_bits(&digest(challenge, nonce)) >= u16::from(difficulty_bits) {
        Ok(())
    } else {
        Err(ProofOfWorkError::InvalidSolution)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leading_zero_bits_counts_full_and_partial_bytes() {
        assert_eq!(leading_zero_bits(&[0; 32]), 256);
        assert_eq!(leading_zero_bits(&[0x0f; 32]), 4);
        assert_eq!(leading_zero_bits(&[0x7f; 32]), 1);
        assert_eq!(leading_zero_bits(&[0x80; 32]), 0);
    }

    #[test]
    fn digest_solution_can_be_verified() {
        let challenge = [9u8; CHALLENGE_BYTES];
        let nonce = (0u64..).find(|nonce| verify(&challenge, 12, *nonce).is_ok());
        assert!(nonce.is_some());
        assert_eq!(
            verify(&challenge, 25, nonce.unwrap()),
            Err(ProofOfWorkError::InvalidDifficulty)
        );
    }

    #[test]
    fn difficulty_is_bounded() {
        assert_eq!(
            validate_difficulty(11),
            Err(ProofOfWorkError::InvalidDifficulty)
        );
        assert!(validate_difficulty(DEFAULT_DIFFICULTY_BITS).is_ok());
        assert_eq!(
            validate_difficulty(25),
            Err(ProofOfWorkError::InvalidDifficulty)
        );
    }
}
