//! Ed25519 identities and RFC 9420 basic credentials. A basic credential is an
//! assertion, not an OTP proof or a certificate; enrollment authenticates it.
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use thiserror::Error;
use tls_codec::{Serialize, TlsSerialize, TlsSize, VLBytes};
use uuid::Uuid;
use zeroize::Zeroizing;

const PQXDH_EC_ENCODING_TAG: u8 = 1;
const PQXDH_KEM_ENCODING_TAG: u8 = 2;
const PQXDH_IDENTITY_BINDING_DOMAIN: &[u8] = b"links/pqxdh/identity-binding/v1\0";
const PQXDH_SIGNED_PREKEY_DOMAIN: &[u8] = b"links/pqxdh/signed-prekey/v1\0";
const PQXDH_KEM_PREKEY_DOMAIN: &[u8] = b"links/pqxdh/kem-prekey/v1\0";

#[derive(Debug, Error, PartialEq, Eq)]
pub enum IdentityError {
    #[error("invalid identity input")]
    Invalid,
    #[error("identity authentication failed")]
    Authentication,
    #[error("secure randomness unavailable")]
    RandomUnavailable,
    #[error("hardware-backed key storage unavailable")]
    HardwareUnavailable,
}

/// Random Ed25519 seed. Intentionally not Clone/Debug; owned bytes wipe on drop.
pub struct IdentitySeed(Zeroizing<[u8; 32]>);
impl IdentitySeed {
    pub fn generate() -> Result<Self, IdentityError> {
        let mut seed = Zeroizing::new([0u8; 32]);
        getrandom::fill(seed.as_mut()).map_err(|_| IdentityError::RandomUnavailable)?;
        Ok(Self(seed))
    }
    /// Only for bytes recovered from the device vault, never from phone numbers.
    pub fn from_vault(bytes: Zeroizing<[u8; 32]>) -> Self {
        Self(bytes)
    }
    pub fn expose_for_wrapping(&self) -> &[u8; 32] {
        &self.0
    }
    pub fn public_key(&self) -> [u8; 32] {
        SigningKey::from_bytes(&self.0).verifying_key().to_bytes()
    }
    pub fn sign(&self, transcript: &[u8]) -> [u8; 64] {
        SigningKey::from_bytes(&self.0).sign(transcript).to_bytes()
    }
}

pub fn validate_public_key(key: &[u8; 32]) -> Result<(), IdentityError> {
    let key = VerifyingKey::from_bytes(key).map_err(|_| IdentityError::Invalid)?;
    if key.is_weak() {
        return Err(IdentityError::Invalid);
    }
    Ok(())
}
pub fn verify(key: &[u8; 32], transcript: &[u8], signature: &[u8]) -> Result<(), IdentityError> {
    validate_public_key(key)?;
    let key = VerifyingKey::from_bytes(key).map_err(|_| IdentityError::Invalid)?;
    let signature = Signature::from_slice(signature).map_err(|_| IdentityError::Authentication)?;
    key.verify_strict(transcript, &signature)
        .map_err(|_| IdentityError::Authentication)
}

pub fn pqxdh_identity_binding_transcript(dh_key: &[u8; 32]) -> Vec<u8> {
    let mut transcript = PQXDH_IDENTITY_BINDING_DOMAIN.to_vec();
    transcript.push(PQXDH_EC_ENCODING_TAG);
    transcript.extend_from_slice(dh_key);
    transcript
}

pub fn pqxdh_signed_prekey_transcript(
    identity_dh_key: &[u8; 32],
    prekey_id: u64,
    prekey: &[u8; 32],
) -> Vec<u8> {
    let mut transcript = PQXDH_SIGNED_PREKEY_DOMAIN.to_vec();
    transcript.push(PQXDH_EC_ENCODING_TAG);
    transcript.extend_from_slice(identity_dh_key);
    transcript.extend_from_slice(&prekey_id.to_be_bytes());
    transcript.push(PQXDH_EC_ENCODING_TAG);
    transcript.extend_from_slice(prekey);
    transcript
}

pub fn pqxdh_kem_prekey_transcript(
    identity_dh_key: &[u8; 32],
    prekey_id: u64,
    one_time: bool,
    prekey: &[u8],
) -> Vec<u8> {
    let mut transcript = PQXDH_KEM_PREKEY_DOMAIN.to_vec();
    transcript.push(PQXDH_EC_ENCODING_TAG);
    transcript.extend_from_slice(identity_dh_key);
    transcript.extend_from_slice(&prekey_id.to_be_bytes());
    transcript.push(u8::from(one_time));
    transcript.push(PQXDH_KEM_ENCODING_TAG);
    transcript.extend_from_slice(prekey);
    transcript
}

/// Proof-of-possession before sending OTP. Phone is transport-only, never an MLS
/// identity or key seed. This transcript is deterministic for request retries.
pub fn phone_auth_transcript(
    phone: &str,
    channel: &str,
    device: Uuid,
    node: Uuid,
    public_key: &[u8; 32],
) -> Result<Vec<u8>, IdentityError> {
    if phone.len() > 16
        || !phone.starts_with('+')
        || phone.len() < 9
        || !phone[1..].bytes().all(|b| b.is_ascii_digit())
        || phone.as_bytes()[1] == b'0'
        || !matches!(channel, "sms" | "whatsapp")
        || device.is_nil()
        || node.is_nil()
    {
        return Err(IdentityError::Invalid);
    }
    validate_public_key(public_key)?;
    let mut bytes = b"links/phone-auth/v1\0".to_vec();
    bytes.push(phone.len() as u8);
    bytes.extend(phone.as_bytes());
    bytes.push(channel.len() as u8);
    bytes.extend(channel.as_bytes());
    bytes.extend(device.as_bytes());
    bytes.extend(node.as_bytes());
    bytes.extend(public_key);
    Ok(bytes)
}

#[derive(Clone, PartialEq, Eq)]
pub struct DeviceBinding {
    pub user_id: Uuid,
    pub device_id: Uuid,
    pub mls_node_id: Uuid,
    pub public_key: [u8; 32],
}
#[derive(TlsSerialize, TlsSize)]
struct BasicCredential {
    credential_type: u16,
    identity: VLBytes,
}
impl DeviceBinding {
    /// RFC 9420 CredentialType=basic (1), with application-defined opaque identity.
    /// The phone and server-secret lookup digest never enter a peer credential.
    pub fn mls_credential(&self) -> Result<Vec<u8>, IdentityError> {
        if self.user_id.is_nil() || self.device_id.is_nil() || self.mls_node_id.is_nil() {
            return Err(IdentityError::Invalid);
        }
        validate_public_key(&self.public_key)?;
        let mut identity = b"links/device/v1\0".to_vec();
        identity.extend(self.user_id.as_bytes());
        identity.extend(self.device_id.as_bytes());
        identity.extend(self.mls_node_id.as_bytes());
        identity.extend(self.public_key);
        BasicCredential {
            credential_type: 1,
            identity: identity.into(),
        }
        .tls_serialize_detached()
        .map_err(|_| IdentityError::Invalid)
    }
    pub fn enrollment_transcript(
        &self,
        challenge_id: Uuid,
        nonce: &[u8; 32],
        expires_at_ms: u64,
    ) -> Result<Vec<u8>, IdentityError> {
        if challenge_id.is_nil() || expires_at_ms == 0 {
            return Err(IdentityError::Invalid);
        }
        let mut transcript = b"links/enroll/v1\0".to_vec();
        transcript.extend(challenge_id.as_bytes());
        transcript.extend(nonce);
        transcript.extend(expires_at_ms.to_be_bytes());
        transcript.extend(self.mls_credential()?);
        Ok(transcript)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn decode_hex<const N: usize>(s: &str) -> [u8; N] {
        assert_eq!(s.len(), N * 2);
        std::array::from_fn(|i| u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).unwrap())
    }
    #[test]
    fn rfc8032_test_vector_one() {
        let seed = IdentitySeed::from_vault(Zeroizing::new(decode_hex(
            "9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60",
        )));
        assert_eq!(
            seed.public_key(),
            decode_hex::<32>("d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a")
        );
        let signature = decode_hex::<64>("e5564300c360ac729086e2cc806e828a84877f1eb8e5d974d873e065224901555fb8821590a33bacc61e39701cf9b46bd25bf5f0595bbe24655141438e7a100b");
        assert_eq!(seed.sign(b""), signature);
        verify(&seed.public_key(), b"", &signature).unwrap();
        assert!(verify(&seed.public_key(), b"tampered", &signature).is_err());
    }
    #[test]
    fn random_keys_and_credentials_are_bound_to_device_and_challenge() {
        let seed = IdentitySeed::generate().unwrap();
        assert_ne!(
            seed.public_key(),
            IdentitySeed::generate().unwrap().public_key()
        );
        let mut binding = DeviceBinding {
            user_id: Uuid::from_u128(1),
            device_id: Uuid::from_u128(2),
            mls_node_id: Uuid::from_u128(3),
            public_key: seed.public_key(),
        };
        let credential = binding.mls_credential().unwrap();
        assert_eq!(&credential[..2], &[0, 1]);
        // 96-byte identity uses MLS's two-byte variable-length prefix.
        assert_eq!(&credential[2..4], &[0x40, 96]);
        let transcript = binding
            .enrollment_transcript(Uuid::from_u128(4), &[1; 32], 100)
            .unwrap();
        let signature = seed.sign(&transcript);
        verify(&binding.public_key, &transcript, &signature).unwrap();
        binding.device_id = Uuid::from_u128(5);
        assert!(verify(
            &binding.public_key,
            &binding
                .enrollment_transcript(Uuid::from_u128(4), &[1; 32], 100)
                .unwrap(),
            &signature
        )
        .is_err());
        assert!(verify(&binding.public_key, &transcript, &[0; 63]).is_err());
        assert!(validate_public_key(&[0; 32]).is_err());
    }
}
