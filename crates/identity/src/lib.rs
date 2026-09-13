//! Ed25519 identities and RFC 9420 basic credentials. A basic credential is an
//! assertion, not an OTP proof or a certificate; enrollment authenticates it.
use bip39::{Language, Mnemonic};
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use hkdf::Hkdf;
use sha2::Sha512;
use thiserror::Error;
use tls_codec::{Serialize, TlsSerialize, TlsSize, VLBytes};
use uuid::Uuid;
use zeroize::Zeroizing;

const PQXDH_EC_ENCODING_TAG: u8 = 1;
const PQXDH_KEM_ENCODING_TAG: u8 = 2;
const PQXDH_IDENTITY_BINDING_DOMAIN: &[u8] = b"links/pqxdh/identity-binding/v1\0";
const PQXDH_SIGNED_PREKEY_DOMAIN: &[u8] = b"links/pqxdh/signed-prekey/v1\0";
const PQXDH_KEM_PREKEY_DOMAIN: &[u8] = b"links/pqxdh/kem-prekey/v1\0";
const DEVICE_IDENTITY_DOMAIN: &[u8] = b"links/device/v1\0";
const RECOVERY_KDF_SALT: &[u8] = b"links/recovery/v1/salt\0";
const RECOVERY_IDENTITY_KDF_INFO: &[u8] = b"links/recovery/v1/ed25519-identity\0";
const PASSKEY_IDENTITY_KDF_SALT: &[u8] = b"links/passkey-identity/v1/salt\0";
const PASSKEY_IDENTITY_KDF_INFO: &[u8] = b"links/passkey-identity/v1/ed25519-identity\0";
const PASSKEY_IDENTITY_PRF_SALT: &[u8; 32] = b"links/passkey-identity/v1\0\0\0\0\0\0\0";

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

    /// Derive a stable identity from a WebAuthn PRF output. The PRF result is
    /// local-only and is immediately converted through a domain-separated KDF;
    /// callers must wipe their input after this call.
    pub fn from_passkey_prf(prf_output: &[u8]) -> Result<Self, IdentityError> {
        if prf_output.len() != 32 || prf_output.iter().all(|byte| *byte == 0) {
            return Err(IdentityError::Authentication);
        }
        let hkdf = Hkdf::<Sha512>::new(Some(PASSKEY_IDENTITY_KDF_SALT), prf_output);
        let mut identity = Zeroizing::new([0u8; 32]);
        hkdf.expand(PASSKEY_IDENTITY_KDF_INFO, identity.as_mut())
            .expect("fixed passkey identity output length is valid");
        Ok(Self::from_derived(identity))
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

    fn from_derived(bytes: Zeroizing<[u8; 32]>) -> Self {
        Self(bytes)
    }
}

/// Stable WebAuthn PRF salt for passkey-derived self-sovereign identities.
/// Backup envelopes intentionally use a different random salt.
pub fn passkey_identity_prf_salt() -> [u8; 32] {
    *PASSKEY_IDENTITY_PRF_SALT
}

/// A validated BIP-39 mnemonic held in zeroizing memory.
///
/// Only English 12- and 24-word phrases are accepted for this recovery profile.
/// The phrase must stay local; never send it to a server or analytics service.
pub struct RecoveryMnemonic(Zeroizing<String>);

impl RecoveryMnemonic {
    pub fn generate(word_count: usize) -> Result<Self, IdentityError> {
        let entropy_bytes = match word_count {
            12 => 16,
            24 => 32,
            _ => return Err(IdentityError::Invalid),
        };
        let mut entropy = Zeroizing::new(vec![0u8; entropy_bytes]);
        getrandom::fill(&mut entropy).map_err(|_| IdentityError::RandomUnavailable)?;
        let mnemonic = Mnemonic::from_entropy_in(Language::English, &entropy)
            .map_err(|_| IdentityError::Invalid)?;
        Ok(Self(Zeroizing::new(mnemonic.to_string())))
    }

    pub fn from_phrase(phrase: &str) -> Result<Self, IdentityError> {
        let mnemonic = Mnemonic::parse_in(Language::English, phrase)
            .map_err(|_| IdentityError::Authentication)?;
        if !matches!(mnemonic.word_count(), 12 | 24) {
            return Err(IdentityError::Invalid);
        }
        Ok(Self(Zeroizing::new(mnemonic.to_string())))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn word_count(&self) -> usize {
        self.0.split_whitespace().count()
    }

    pub fn to_seed(&self, passphrase: &str) -> Result<RecoverySeed, IdentityError> {
        derive_recovery_seed(self.as_str(), passphrase)
    }

    pub fn derive_identity_seed(&self, passphrase: &str) -> Result<IdentitySeed, IdentityError> {
        self.to_seed(passphrase)
            .map(|seed| seed.derive_identity_seed())
    }
}

/// The 512-bit BIP-39 PBKDF2-HMAC-SHA512 output. It remains in zeroizing
/// memory and is never persisted by this crate.
pub struct RecoverySeed(Zeroizing<[u8; 64]>);

impl RecoverySeed {
    pub fn as_bytes(&self) -> &[u8; 64] {
        &self.0
    }

    /// Derive the stable Links Ed25519 identity seed from the BIP-39 root.
    /// The extra domain-separated step prevents accidental cross-purpose key
    /// reuse with other keys derived from the same recovery phrase.
    pub fn derive_identity_seed(&self) -> IdentitySeed {
        let hkdf = Hkdf::<Sha512>::new(Some(RECOVERY_KDF_SALT), self.0.as_ref());
        let mut identity = Zeroizing::new([0u8; 32]);
        hkdf.expand(RECOVERY_IDENTITY_KDF_INFO, identity.as_mut())
            .expect("fixed recovery output length is valid");
        IdentitySeed::from_derived(identity)
    }
}

pub fn generate_recovery_mnemonic(word_count: usize) -> Result<RecoveryMnemonic, IdentityError> {
    RecoveryMnemonic::generate(word_count)
}

pub fn derive_recovery_seed(phrase: &str, passphrase: &str) -> Result<RecoverySeed, IdentityError> {
    let mnemonic =
        Mnemonic::parse_in(Language::English, phrase).map_err(|_| IdentityError::Authentication)?;
    if !matches!(mnemonic.word_count(), 12 | 24) {
        return Err(IdentityError::Invalid);
    }
    Ok(RecoverySeed(Zeroizing::new(mnemonic.to_seed(passphrase))))
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

/// Transcript signed by a new physical device when an authenticated account
/// device approves its enrollment. The nonce is supplied by the approving
/// device and must be freshly random for each pairing attempt.
pub fn device_pairing_transcript(
    user_id: Uuid,
    device_id: Uuid,
    mls_node_id: Uuid,
    public_key: &[u8; 32],
    nonce: &[u8; 32],
) -> Result<Vec<u8>, IdentityError> {
    if user_id.is_nil() || device_id.is_nil() || mls_node_id.is_nil() {
        return Err(IdentityError::Invalid);
    }
    validate_public_key(public_key)?;
    let mut bytes = b"links/device-pairing/v1\0".to_vec();
    bytes.extend(user_id.as_bytes());
    bytes.extend(device_id.as_bytes());
    bytes.extend(mls_node_id.as_bytes());
    bytes.extend(public_key);
    bytes.extend(nonce);
    Ok(bytes)
}

/// Proof transcript for creating a pseudonymous account or logging in with its
/// first-party device key. The handle is the canonical form without `@`.
pub fn username_registration_transcript(
    handle: &str,
    device_id: Uuid,
    mls_node_id: Uuid,
    public_key: &[u8; 32],
    nonce: &[u8; 32],
) -> Result<Vec<u8>, IdentityError> {
    username_transcript(
        b"links/username-register/v1\0",
        handle,
        device_id,
        mls_node_id,
        public_key,
        nonce,
    )
}

/// Proof transcript for returning to a pseudonymous account without a phone.
pub fn username_login_transcript(
    handle: &str,
    device_id: Uuid,
    mls_node_id: Uuid,
    public_key: &[u8; 32],
    nonce: &[u8; 32],
) -> Result<Vec<u8>, IdentityError> {
    username_transcript(
        b"links/username-login/v1\0",
        handle,
        device_id,
        mls_node_id,
        public_key,
        nonce,
    )
}

fn username_transcript(
    domain: &[u8],
    handle: &str,
    device_id: Uuid,
    mls_node_id: Uuid,
    public_key: &[u8; 32],
    nonce: &[u8; 32],
) -> Result<Vec<u8>, IdentityError> {
    links_protocol::validate_handle(handle).map_err(|_| IdentityError::Invalid)?;
    if device_id.is_nil() || mls_node_id.is_nil() {
        return Err(IdentityError::Invalid);
    }
    validate_public_key(public_key)?;
    let mut bytes = domain.to_vec();
    bytes.push(handle.len() as u8);
    bytes.extend(handle.as_bytes());
    bytes.extend(device_id.as_bytes());
    bytes.extend(mls_node_id.as_bytes());
    bytes.extend(public_key);
    bytes.extend(nonce);
    Ok(bytes)
}

#[derive(Clone, PartialEq, Eq)]
pub struct DeviceBinding {
    pub user_id: Uuid,
    pub device_id: Uuid,
    pub mls_node_id: Uuid,
    pub public_key: [u8; 32],
}

/// Parse the application identity carried inside an MLS BasicCredential.
/// Trust still comes from the caller's authenticated device directory.
pub fn parse_mls_basic_identity(identity: &[u8]) -> Result<DeviceBinding, IdentityError> {
    let bytes = identity
        .strip_prefix(DEVICE_IDENTITY_DOMAIN)
        .ok_or(IdentityError::Invalid)?;
    if bytes.len() != 16 + 16 + 16 + 32 {
        return Err(IdentityError::Invalid);
    }
    let user_id = Uuid::from_slice(&bytes[..16]).map_err(|_| IdentityError::Invalid)?;
    let device_id = Uuid::from_slice(&bytes[16..32]).map_err(|_| IdentityError::Invalid)?;
    let mls_node_id = Uuid::from_slice(&bytes[32..48]).map_err(|_| IdentityError::Invalid)?;
    let public_key: [u8; 32] = bytes[48..].try_into().map_err(|_| IdentityError::Invalid)?;
    if user_id.is_nil() || device_id.is_nil() || mls_node_id.is_nil() {
        return Err(IdentityError::Invalid);
    }
    validate_public_key(&public_key)?;
    Ok(DeviceBinding {
        user_id,
        device_id,
        mls_node_id,
        public_key,
    })
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
        let mut identity = DEVICE_IDENTITY_DOMAIN.to_vec();
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

    #[test]
    fn bip39_recovery_matches_standard_vector_and_derives_stably() {
        let phrase =
            "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
        let mnemonic = RecoveryMnemonic::from_phrase(phrase).unwrap();
        assert_eq!(mnemonic.word_count(), 12);
        let seed = mnemonic.to_seed("TREZOR").unwrap();
        assert_eq!(
            seed.as_bytes(),
            &decode_hex::<64>("c55257c360c07c72029aebc1b53c05ed0362ada38ead3e3e9efa3708e53495531f09a6987599d18264c1e1c92f2cf141630c7a3c4ab7c81b2f001698e7463b04")
        );
        let first = seed.derive_identity_seed().public_key();
        let second = RecoveryMnemonic::from_phrase(phrase)
            .unwrap()
            .derive_identity_seed("TREZOR")
            .unwrap()
            .public_key();
        assert_eq!(first, second);
        assert!(RecoveryMnemonic::generate(15).is_err());
    }

    #[test]
    fn passkey_identity_derivation_is_stable_and_domain_separated() {
        let first = IdentitySeed::from_passkey_prf(&[7; 32]).unwrap();
        let second = IdentitySeed::from_passkey_prf(&[7; 32]).unwrap();
        let different = IdentitySeed::from_passkey_prf(&[8; 32]).unwrap();
        assert_eq!(first.public_key(), second.public_key());
        assert_ne!(first.public_key(), different.public_key());
        assert_eq!(passkey_identity_prf_salt().len(), 32);
        assert!(IdentitySeed::from_passkey_prf(&[0; 32]).is_err());
        assert!(IdentitySeed::from_passkey_prf(&[0; 31]).is_err());
    }
}
