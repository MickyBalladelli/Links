//! Verifiable OPRF primitives for one-sided private contact discovery.
//!
//! The client sends blinded Ristretto points. The server proves, with a
//! DLEQ proof, that it evaluated every point with the configured directory
//! key. The client unblinds the result and compares opaque OPRF tokens locally.
//! This protects the client's contact values from the server; it is not a
//! claim that TLS, account identity, query volume, or timing metadata vanish.

use curve25519_dalek::{
    constants::RISTRETTO_BASEPOINT_POINT,
    ristretto::{CompressedRistretto, RistrettoPoint},
    scalar::Scalar,
    traits::IsIdentity,
};
use sha2::{Digest, Sha256, Sha512};
use thiserror::Error;

pub const VERSION: u32 = 1;
pub const POINT_BYTES: usize = 32;
pub const PROOF_BYTES: usize = 64;
pub const TOKEN_BYTES: usize = 32;
pub const MAX_QUERY_ITEMS: usize = 256;
pub const MAX_DIRECTORY_TOKENS: usize = 1_000_000;
pub const FILTER_HASH_COUNT: u8 = 44;
pub const FILTER_MIN_BITS: usize = 8_192;
pub const FILTER_BITS_PER_TOKEN: usize = 64;
pub const FILTER_MAX_BYTES: usize = 16 * 1024 * 1024;

const INPUT_DOMAIN: &[u8] = b"links/contact-psi/input/v1\0";
const HASH_TO_GROUP_DOMAIN: &[u8] = b"links/contact-psi/hash-to-group/v1\0";
const OUTPUT_DOMAIN: &[u8] = b"links/contact-psi/output/v1\0";
const PROOF_DOMAIN: &[u8] = b"links/contact-psi/dleq/v1\0";
const FILTER_DOMAIN: &[u8] = b"links/contact-psi/filter/v1\0";

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ContactPsiError {
    #[error("phone must be canonical E.164")]
    InvalidPhone,
    #[error("invalid contact PSI point")]
    InvalidPoint,
    #[error("invalid contact PSI scalar")]
    InvalidScalar,
    #[error("invalid contact PSI proof")]
    InvalidProof,
    #[error("invalid contact PSI membership filter")]
    InvalidFilter,
    #[error("contact PSI batch or directory is too large")]
    TooLarge,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OprfEvaluation {
    pub evaluated_point: [u8; POINT_BYTES],
    pub proof: [u8; PROOF_BYTES],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContactPsiFilter {
    bits: Vec<u8>,
    hash_count: u8,
    item_count: u64,
}

impl ContactPsiFilter {
    pub fn from_tokens(tokens: &[[u8; TOKEN_BYTES]]) -> Result<Self, ContactPsiError> {
        if tokens.len() > MAX_DIRECTORY_TOKENS {
            return Err(ContactPsiError::TooLarge);
        }
        let byte_count = filter_byte_count(tokens.len() as u64)?;
        let mut filter = Self {
            bits: vec![0; byte_count],
            hash_count: FILTER_HASH_COUNT,
            item_count: tokens.len() as u64,
        };
        for token in tokens {
            filter.insert(token);
        }
        Ok(filter)
    }

    pub fn from_parts(
        bits: Vec<u8>,
        hash_count: u8,
        item_count: u64,
    ) -> Result<Self, ContactPsiError> {
        if hash_count != FILTER_HASH_COUNT || bits.len() != filter_byte_count(item_count)? {
            return Err(ContactPsiError::InvalidFilter);
        }
        Ok(Self {
            bits,
            hash_count,
            item_count,
        })
    }

    pub fn bits(&self) -> &[u8] {
        &self.bits
    }

    pub fn hash_count(&self) -> u8 {
        self.hash_count
    }

    pub fn item_count(&self) -> u64 {
        self.item_count
    }

    pub fn contains(&self, token: &[u8; TOKEN_BYTES]) -> bool {
        if self.bits.is_empty() {
            return false;
        }
        let bit_count = self.bits.len() as u64 * 8;
        let (first, step) = filter_hashes(token);
        (0..self.hash_count).all(|index| {
            let bit = first.wrapping_add(u64::from(index).wrapping_mul(step)) % bit_count;
            self.bits[(bit / 8) as usize] & (1 << (bit % 8)) != 0
        })
    }

    fn insert(&mut self, token: &[u8; TOKEN_BYTES]) {
        let bit_count = self.bits.len() as u64 * 8;
        let (first, step) = filter_hashes(token);
        for index in 0..self.hash_count {
            let bit = first.wrapping_add(u64::from(index).wrapping_mul(step)) % bit_count;
            self.bits[(bit / 8) as usize] |= 1 << (bit % 8);
        }
    }
}

pub fn contact_input(phone_e164: &str) -> Result<[u8; 32], ContactPsiError> {
    validate_e164(phone_e164)?;
    let mut hasher = Sha256::new();
    hasher.update(INPUT_DOMAIN);
    hasher.update(phone_e164.as_bytes());
    Ok(hasher.finalize().into())
}

pub fn validate_e164(phone: &str) -> Result<(), ContactPsiError> {
    let digits = phone
        .strip_prefix('+')
        .ok_or(ContactPsiError::InvalidPhone)?;
    if !(8..=15).contains(&digits.len())
        || digits.starts_with('0')
        || !digits.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(ContactPsiError::InvalidPhone);
    }
    Ok(())
}

pub fn scalar_from_randomness(randomness: &[u8; 64]) -> Result<[u8; 32], ContactPsiError> {
    let scalar = Scalar::from_bytes_mod_order_wide(randomness);
    if scalar == Scalar::ZERO {
        return Err(ContactPsiError::InvalidScalar);
    }
    Ok(scalar.to_bytes())
}

pub fn blind_input(
    input: &[u8; 32],
    randomness: &[u8; 64],
) -> Result<([u8; POINT_BYTES], [u8; 32]), ContactPsiError> {
    let blind_bytes = scalar_from_randomness(randomness)?;
    let blind = scalar_from_bytes(&blind_bytes)?;
    let point = hash_to_group(input);
    let blinded = point * blind;
    if blinded.is_identity().into() {
        return Err(ContactPsiError::InvalidPoint);
    }
    Ok((blinded.compress().to_bytes(), blind_bytes))
}

pub fn server_public_key(server_secret: &[u8; 32]) -> [u8; POINT_BYTES] {
    (RISTRETTO_BASEPOINT_POINT * server_scalar(server_secret))
        .compress()
        .to_bytes()
}

pub fn evaluate_blinded(
    server_secret: &[u8; 32],
    blinded_bytes: &[u8; POINT_BYTES],
    proof_randomness: &[u8; 64],
) -> Result<OprfEvaluation, ContactPsiError> {
    let blinded = decode_point(blinded_bytes)?;
    let secret = server_scalar(server_secret);
    let nonce_bytes = scalar_from_randomness(proof_randomness)?;
    let nonce = scalar_from_bytes(&nonce_bytes)?;
    let evaluated = blinded * secret;
    if evaluated.is_identity().into() {
        return Err(ContactPsiError::InvalidPoint);
    }
    let public_key = (RISTRETTO_BASEPOINT_POINT * secret).compress().to_bytes();
    let evaluated_bytes = evaluated.compress().to_bytes();
    let a = RISTRETTO_BASEPOINT_POINT * nonce;
    let b = blinded * nonce;
    let challenge = proof_challenge(
        &public_key,
        blinded_bytes,
        &evaluated_bytes,
        &a.compress().to_bytes(),
        &b.compress().to_bytes(),
    );
    let response = nonce + challenge * secret;
    let mut proof = [0u8; PROOF_BYTES];
    proof[..32].copy_from_slice(&challenge.to_bytes());
    proof[32..].copy_from_slice(&response.to_bytes());
    Ok(OprfEvaluation {
        evaluated_point: evaluated_bytes,
        proof,
    })
}

pub fn verify_evaluation(
    server_public_key_bytes: &[u8; POINT_BYTES],
    blinded_bytes: &[u8; POINT_BYTES],
    evaluation: &OprfEvaluation,
) -> Result<(), ContactPsiError> {
    let public_key = decode_point(server_public_key_bytes)?;
    let blinded = decode_point(blinded_bytes)?;
    let evaluated = decode_point(&evaluation.evaluated_point)?;
    let challenge = canonical_scalar(&evaluation.proof[..32])?;
    let response = canonical_scalar(&evaluation.proof[32..])?;
    let a = (RISTRETTO_BASEPOINT_POINT * response) - (public_key * challenge);
    let b = (blinded * response) - (evaluated * challenge);
    let expected = proof_challenge(
        server_public_key_bytes,
        blinded_bytes,
        &evaluation.evaluated_point,
        &a.compress().to_bytes(),
        &b.compress().to_bytes(),
    );
    if expected != challenge {
        return Err(ContactPsiError::InvalidProof);
    }
    Ok(())
}

pub fn unblind(
    evaluated_bytes: &[u8; POINT_BYTES],
    blind_bytes: &[u8; 32],
) -> Result<[u8; POINT_BYTES], ContactPsiError> {
    let evaluated = decode_point(evaluated_bytes)?;
    let blind = scalar_from_bytes(blind_bytes)?;
    let unblinded = evaluated * blind.invert();
    if unblinded.is_identity().into() {
        return Err(ContactPsiError::InvalidPoint);
    }
    Ok(unblinded.compress().to_bytes())
}

pub fn oprf_output(
    input: &[u8; 32],
    unblinded_point: &[u8; POINT_BYTES],
) -> Result<[u8; TOKEN_BYTES], ContactPsiError> {
    decode_point(unblinded_point)?;
    let mut hasher = Sha256::new();
    hasher.update(OUTPUT_DOMAIN);
    hasher.update(input);
    hasher.update(unblinded_point);
    Ok(hasher.finalize().into())
}

pub fn directory_token(
    phone_e164: &str,
    server_secret: &[u8; 32],
) -> Result<[u8; TOKEN_BYTES], ContactPsiError> {
    let input = contact_input(phone_e164)?;
    let point = hash_to_group(&input) * server_scalar(server_secret);
    oprf_output(&input, &point.compress().to_bytes())
}

fn hash_to_group(input: &[u8; 32]) -> RistrettoPoint {
    let mut hasher = Sha512::new();
    hasher.update(HASH_TO_GROUP_DOMAIN);
    hasher.update(input);
    let wide: [u8; 64] = hasher.finalize().into();
    RistrettoPoint::from_uniform_bytes(&wide)
}

fn server_scalar(server_secret: &[u8; 32]) -> Scalar {
    let mut hasher = Sha512::new();
    hasher.update(PROOF_DOMAIN);
    hasher.update(server_secret);
    let wide: [u8; 64] = hasher.finalize().into();
    let scalar = Scalar::from_bytes_mod_order_wide(&wide);
    if scalar == Scalar::ZERO {
        Scalar::ONE
    } else {
        scalar
    }
}

fn decode_point(bytes: &[u8; POINT_BYTES]) -> Result<RistrettoPoint, ContactPsiError> {
    let point = CompressedRistretto(*bytes)
        .decompress()
        .ok_or(ContactPsiError::InvalidPoint)?;
    if point.is_identity().into() {
        return Err(ContactPsiError::InvalidPoint);
    }
    Ok(point)
}

fn scalar_from_bytes(bytes: &[u8; 32]) -> Result<Scalar, ContactPsiError> {
    canonical_scalar(bytes)
}

fn canonical_scalar(bytes: &[u8]) -> Result<Scalar, ContactPsiError> {
    let bytes: [u8; 32] = bytes
        .try_into()
        .map_err(|_| ContactPsiError::InvalidScalar)?;
    Option::from(Scalar::from_canonical_bytes(bytes)).ok_or(ContactPsiError::InvalidScalar)
}

fn proof_challenge(
    public_key: &[u8; POINT_BYTES],
    blinded: &[u8; POINT_BYTES],
    evaluated: &[u8; POINT_BYTES],
    a: &[u8; POINT_BYTES],
    b: &[u8; POINT_BYTES],
) -> Scalar {
    let mut hasher = Sha512::new();
    hasher.update(PROOF_DOMAIN);
    hasher.update(public_key);
    hasher.update(blinded);
    hasher.update(evaluated);
    hasher.update(a);
    hasher.update(b);
    let wide: [u8; 64] = hasher.finalize().into();
    Scalar::from_bytes_mod_order_wide(&wide)
}

fn filter_hashes(token: &[u8; TOKEN_BYTES]) -> (u64, u64) {
    let mut hasher = Sha512::new();
    hasher.update(FILTER_DOMAIN);
    hasher.update(token);
    let digest: [u8; 64] = hasher.finalize().into();
    let first = u64::from_be_bytes(digest[..8].try_into().expect("fixed digest length"));
    let step = u64::from_be_bytes(digest[8..16].try_into().expect("fixed digest length")) | 1;
    (first, step)
}

fn filter_byte_count(item_count: u64) -> Result<usize, ContactPsiError> {
    if item_count > MAX_DIRECTORY_TOKENS as u64 {
        return Err(ContactPsiError::TooLarge);
    }
    let item_count = item_count as usize;
    let bit_count = FILTER_MIN_BITS.max(
        item_count
            .saturating_mul(FILTER_BITS_PER_TOKEN)
            .div_ceil(8)
            .saturating_mul(8),
    );
    let byte_count = bit_count / 8;
    if byte_count > FILTER_MAX_BYTES {
        return Err(ContactPsiError::TooLarge);
    }
    Ok(byte_count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blinded_evaluation_verifies_and_unblinds() {
        let secret = [9u8; 32];
        let input = contact_input("+12025550123").unwrap();
        let (blinded, blind) = blind_input(&input, &[7u8; 64]).unwrap();
        let evaluation = evaluate_blinded(&secret, &blinded, &[8u8; 64]).unwrap();
        verify_evaluation(&server_public_key(&secret), &blinded, &evaluation).unwrap();
        let unblinded = unblind(&evaluation.evaluated_point, &blind).unwrap();
        let expected = directory_token("+12025550123", &secret).unwrap();
        assert_eq!(oprf_output(&input, &unblinded).unwrap(), expected);
    }

    #[test]
    fn modified_evaluation_fails_proof() {
        let secret = [9u8; 32];
        let input = contact_input("+12025550123").unwrap();
        let (blinded, _) = blind_input(&input, &[7u8; 64]).unwrap();
        let mut evaluation = evaluate_blinded(&secret, &blinded, &[8u8; 64]).unwrap();
        evaluation.evaluated_point[0] ^= 1;
        assert_eq!(
            verify_evaluation(&server_public_key(&secret), &blinded, &evaluation),
            Err(ContactPsiError::InvalidPoint)
        );
    }
}
