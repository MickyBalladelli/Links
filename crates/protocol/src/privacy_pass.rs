//! Privacy Pass private-token issuance and redemption primitives.
//!
//! This uses the RFC 9578 VOPRF(P-384, SHA-384) profile. Issuance sees only a
//! blinded message. Redemption sees only a challenge-bound, one-time token and
//! never receives the account that obtained it.

use p384::{
    elliptic_curve::{
        ff::PrimeField,
        group::Group,
        hash2curve::{ExpandMsgXmd, GroupDigest},
        sec1::{FromEncodedPoint, ToEncodedPoint},
        subtle::ConstantTimeEq,
    },
    EncodedPoint, FieldBytes, NistP384, ProjectivePoint, Scalar,
};
use sha2::{Digest, Sha256, Sha384};
use thiserror::Error;

pub const VERSION: u32 = 1;
pub const TOKEN_TYPE: u16 = 0x0001;
pub const TOKEN_KEY_ID_BYTES: usize = 32;
pub const NONCE_BYTES: usize = 32;
pub const CHALLENGE_BYTES: usize = 40;
pub const SCALAR_BYTES: usize = 48;
pub const POINT_BYTES: usize = 49;
pub const PROOF_BYTES: usize = SCALAR_BYTES * 2;
pub const TOKEN_INPUT_BYTES: usize = 2 + NONCE_BYTES + 32 + TOKEN_KEY_ID_BYTES;
pub const AUTHENTICATOR_BYTES: usize = 48;
pub const TOKEN_BYTES: usize = 2 + NONCE_BYTES + 32 + TOKEN_KEY_ID_BYTES + AUTHENTICATOR_BYTES;

const CONTEXT: &[u8] = b"OPRFV1-\x01-P384-SHA384";
const HASH_TO_GROUP_DST: &[u8] = b"HashToGroup-OPRFV1-\x01-P384-SHA384";
const HASH_TO_SCALAR_DST: &[u8] = b"HashToScalar-OPRFV1-\x01-P384-SHA384";
const DERIVE_KEY_DST: &[u8] = b"DeriveKeyPairOPRFV1-\x01-P384-SHA384";
const SEED_DST_PREFIX: &[u8] = b"Seed-";
const COMPOSITE_SUFFIX: &[u8] = b"Composite";
const CHALLENGE_SUFFIX: &[u8] = b"Challenge";
const FINALIZE_SUFFIX: &[u8] = b"Finalize";

#[derive(Debug, Error, PartialEq, Eq)]
pub enum PrivacyPassError {
    #[error("invalid Privacy Pass scalar")]
    InvalidScalar,
    #[error("invalid Privacy Pass point")]
    InvalidPoint,
    #[error("invalid Privacy Pass proof")]
    InvalidProof,
    #[error("invalid Privacy Pass token")]
    InvalidToken,
    #[error("expired Privacy Pass challenge")]
    ExpiredChallenge,
    #[error("Privacy Pass key mismatch")]
    KeyMismatch,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct IssuerParameters {
    pub public_key: [u8; POINT_BYTES],
    pub token_key_id: [u8; TOKEN_KEY_ID_BYTES],
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct TokenRequest {
    pub token_type: u16,
    pub truncated_token_key_id: u8,
    pub blinded_message: [u8; POINT_BYTES],
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct TokenResponse {
    pub evaluated_message: [u8; POINT_BYTES],
    pub proof: [u8; PROOF_BYTES],
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct BlindState {
    pub nonce: [u8; NONCE_BYTES],
    pub challenge_digest: [u8; 32],
    pub token_key_id: [u8; TOKEN_KEY_ID_BYTES],
    pub token_input: [u8; TOKEN_INPUT_BYTES],
    pub blind: [u8; SCALAR_BYTES],
    pub blinded_message: [u8; POINT_BYTES],
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct PrivacyPassToken {
    pub nonce: [u8; NONCE_BYTES],
    pub challenge_digest: [u8; 32],
    pub token_key_id: [u8; TOKEN_KEY_ID_BYTES],
    pub authenticator: [u8; AUTHENTICATOR_BYTES],
}

impl IssuerParameters {
    pub fn from_seed(seed: &[u8; 32]) -> Result<Self, PrivacyPassError> {
        let secret = derive_key(seed)?;
        let public_key = serialize_point(&(ProjectivePoint::GENERATOR * secret))?;
        let token_key_id = Sha256::digest(public_key).into();
        Ok(Self {
            public_key,
            token_key_id,
        })
    }
}

impl TokenRequest {
    pub fn new(
        state: &BlindState,
        parameters: &IssuerParameters,
    ) -> Result<Self, PrivacyPassError> {
        if state.token_key_id != parameters.token_key_id {
            return Err(PrivacyPassError::KeyMismatch);
        }
        Ok(Self {
            token_type: TOKEN_TYPE,
            truncated_token_key_id: parameters.token_key_id[TOKEN_KEY_ID_BYTES - 1],
            blinded_message: state.blinded_message,
        })
    }
}

impl PrivacyPassToken {
    pub fn to_bytes(&self) -> [u8; TOKEN_BYTES] {
        let mut bytes = [0u8; TOKEN_BYTES];
        bytes[..2].copy_from_slice(&TOKEN_TYPE.to_be_bytes());
        bytes[2..2 + NONCE_BYTES].copy_from_slice(&self.nonce);
        let challenge_end = 2 + NONCE_BYTES + 32;
        bytes[2 + NONCE_BYTES..challenge_end].copy_from_slice(&self.challenge_digest);
        let key_end = challenge_end + TOKEN_KEY_ID_BYTES;
        bytes[challenge_end..key_end].copy_from_slice(&self.token_key_id);
        bytes[key_end..].copy_from_slice(&self.authenticator);
        bytes
    }

    pub fn from_bytes(bytes: &[u8; TOKEN_BYTES]) -> Result<Self, PrivacyPassError> {
        if u16::from_be_bytes([bytes[0], bytes[1]]) != TOKEN_TYPE {
            return Err(PrivacyPassError::InvalidToken);
        }
        let mut nonce = [0u8; NONCE_BYTES];
        nonce.copy_from_slice(&bytes[2..2 + NONCE_BYTES]);
        let challenge_start = 2 + NONCE_BYTES;
        let challenge_end = challenge_start + 32;
        let mut challenge_digest = [0u8; 32];
        challenge_digest.copy_from_slice(&bytes[challenge_start..challenge_end]);
        let key_end = challenge_end + TOKEN_KEY_ID_BYTES;
        let mut token_key_id = [0u8; TOKEN_KEY_ID_BYTES];
        token_key_id.copy_from_slice(&bytes[challenge_end..key_end]);
        let mut authenticator = [0u8; AUTHENTICATOR_BYTES];
        authenticator.copy_from_slice(&bytes[key_end..]);
        Ok(Self {
            nonce,
            challenge_digest,
            token_key_id,
            authenticator,
        })
    }
}

/// Blind a fresh token for an origin challenge. The challenge contains a
/// random 32-byte nonce followed by a big-endian expiry timestamp in ms.
pub fn blind(
    challenge: &[u8; CHALLENGE_BYTES],
    nonce: &[u8; NONCE_BYTES],
    parameters: &IssuerParameters,
    blind_bytes: &[u8; SCALAR_BYTES],
) -> Result<(BlindState, TokenRequest), PrivacyPassError> {
    let challenge_digest: [u8; 32] = Sha256::digest(challenge).into();
    let token_input = token_input(nonce, &challenge_digest, &parameters.token_key_id);
    let blind = scalar_from_bytes(blind_bytes)?;
    let input_element = hash_to_group(&token_input)?;
    let blinded = input_element * blind;
    let blinded_message = serialize_point(&blinded)?;
    let state = BlindState {
        nonce: *nonce,
        challenge_digest,
        token_key_id: parameters.token_key_id,
        token_input,
        blind: *blind_bytes,
        blinded_message,
    };
    let request = TokenRequest::new(&state, parameters)?;
    Ok((state, request))
}

pub fn finalize(
    state: &BlindState,
    parameters: &IssuerParameters,
    response: &TokenResponse,
) -> Result<PrivacyPassToken, PrivacyPassError> {
    if state.token_key_id != parameters.token_key_id {
        return Err(PrivacyPassError::KeyMismatch);
    }
    verify_proof(&parameters.public_key, &state.blinded_message, response)?;
    let evaluated = decode_point(&response.evaluated_message)?;
    let blind = scalar_from_bytes(&state.blind)?;
    let blind_inverse: Scalar =
        Option::from(blind.invert()).ok_or(PrivacyPassError::InvalidScalar)?;
    let unblinded = evaluated * blind_inverse;
    let authenticator = finalize_hash(&state.token_input, &serialize_point(&unblinded)?);
    Ok(PrivacyPassToken {
        nonce: state.nonce,
        challenge_digest: state.challenge_digest,
        token_key_id: state.token_key_id,
        authenticator,
    })
}

pub fn evaluate(
    seed: &[u8; 32],
    request: &TokenRequest,
    proof_randomness: &[u8; SCALAR_BYTES],
) -> Result<TokenResponse, PrivacyPassError> {
    if request.token_type != TOKEN_TYPE {
        return Err(PrivacyPassError::InvalidToken);
    }
    let parameters = IssuerParameters::from_seed(seed)?;
    if request.truncated_token_key_id != parameters.token_key_id[TOKEN_KEY_ID_BYTES - 1] {
        return Err(PrivacyPassError::KeyMismatch);
    }
    let secret = derive_key(seed)?;
    let blinded = decode_point(&request.blinded_message)?;
    let evaluated = blinded * secret;
    let evaluated_message = serialize_point(&evaluated)?;
    let proof = generate_proof(
        &secret,
        &parameters.public_key,
        &request.blinded_message,
        &evaluated_message,
        proof_randomness,
    )?;
    Ok(TokenResponse {
        evaluated_message,
        proof,
    })
}

pub fn verify_token(
    token: &PrivacyPassToken,
    challenge: &[u8; CHALLENGE_BYTES],
    now_ms: u64,
    seed: &[u8; 32],
) -> Result<(), PrivacyPassError> {
    if challenge_expiry_ms(challenge) <= now_ms {
        return Err(PrivacyPassError::ExpiredChallenge);
    }
    let parameters = IssuerParameters::from_seed(seed)?;
    if token.token_key_id != parameters.token_key_id {
        return Err(PrivacyPassError::KeyMismatch);
    }
    let expected_challenge_digest: [u8; 32] = Sha256::digest(challenge).into();
    if token.challenge_digest != expected_challenge_digest {
        return Err(PrivacyPassError::InvalidToken);
    }
    let secret = derive_key(seed)?;
    let token_input = token_input(&token.nonce, &token.challenge_digest, &token.token_key_id);
    let issued = hash_to_group(&token_input)? * secret;
    let expected = finalize_hash(&token_input, &serialize_point(&issued)?);
    if expected.ct_eq(&token.authenticator).into() {
        Ok(())
    } else {
        Err(PrivacyPassError::InvalidToken)
    }
}

pub fn challenge_expiry_ms(challenge: &[u8; CHALLENGE_BYTES]) -> u64 {
    let mut bytes = [0u8; 8];
    bytes.copy_from_slice(&challenge[CHALLENGE_BYTES - 8..]);
    u64::from_be_bytes(bytes)
}

fn derive_key(seed: &[u8; 32]) -> Result<Scalar, PrivacyPassError> {
    let info = b"PrivacyPass";
    let mut input = Vec::with_capacity(seed.len() + 2 + info.len() + 1);
    input.extend_from_slice(seed);
    append_len(&mut input, info.len());
    input.extend_from_slice(info);
    for counter in 0u8..=u8::MAX {
        input.push(counter);
        let scalar = hash_to_scalar(&input, DERIVE_KEY_DST)?;
        input.pop();
        if scalar != Scalar::ZERO {
            return Ok(scalar);
        }
    }
    Err(PrivacyPassError::InvalidScalar)
}

fn token_input(
    nonce: &[u8; NONCE_BYTES],
    challenge_digest: &[u8; 32],
    token_key_id: &[u8; TOKEN_KEY_ID_BYTES],
) -> [u8; TOKEN_INPUT_BYTES] {
    let mut input = [0u8; TOKEN_INPUT_BYTES];
    input[..2].copy_from_slice(&TOKEN_TYPE.to_be_bytes());
    input[2..2 + NONCE_BYTES].copy_from_slice(nonce);
    let challenge_end = 2 + NONCE_BYTES + 32;
    input[2 + NONCE_BYTES..challenge_end].copy_from_slice(challenge_digest);
    input[challenge_end..].copy_from_slice(token_key_id);
    input
}

fn generate_proof(
    secret: &Scalar,
    public_key: &[u8; POINT_BYTES],
    blinded_message: &[u8; POINT_BYTES],
    evaluated_message: &[u8; POINT_BYTES],
    proof_randomness: &[u8; SCALAR_BYTES],
) -> Result<[u8; PROOF_BYTES], PrivacyPassError> {
    let blinded = decode_point(blinded_message)?;
    let evaluated = decode_point(evaluated_message)?;
    let (composite, evaluated_composite) =
        compute_composites(public_key, &[blinded], &[evaluated])?;
    let nonce = scalar_from_bytes(proof_randomness)?;
    let t2 = ProjectivePoint::GENERATOR * nonce;
    let t3 = composite * nonce;
    let c = proof_challenge(
        public_key,
        &serialize_point(&composite)?,
        &serialize_point(&evaluated_composite)?,
        &serialize_point(&t2)?,
        &serialize_point(&t3)?,
    )?;
    let response = nonce - c * secret;
    let mut proof = [0u8; PROOF_BYTES];
    proof[..SCALAR_BYTES].copy_from_slice(&c.to_bytes());
    proof[SCALAR_BYTES..].copy_from_slice(&response.to_bytes());
    Ok(proof)
}

fn verify_proof(
    public_key: &[u8; POINT_BYTES],
    blinded_message: &[u8; POINT_BYTES],
    response: &TokenResponse,
) -> Result<(), PrivacyPassError> {
    let public = decode_point(public_key)?;
    let blinded = decode_point(blinded_message)?;
    let evaluated = decode_point(&response.evaluated_message)?;
    let challenge = scalar_from_slice(&response.proof[..SCALAR_BYTES])?;
    let response_scalar = scalar_from_slice(&response.proof[SCALAR_BYTES..])?;
    let (composite, evaluated_composite) =
        compute_composites(public_key, &[blinded], &[evaluated])?;
    let t2 = ProjectivePoint::GENERATOR * response_scalar + public * challenge;
    let t3 = composite * response_scalar + evaluated_composite * challenge;
    let expected = proof_challenge(
        public_key,
        &serialize_point(&composite)?,
        &serialize_point(&evaluated_composite)?,
        &serialize_point(&t2)?,
        &serialize_point(&t3)?,
    )?;
    if expected == challenge {
        Ok(())
    } else {
        Err(PrivacyPassError::InvalidProof)
    }
}

fn compute_composites(
    public_key: &[u8; POINT_BYTES],
    blinded: &[ProjectivePoint; 1],
    evaluated: &[ProjectivePoint; 1],
) -> Result<(ProjectivePoint, ProjectivePoint), PrivacyPassError> {
    let mut seed_transcript = Vec::new();
    append_bytes(&mut seed_transcript, public_key);
    let mut seed_dst = Vec::with_capacity(SEED_DST_PREFIX.len() + CONTEXT.len());
    seed_dst.extend_from_slice(SEED_DST_PREFIX);
    seed_dst.extend_from_slice(CONTEXT);
    append_bytes(&mut seed_transcript, &seed_dst);
    let seed = Sha384::digest(seed_transcript);
    let mut composite = ProjectivePoint::IDENTITY;
    let mut evaluated_composite = ProjectivePoint::IDENTITY;
    for index in 0..1usize {
        let blinded_bytes = serialize_point(&blinded[index])?;
        let evaluated_bytes = serialize_point(&evaluated[index])?;
        let mut transcript = Vec::new();
        append_bytes(&mut transcript, &seed);
        transcript.extend_from_slice(&(index as u16).to_be_bytes());
        append_bytes(&mut transcript, &blinded_bytes);
        append_bytes(&mut transcript, &evaluated_bytes);
        transcript.extend_from_slice(COMPOSITE_SUFFIX);
        let coefficient = hash_to_scalar(&transcript, HASH_TO_SCALAR_DST)?;
        composite += blinded[index] * coefficient;
        evaluated_composite += evaluated[index] * coefficient;
    }
    Ok((composite, evaluated_composite))
}

fn proof_challenge(
    public_key: &[u8; POINT_BYTES],
    composite: &[u8; POINT_BYTES],
    evaluated_composite: &[u8; POINT_BYTES],
    t2: &[u8; POINT_BYTES],
    t3: &[u8; POINT_BYTES],
) -> Result<Scalar, PrivacyPassError> {
    let mut transcript = Vec::new();
    append_bytes(&mut transcript, public_key);
    append_bytes(&mut transcript, composite);
    append_bytes(&mut transcript, evaluated_composite);
    append_bytes(&mut transcript, t2);
    append_bytes(&mut transcript, t3);
    transcript.extend_from_slice(CHALLENGE_SUFFIX);
    hash_to_scalar(&transcript, HASH_TO_SCALAR_DST)
}

fn finalize_hash(
    input: &[u8; TOKEN_INPUT_BYTES],
    issued: &[u8; POINT_BYTES],
) -> [u8; AUTHENTICATOR_BYTES] {
    let mut transcript = Vec::new();
    append_bytes(&mut transcript, input);
    append_bytes(&mut transcript, issued);
    transcript.extend_from_slice(FINALIZE_SUFFIX);
    Sha384::digest(transcript).into()
}

fn hash_to_group(input: &[u8; TOKEN_INPUT_BYTES]) -> Result<ProjectivePoint, PrivacyPassError> {
    <NistP384 as GroupDigest>::hash_from_bytes::<ExpandMsgXmd<Sha384>>(
        &[input],
        &[HASH_TO_GROUP_DST],
    )
    .map_err(|_| PrivacyPassError::InvalidPoint)
}

fn hash_to_scalar(input: &[u8], dst: &[u8]) -> Result<Scalar, PrivacyPassError> {
    <NistP384 as GroupDigest>::hash_to_scalar::<ExpandMsgXmd<Sha384>>(&[input], &[dst])
        .map_err(|_| PrivacyPassError::InvalidScalar)
}

fn serialize_point(point: &ProjectivePoint) -> Result<[u8; POINT_BYTES], PrivacyPassError> {
    if bool::from(point.is_identity()) {
        return Err(PrivacyPassError::InvalidPoint);
    }
    point
        .to_encoded_point(true)
        .as_bytes()
        .try_into()
        .map_err(|_| PrivacyPassError::InvalidPoint)
}

fn decode_point(bytes: &[u8; POINT_BYTES]) -> Result<ProjectivePoint, PrivacyPassError> {
    let encoded = EncodedPoint::from_bytes(bytes).map_err(|_| PrivacyPassError::InvalidPoint)?;
    let point: ProjectivePoint = Option::from(ProjectivePoint::from_encoded_point(&encoded))
        .ok_or(PrivacyPassError::InvalidPoint)?;
    if bool::from(point.is_identity()) {
        return Err(PrivacyPassError::InvalidPoint);
    }
    Ok(point)
}

fn scalar_from_bytes(bytes: &[u8; SCALAR_BYTES]) -> Result<Scalar, PrivacyPassError> {
    scalar_from_slice(bytes)
}

fn scalar_from_slice(bytes: &[u8]) -> Result<Scalar, PrivacyPassError> {
    let bytes: &[u8; SCALAR_BYTES] = bytes
        .try_into()
        .map_err(|_| PrivacyPassError::InvalidScalar)?;
    let scalar = Option::from(Scalar::from_repr(FieldBytes::clone_from_slice(bytes)))
        .ok_or(PrivacyPassError::InvalidScalar)?;
    if scalar == Scalar::ZERO {
        return Err(PrivacyPassError::InvalidScalar);
    }
    Ok(scalar)
}

fn append_len(output: &mut Vec<u8>, length: usize) {
    output.extend_from_slice(&(length as u16).to_be_bytes());
}

fn append_bytes(output: &mut Vec<u8>, bytes: &[u8]) {
    append_len(output, bytes.len());
    output.extend_from_slice(bytes);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn issue_finalize_and_verify_round_trip() {
        let seed = [7u8; 32];
        let parameters = IssuerParameters::from_seed(&seed).unwrap();
        let mut challenge = [0u8; CHALLENGE_BYTES];
        challenge[..32].copy_from_slice(&[3u8; 32]);
        challenge[32..].copy_from_slice(&10_000u64.to_be_bytes());
        let (state, request) = blind(
            &challenge,
            &[4u8; NONCE_BYTES],
            &parameters,
            &[5u8; SCALAR_BYTES],
        )
        .unwrap();
        let response = evaluate(&seed, &request, &[6u8; SCALAR_BYTES]).unwrap();
        let token = finalize(&state, &parameters, &response).unwrap();
        verify_token(&token, &challenge, 1, &seed).unwrap();
    }

    #[test]
    fn challenge_binding_rejects_other_challenge() {
        let seed = [7u8; 32];
        let parameters = IssuerParameters::from_seed(&seed).unwrap();
        let mut challenge = [0u8; CHALLENGE_BYTES];
        challenge[32..].copy_from_slice(&10_000u64.to_be_bytes());
        let (state, request) = blind(
            &challenge,
            &[4u8; NONCE_BYTES],
            &parameters,
            &[5u8; SCALAR_BYTES],
        )
        .unwrap();
        let response = evaluate(&seed, &request, &[6u8; SCALAR_BYTES]).unwrap();
        let token = finalize(&state, &parameters, &response).unwrap();
        challenge[0] = 1;
        assert_eq!(
            verify_token(&token, &challenge, 1, &seed),
            Err(PrivacyPassError::InvalidToken)
        );
    }
}
