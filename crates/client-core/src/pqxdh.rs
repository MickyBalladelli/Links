//! Links PQXDH profile: X25519 identity/prekeys, ML-KEM-768, SHA-512 and HKDF.
//!
//! Account authentication remains Ed25519. Each X25519 identity key is bound to
//! that account key with a signature so signing and DH never reuse one secret.
//! Callers must keep private seeds in platform hardware-backed storage.

use crate::{crypto::SecretBytes, CoreError};
use hkdf::Hkdf;
use ml_kem::{
    kem::{Ciphertext, Decapsulate, Key, KeyExport},
    ml_kem_768::{DecapsulationKey, EncapsulationKey},
    B32, MlKem768, Seed,
};
use sha2::Sha512;
use x25519_dalek::{PublicKey, StaticSecret};
use zeroize::{Zeroize, Zeroizing};

pub const ML_KEM_768_PUBLIC_KEY_BYTES: usize = 1184;
pub const ML_KEM_768_CIPHERTEXT_BYTES: usize = 1088;
pub const ML_KEM_768_SEED_BYTES: usize = 64;
pub const SESSION_KEY_BYTES: usize = 32;

const EC_ENCODING_TAG: u8 = 1;
const KEM_ENCODING_TAG: u8 = 2;
const KDF_INFO: &[u8] = b"LinksV1_X25519_SHA-512_ML-KEM-768";
const IDENTITY_BINDING_DOMAIN: &[u8] = b"links/pqxdh/identity-binding/v1\0";
const SIGNED_PREKEY_DOMAIN: &[u8] = b"links/pqxdh/signed-prekey/v1\0";
const KEM_PREKEY_DOMAIN: &[u8] = b"links/pqxdh/kem-prekey/v1\0";
const ASSOCIATED_DATA_DOMAIN: &[u8] = b"links/pqxdh/ad/v1\0";

/// Generate X25519 private material. Store it in a hardware-backed vault before
/// constructing a long-lived identity or prekey. The returned buffer wipes on drop.
pub fn generate_x25519_seed() -> Result<Zeroizing<[u8; 32]>, CoreError> {
    let mut seed = Zeroizing::new([0; 32]);
    getrandom::fill(seed.as_mut()).map_err(|_| CoreError::Provider)?;
    Ok(seed)
}

/// Generate the compact FIPS 203 seed for an ML-KEM-768 decapsulation key.
/// The returned buffer is secret and wipes on drop.
pub fn generate_ml_kem_768_seed() -> Result<Zeroizing<[u8; 64]>, CoreError> {
    let mut seed = Zeroizing::new([0; 64]);
    getrandom::fill(seed.as_mut()).map_err(|_| CoreError::Provider)?;
    Ok(seed)
}

/// Long-lived X25519 identity DH key. It is separate from the Ed25519 account key.
pub struct IdentityPrivateKey(StaticSecret);

impl IdentityPrivateKey {
    pub fn from_seed(seed: Zeroizing<[u8; 32]>) -> Self {
        Self(StaticSecret::from(*seed))
    }

    pub fn public_key(&self) -> [u8; 32] {
        PublicKey::from(&self.0).to_bytes()
    }
}

/// Public identity authenticated by the existing hardware-backed Ed25519 key.
#[derive(Clone, PartialEq, Eq)]
pub struct PublicIdentity {
    pub signing_key: [u8; 32],
    pub dh_key: [u8; 32],
    pub binding_signature: [u8; 64],
}

impl PublicIdentity {
    pub fn new(
        signing_key: [u8; 32],
        dh_key: [u8; 32],
        binding_signature: [u8; 64],
    ) -> Result<Self, CoreError> {
        let identity = Self {
            signing_key,
            dh_key,
            binding_signature,
        };
        identity.verify_for(&signing_key)?;
        Ok(identity)
    }

    /// The expected key must come from authenticated account/device metadata.
    pub fn verify_for(&self, expected_signing_key: &[u8; 32]) -> Result<(), CoreError> {
        if &self.signing_key != expected_signing_key || !valid_public_key(&self.dh_key) {
            return Err(CoreError::Authentication);
        }
        links_identity::verify(
            &self.signing_key,
            &identity_binding_transcript(&self.dh_key),
            &self.binding_signature,
        )
        .map_err(|_| CoreError::Authentication)
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct PublicCurvePreKey {
    pub id: u64,
    pub key: [u8; 32],
}

/// Private signed or one-time X25519 prekey.
pub struct CurvePreKey {
    id: u64,
    secret: StaticSecret,
}

impl CurvePreKey {
    pub fn from_seed(id: u64, seed: Zeroizing<[u8; 32]>) -> Result<Self, CoreError> {
        if id == 0 {
            return Err(CoreError::Authentication);
        }
        Ok(Self {
            id,
            secret: StaticSecret::from(*seed),
        })
    }

    pub fn public(&self) -> PublicCurvePreKey {
        PublicCurvePreKey {
            id: self.id,
            key: PublicKey::from(&self.secret).to_bytes(),
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct PublicKemPreKey {
    pub id: u64,
    pub key: Vec<u8>,
    pub one_time: bool,
}

/// ML-KEM-768 decapsulation prekey reconstructed from its compact 64-byte seed.
pub struct KemPreKey {
    id: u64,
    key: DecapsulationKey,
    one_time: bool,
}

impl KemPreKey {
    pub fn from_seed(
        id: u64,
        one_time: bool,
        mut seed: Zeroizing<[u8; 64]>,
    ) -> Result<Self, CoreError> {
        if id == 0 {
            return Err(CoreError::Authentication);
        }
        let encoded = Seed::try_from(seed.as_slice()).map_err(|_| CoreError::Provider)?;
        let key = DecapsulationKey::from_seed(encoded);
        seed.zeroize();
        Ok(Self { id, key, one_time })
    }

    pub fn public(&self) -> PublicKemPreKey {
        PublicKemPreKey {
            id: self.id,
            key: self.key.encapsulation_key().to_bytes().to_vec(),
            one_time: self.one_time,
        }
    }
}

/// Public keys fetched atomically from the prekey service.
#[derive(Clone, PartialEq, Eq)]
pub struct PreKeyBundle {
    pub identity: PublicIdentity,
    pub signed_prekey: PublicCurvePreKey,
    pub signed_prekey_signature: [u8; 64],
    pub one_time_prekey: Option<PublicCurvePreKey>,
    pub kem_prekey: PublicKemPreKey,
    pub kem_prekey_signature: [u8; 64],
}

impl PreKeyBundle {
    pub fn verify_for(&self, expected_signing_key: &[u8; 32]) -> Result<(), CoreError> {
        self.identity.verify_for(expected_signing_key)?;
        if self.signed_prekey.id == 0
            || !valid_public_key(&self.signed_prekey.key)
            || self
                .one_time_prekey
                .as_ref()
                .is_some_and(|key| {
                    key.id == 0
                        || key.id == self.signed_prekey.id
                        || !valid_public_key(&key.key)
                })
            || self.kem_prekey.id == 0
        {
            return Err(CoreError::Authentication);
        }
        decode_kem_key(&self.kem_prekey.key)?;
        links_identity::verify(
            &self.identity.signing_key,
            &signed_prekey_transcript(&self.identity.dh_key, &self.signed_prekey),
            &self.signed_prekey_signature,
        )
        .map_err(|_| CoreError::Authentication)?;
        links_identity::verify(
            &self.identity.signing_key,
            &kem_prekey_transcript(&self.identity.dh_key, &self.kem_prekey),
            &self.kem_prekey_signature,
        )
        .map_err(|_| CoreError::Authentication)
    }
}

/// Private responder material for one selected public bundle. The caller chooses
/// historical keys by ID before invoking `respond`.
pub struct ResponderPrivateKeys {
    pub identity: IdentityPrivateKey,
    pub signed_prekey: CurvePreKey,
    pub one_time_prekey: Option<CurvePreKey>,
    pub kem_prekey: KemPreKey,
}

impl ResponderPrivateKeys {
    pub fn public_bundle(
        &self,
        identity: PublicIdentity,
        signed_prekey_signature: [u8; 64],
        kem_prekey_signature: [u8; 64],
    ) -> Result<PreKeyBundle, CoreError> {
        if identity.dh_key != self.identity.public_key() {
            return Err(CoreError::Authentication);
        }
        let bundle = PreKeyBundle {
            identity,
            signed_prekey: self.signed_prekey.public(),
            signed_prekey_signature,
            one_time_prekey: self.one_time_prekey.as_ref().map(CurvePreKey::public),
            kem_prekey: self.kem_prekey.public(),
            kem_prekey_signature,
        };
        bundle.verify_for(&bundle.identity.signing_key)?;
        Ok(bundle)
    }
}

/// Unambiguous public PQXDH header. Its authenticated initial ciphertext is
/// supplied by the later MLS/envelope layer using `SessionSecrets::associated_data`.
#[derive(Clone, PartialEq, Eq)]
pub struct InitialMessage {
    pub initiator_identity: PublicIdentity,
    pub ephemeral_key: [u8; 32],
    pub kem_ciphertext: Vec<u8>,
    pub responder_signed_prekey_id: u64,
    pub responder_one_time_prekey_id: Option<u64>,
    pub responder_kem_prekey_id: u64,
    pub responder_kem_prekey_one_time: bool,
}

pub struct SessionSecrets {
    shared_secret: SecretBytes,
    associated_data: Vec<u8>,
}

impl SessionSecrets {
    pub fn shared_secret(&self) -> &[u8] {
        self.shared_secret.as_bytes()
    }

    pub fn associated_data(&self) -> &[u8] {
        &self.associated_data
    }
}

pub struct InitiatorHandshake {
    pub message: InitialMessage,
    pub secrets: SessionSecrets,
}

pub struct ResponderHandshake {
    pub secrets: SessionSecrets,
    /// Delete these only after the initial AEAD/MLS payload authenticates.
    pub used_one_time_curve_prekey_id: Option<u64>,
    pub used_one_time_kem_prekey_id: Option<u64>,
}

/// Verify the responder bundle, perform the three/four X25519 operations and
/// encapsulate to its ML-KEM-768 key.
pub fn initiate(
    initiator_private: &IdentityPrivateKey,
    initiator_public: &PublicIdentity,
    expected_responder_signing_key: &[u8; 32],
    bundle: &PreKeyBundle,
) -> Result<InitiatorHandshake, CoreError> {
    initiator_public.verify_for(&initiator_public.signing_key)?;
    if initiator_public.dh_key != initiator_private.public_key() {
        return Err(CoreError::Authentication);
    }
    bundle.verify_for(expected_responder_signing_key)?;

    let ephemeral = IdentityPrivateKey::from_seed(generate_x25519_seed()?);
    let ephemeral_key = ephemeral.public_key();
    let responder_identity = PublicKey::from(bundle.identity.dh_key);
    let responder_signed = PublicKey::from(bundle.signed_prekey.key);
    let mut key_material = Zeroizing::new(Vec::with_capacity(160));
    append_dh(&mut key_material, &initiator_private.0, &responder_signed)?;
    append_dh(&mut key_material, &ephemeral.0, &responder_identity)?;
    append_dh(&mut key_material, &ephemeral.0, &responder_signed)?;
    if let Some(one_time) = &bundle.one_time_prekey {
        append_dh(
            &mut key_material,
            &ephemeral.0,
            &PublicKey::from(one_time.key),
        )?;
    }

    let kem_key = decode_kem_key(&bundle.kem_prekey.key)?;
    let mut randomness = B32::default();
    getrandom::fill(randomness.as_mut_slice()).map_err(|_| CoreError::Provider)?;
    // This is the fallible-randomness equivalent of Encapsulate::encapsulate.
    // `randomness` is exactly 32 uniformly random bytes from the OS CSPRNG.
    let (ciphertext, mut kem_secret) = kem_key.encapsulate_deterministic(&randomness);
    randomness.zeroize();
    key_material.extend_from_slice(kem_secret.as_slice());
    kem_secret.zeroize();

    let shared_secret = derive_key(&key_material)?;
    let associated_data = associated_data(initiator_public, &bundle.identity);
    Ok(InitiatorHandshake {
        message: InitialMessage {
            initiator_identity: initiator_public.clone(),
            ephemeral_key,
            kem_ciphertext: ciphertext.to_vec(),
            responder_signed_prekey_id: bundle.signed_prekey.id,
            responder_one_time_prekey_id: bundle.one_time_prekey.as_ref().map(|key| key.id),
            responder_kem_prekey_id: bundle.kem_prekey.id,
            responder_kem_prekey_one_time: bundle.kem_prekey.one_time,
        },
        secrets: SessionSecrets {
            shared_secret,
            associated_data,
        },
    })
}

/// Recompute the initiator's secret. ML-KEM uses implicit rejection, so callers
/// must authenticate the first AEAD/MLS payload before accepting this result.
pub fn respond(
    expected_initiator_signing_key: &[u8; 32],
    responder_public: &PublicIdentity,
    responder: &ResponderPrivateKeys,
    message: &InitialMessage,
) -> Result<ResponderHandshake, CoreError> {
    message
        .initiator_identity
        .verify_for(expected_initiator_signing_key)?;
    responder_public.verify_for(&responder_public.signing_key)?;
    if responder_public.dh_key != responder.identity.public_key()
        || message.responder_signed_prekey_id != responder.signed_prekey.id
        || message.responder_kem_prekey_id != responder.kem_prekey.id
        || message.responder_kem_prekey_one_time != responder.kem_prekey.one_time
        || !valid_public_key(&message.ephemeral_key)
    {
        return Err(CoreError::Authentication);
    }
    let selected_one_time = match (
        message.responder_one_time_prekey_id,
        responder.one_time_prekey.as_ref(),
    ) {
        (None, _) => None,
        (Some(id), Some(key)) if id == key.id => Some(key),
        _ => return Err(CoreError::Authentication),
    };

    let initiator_identity = PublicKey::from(message.initiator_identity.dh_key);
    let initiator_ephemeral = PublicKey::from(message.ephemeral_key);
    let mut key_material = Zeroizing::new(Vec::with_capacity(160));
    append_dh(
        &mut key_material,
        &responder.signed_prekey.secret,
        &initiator_identity,
    )?;
    append_dh(
        &mut key_material,
        &responder.identity.0,
        &initiator_ephemeral,
    )?;
    append_dh(
        &mut key_material,
        &responder.signed_prekey.secret,
        &initiator_ephemeral,
    )?;
    if let Some(one_time) = selected_one_time {
        append_dh(
            &mut key_material,
            &one_time.secret,
            &initiator_ephemeral,
        )?;
    }

    let ciphertext = Ciphertext::<MlKem768>::try_from(message.kem_ciphertext.as_slice())
        .map_err(|_| CoreError::Authentication)?;
    let mut kem_secret = responder.kem_prekey.key.decapsulate(&ciphertext);
    key_material.extend_from_slice(kem_secret.as_slice());
    kem_secret.zeroize();
    let shared_secret = derive_key(&key_material)?;

    Ok(ResponderHandshake {
        secrets: SessionSecrets {
            shared_secret,
            associated_data: associated_data(&message.initiator_identity, responder_public),
        },
        used_one_time_curve_prekey_id: selected_one_time.map(|key| key.id),
        used_one_time_kem_prekey_id: responder.kem_prekey.one_time.then_some(responder.kem_prekey.id),
    })
}

pub fn identity_binding_transcript(dh_key: &[u8; 32]) -> Vec<u8> {
    let mut transcript = IDENTITY_BINDING_DOMAIN.to_vec();
    transcript.extend_from_slice(&encode_ec(dh_key));
    transcript
}

pub fn signed_prekey_transcript(
    identity_dh_key: &[u8; 32],
    prekey: &PublicCurvePreKey,
) -> Vec<u8> {
    let mut transcript = SIGNED_PREKEY_DOMAIN.to_vec();
    transcript.extend_from_slice(&encode_ec(identity_dh_key));
    transcript.extend_from_slice(&prekey.id.to_be_bytes());
    transcript.extend_from_slice(&encode_ec(&prekey.key));
    transcript
}

pub fn kem_prekey_transcript(
    identity_dh_key: &[u8; 32],
    prekey: &PublicKemPreKey,
) -> Vec<u8> {
    let mut transcript = KEM_PREKEY_DOMAIN.to_vec();
    transcript.extend_from_slice(&encode_ec(identity_dh_key));
    transcript.extend_from_slice(&prekey.id.to_be_bytes());
    transcript.push(u8::from(prekey.one_time));
    transcript.extend_from_slice(&encode_kem(&prekey.key));
    transcript
}

fn append_dh(
    output: &mut Vec<u8>,
    secret: &StaticSecret,
    public: &PublicKey,
) -> Result<(), CoreError> {
    let shared = secret.diffie_hellman(public);
    if !shared.was_contributory() {
        return Err(CoreError::Authentication);
    }
    output.extend_from_slice(shared.as_bytes());
    Ok(())
}

fn derive_key(key_material: &[u8]) -> Result<SecretBytes, CoreError> {
    let mut input = Zeroizing::new(Vec::with_capacity(32 + key_material.len()));
    input.extend_from_slice(&[0xff; 32]);
    input.extend_from_slice(key_material);
    let salt = [0; 64];
    let hkdf = Hkdf::<Sha512>::new(Some(&salt), &input);
    let mut output = Zeroizing::new(vec![0; SESSION_KEY_BYTES]);
    hkdf.expand(KDF_INFO, output.as_mut_slice())
        .map_err(|_| CoreError::Provider)?;
    Ok(SecretBytes::new(output.to_vec()))
}

fn associated_data(initiator: &PublicIdentity, responder: &PublicIdentity) -> Vec<u8> {
    let mut output = ASSOCIATED_DATA_DOMAIN.to_vec();
    output.extend_from_slice(&encode_ec(&initiator.dh_key));
    output.extend_from_slice(&encode_ec(&responder.dh_key));
    output.extend_from_slice(&initiator.signing_key);
    output.extend_from_slice(&responder.signing_key);
    output
}

fn encode_ec(key: &[u8; 32]) -> [u8; 33] {
    let mut encoded = [0; 33];
    encoded[0] = EC_ENCODING_TAG;
    encoded[1..].copy_from_slice(key);
    encoded
}

fn encode_kem(key: &[u8]) -> Vec<u8> {
    let mut encoded = Vec::with_capacity(1 + key.len());
    encoded.push(KEM_ENCODING_TAG);
    encoded.extend_from_slice(key);
    encoded
}

fn valid_public_key(key: &[u8; 32]) -> bool {
    key.iter().any(|byte| *byte != 0)
}

fn decode_kem_key(bytes: &[u8]) -> Result<EncapsulationKey, CoreError> {
    let encoded = Key::<EncapsulationKey>::try_from(bytes).map_err(|_| CoreError::Authentication)?;
    EncapsulationKey::new(&encoded).map_err(|_| CoreError::Authentication)
}

#[cfg(test)]
mod tests {
    use super::*;
    use links_identity::IdentitySeed;

    fn authenticated_identity(
        signing_byte: u8,
        dh_byte: u8,
    ) -> (IdentityPrivateKey, PublicIdentity, IdentitySeed) {
        let signing = IdentitySeed::from_vault(Zeroizing::new([signing_byte; 32]));
        let private = IdentityPrivateKey::from_seed(Zeroizing::new([dh_byte; 32]));
        let dh_key = private.public_key();
        let identity = PublicIdentity::new(
            signing.public_key(),
            dh_key,
            signing.sign(&identity_binding_transcript(&dh_key)),
        )
        .unwrap();
        (private, identity, signing)
    }

    fn responder(one_time_curve: bool, one_time_kem: bool) -> (ResponderPrivateKeys, PublicIdentity) {
        let (identity_private, identity_public, signing) = authenticated_identity(17, 18);
        let signed_prekey = CurvePreKey::from_seed(11, Zeroizing::new([9; 32])).unwrap();
        let one_time_prekey = one_time_curve
            .then(|| CurvePreKey::from_seed(12, Zeroizing::new([10; 32])).unwrap());
        let kem_prekey =
            KemPreKey::from_seed(13, one_time_kem, Zeroizing::new([11; 64])).unwrap();
        let signed_public = signed_prekey.public();
        let kem_public = kem_prekey.public();
        let responder = ResponderPrivateKeys {
            identity: identity_private,
            signed_prekey,
            one_time_prekey,
            kem_prekey,
        };
        let bundle = responder
            .public_bundle(
                identity_public.clone(),
                signing.sign(&signed_prekey_transcript(&identity_public.dh_key, &signed_public)),
                signing.sign(&kem_prekey_transcript(&identity_public.dh_key, &kem_public)),
            )
            .unwrap();
        assert!(bundle.identity == identity_public);
        (responder, identity_public)
    }

    fn bundle(responder: &ResponderPrivateKeys, identity: &PublicIdentity) -> PreKeyBundle {
        let signing = IdentitySeed::from_vault(Zeroizing::new([17; 32]));
        let signed = responder.signed_prekey.public();
        let kem = responder.kem_prekey.public();
        responder
            .public_bundle(
                identity.clone(),
                signing.sign(&signed_prekey_transcript(&identity.dh_key, &signed)),
                signing.sign(&kem_prekey_transcript(&identity.dh_key, &kem)),
            )
            .unwrap()
    }

    #[test]
    fn both_sides_derive_the_same_hybrid_secret() {
        let (initiator_private, initiator_public, _) = authenticated_identity(7, 8);
        let (responder, responder_public) = responder(true, true);
        let responder_bundle = bundle(&responder, &responder_public);
        let initiated = initiate(
            &initiator_private,
            &initiator_public,
            &responder_public.signing_key,
            &responder_bundle,
        )
        .unwrap();
        let answered = respond(
            &initiator_public.signing_key,
            &responder_public,
            &responder,
            &initiated.message,
        )
        .unwrap();
        assert_eq!(
            initiated.secrets.shared_secret(),
            answered.secrets.shared_secret()
        );
        assert_eq!(
            initiated.secrets.associated_data(),
            answered.secrets.associated_data()
        );
        assert_eq!(answered.used_one_time_curve_prekey_id, Some(12));
        assert_eq!(answered.used_one_time_kem_prekey_id, Some(13));
    }

    #[test]
    fn invalid_bundle_signature_and_low_order_key_fail_closed() {
        let (initiator_private, initiator_public, _) = authenticated_identity(7, 8);
        let (responder, responder_public) = responder(false, false);
        let mut responder_bundle = bundle(&responder, &responder_public);
        responder_bundle.signed_prekey_signature[0] ^= 1;
        assert!(matches!(
            initiate(
                &initiator_private,
                &initiator_public,
                &responder_public.signing_key,
                &responder_bundle
            ),
            Err(CoreError::Authentication)
        ));
        responder_bundle = bundle(&responder, &responder_public);
        responder_bundle.signed_prekey.key = [0; 32];
        assert!(responder_bundle
            .verify_for(&responder_public.signing_key)
            .is_err());
    }

    #[test]
    fn substituted_identity_and_wrong_prekey_ids_are_rejected() {
        let (initiator_private, initiator_public, _) = authenticated_identity(7, 8);
        let (responder, responder_public) = responder(true, true);
        let responder_bundle = bundle(&responder, &responder_public);
        let mut initiated = initiate(
            &initiator_private,
            &initiator_public,
            &responder_public.signing_key,
            &responder_bundle,
        )
        .unwrap();
        initiated.message.responder_signed_prekey_id += 1;
        assert!(matches!(
            respond(
                &initiator_public.signing_key,
                &responder_public,
                &responder,
                &initiated.message
            ),
            Err(CoreError::Authentication)
        ));
        assert!(responder_bundle.verify_for(&[99; 32]).is_err());
    }
}
