use crate::identity::IdentitySeed;
use crate::CoreError;
use chacha20poly1305::{
    aead::{Aead, KeyInit, Payload},
    ChaCha20Poly1305, Key, Nonce,
};
use hkdf::Hkdf;
use sha2::Sha256;
use uuid::Uuid;
use zeroize::Zeroizing;

const VERSION: u8 = 1;
const IDENTITY_SEED_KIND: u8 = 1;
const BACKUP_ID_BYTES: usize = 16;
const DEVICE_ID_BYTES: usize = 16;
const CREDENTIAL_ID_LENGTH_BYTES: usize = 2;
const PRF_SALT_BYTES: usize = 32;
const NONCE_BYTES: usize = 12;
const SEED_BYTES: usize = 32;
const TAG_BYTES: usize = 16;
const MIN_CREDENTIAL_ID_BYTES: usize = 1;
const MAX_CREDENTIAL_ID_BYTES: usize = 1024;
const HEADER_BYTES: usize =
    1 + 1 + BACKUP_ID_BYTES + DEVICE_ID_BYTES + CREDENTIAL_ID_LENGTH_BYTES + PRF_SALT_BYTES;
const KDF_SALT: &[u8] = b"links/passkey-backup/v1/salt\0";
const KDF_INFO: &[u8] = b"links/passkey-backup/v1/chacha20poly1305\0";

/// A WebAuthn PRF result supplied by the platform after user verification.
/// WebAuthn assertions do not export private keys; this PRF output is the only
/// passkey-derived secret accepted by the backup implementation.
pub struct PasskeyPrfKey(Zeroizing<[u8; 32]>);

impl PasskeyPrfKey {
    pub fn from_output(output: &[u8]) -> Result<Self, CoreError> {
        let bytes: [u8; 32] = output.try_into().map_err(|_| CoreError::Authentication)?;
        if bytes == [0; 32] {
            return Err(CoreError::Authentication);
        }
        Ok(Self(Zeroizing::new(bytes)))
    }
}

/// Public salt used as the input to the WebAuthn PRF extension. Store it with
/// the encrypted envelope; it is not a password or a secret.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct PasskeyBackupSalt([u8; PRF_SALT_BYTES]);

impl PasskeyBackupSalt {
    pub fn from_bytes(bytes: [u8; PRF_SALT_BYTES]) -> Self {
        Self(bytes)
    }

    pub fn generate() -> Result<Self, CoreError> {
        let mut salt = [0u8; PRF_SALT_BYTES];
        getrandom::fill(&mut salt).map_err(|_| CoreError::Provider)?;
        Ok(Self(salt))
    }

    pub fn as_bytes(&self) -> &[u8; PRF_SALT_BYTES] {
        &self.0
    }
}

/// Opaque, authenticated cloud-backup record. The only plaintext metadata is
/// the backup/device/credential binding and the PRF salt. The identity seed is
/// always inside the AEAD ciphertext.
pub struct PasskeyBackupEnvelope(Zeroizing<Vec<u8>>);

impl PasskeyBackupEnvelope {
    pub fn seal_identity_seed(
        backup_id: Uuid,
        device_id: Uuid,
        credential_id: &[u8],
        salt: PasskeyBackupSalt,
        prf_key: &PasskeyPrfKey,
        seed: &IdentitySeed,
    ) -> Result<Self, CoreError> {
        validate_ids(backup_id, device_id)?;
        validate_credential_id(credential_id)?;
        let header = header(backup_id, device_id, credential_id, salt);
        let key = encryption_key(prf_key, salt)?;
        let cipher = ChaCha20Poly1305::new(Key::from_slice(key.as_ref()));
        let mut nonce = [0u8; NONCE_BYTES];
        getrandom::fill(&mut nonce).map_err(|_| CoreError::Provider)?;
        let ciphertext = cipher
            .encrypt(
                Nonce::from_slice(&nonce),
                Payload {
                    msg: seed.expose_for_wrapping(),
                    aad: &header,
                },
            )
            .map_err(|_| CoreError::Provider)?;
        let mut bytes = header;
        bytes.extend_from_slice(&nonce);
        bytes.extend_from_slice(&ciphertext);
        Ok(Self(Zeroizing::new(bytes)))
    }

    pub fn open_identity_seed(
        &self,
        expected_backup_id: Uuid,
        expected_device_id: Uuid,
        expected_credential_id: &[u8],
        prf_key: &PasskeyPrfKey,
    ) -> Result<IdentitySeed, CoreError> {
        let parsed = self.parse()?;
        if parsed.backup_id != expected_backup_id
            || parsed.device_id != expected_device_id
            || parsed.credential_id != expected_credential_id
        {
            return Err(CoreError::Authentication);
        }
        let key = encryption_key(prf_key, PasskeyBackupSalt::from_bytes(parsed.salt))?;
        let cipher = ChaCha20Poly1305::new(Key::from_slice(key.as_ref()));
        let plaintext = cipher
            .decrypt(
                Nonce::from_slice(parsed.nonce),
                Payload {
                    msg: parsed.ciphertext,
                    aad: parsed.header,
                },
            )
            .map_err(|_| CoreError::Authentication)?;
        let seed: [u8; SEED_BYTES] = plaintext
            .as_slice()
            .try_into()
            .map_err(|_| CoreError::Authentication)?;
        Ok(IdentitySeed::from_vault(Zeroizing::new(seed)))
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    pub fn metadata(&self) -> Result<PasskeyBackupMetadata<'_>, CoreError> {
        let parsed = self.parse()?;
        Ok(PasskeyBackupMetadata {
            backup_id: parsed.backup_id,
            device_id: parsed.device_id,
            credential_id: parsed.credential_id,
            salt: parsed.salt,
        })
    }

    fn parse(&self) -> Result<ParsedEnvelope<'_>, CoreError> {
        if self.0.len() < HEADER_BYTES + NONCE_BYTES + SEED_BYTES + TAG_BYTES
            || self.0[0] != VERSION
            || self.0[1] != IDENTITY_SEED_KIND
        {
            return Err(CoreError::Authentication);
        }
        let backup_id = Uuid::from_slice(&self.0[2..18]).map_err(|_| CoreError::Authentication)?;
        let device_id = Uuid::from_slice(&self.0[18..34]).map_err(|_| CoreError::Authentication)?;
        let credential_len = u16::from_be_bytes([self.0[34], self.0[35]]) as usize;
        if !(MIN_CREDENTIAL_ID_BYTES..=MAX_CREDENTIAL_ID_BYTES).contains(&credential_len) {
            return Err(CoreError::Authentication);
        }
        let credential_end = HEADER_BYTES - PRF_SALT_BYTES + credential_len;
        let salt_start = credential_end;
        let salt_end = salt_start + PRF_SALT_BYTES;
        let nonce_end = salt_end + NONCE_BYTES;
        if nonce_end + SEED_BYTES + TAG_BYTES != self.0.len() {
            return Err(CoreError::Authentication);
        }
        let credential_id = &self.0[HEADER_BYTES - PRF_SALT_BYTES..salt_start];
        let salt: [u8; PRF_SALT_BYTES] = self.0[salt_start..salt_end]
            .try_into()
            .map_err(|_| CoreError::Authentication)?;
        Ok(ParsedEnvelope {
            backup_id,
            device_id,
            credential_id,
            salt,
            header: &self.0[..salt_end],
            nonce: &self.0[salt_end..nonce_end],
            ciphertext: &self.0[nonce_end..],
        })
    }
}

pub struct PasskeyBackupMetadata<'a> {
    pub backup_id: Uuid,
    pub device_id: Uuid,
    pub credential_id: &'a [u8],
    pub salt: [u8; PRF_SALT_BYTES],
}

struct ParsedEnvelope<'a> {
    backup_id: Uuid,
    device_id: Uuid,
    credential_id: &'a [u8],
    salt: [u8; PRF_SALT_BYTES],
    header: &'a [u8],
    nonce: &'a [u8],
    ciphertext: &'a [u8],
}

fn validate_ids(backup_id: Uuid, device_id: Uuid) -> Result<(), CoreError> {
    if backup_id.is_nil() || device_id.is_nil() {
        return Err(CoreError::Authentication);
    }
    Ok(())
}

fn validate_credential_id(credential_id: &[u8]) -> Result<(), CoreError> {
    if !(MIN_CREDENTIAL_ID_BYTES..=MAX_CREDENTIAL_ID_BYTES).contains(&credential_id.len()) {
        return Err(CoreError::Authentication);
    }
    Ok(())
}

fn header(
    backup_id: Uuid,
    device_id: Uuid,
    credential_id: &[u8],
    salt: PasskeyBackupSalt,
) -> Vec<u8> {
    let mut header = Vec::with_capacity(HEADER_BYTES + credential_id.len());
    header.push(VERSION);
    header.push(IDENTITY_SEED_KIND);
    header.extend_from_slice(backup_id.as_bytes());
    header.extend_from_slice(device_id.as_bytes());
    header.extend_from_slice(&(credential_id.len() as u16).to_be_bytes());
    header.extend_from_slice(credential_id);
    header.extend_from_slice(salt.as_bytes());
    header
}

fn encryption_key(
    prf_key: &PasskeyPrfKey,
    salt: PasskeyBackupSalt,
) -> Result<Zeroizing<[u8; 32]>, CoreError> {
    let hkdf = Hkdf::<Sha256>::new(Some(KDF_SALT), prf_key.0.as_ref());
    let mut info = Vec::with_capacity(KDF_INFO.len() + PRF_SALT_BYTES);
    info.extend_from_slice(KDF_INFO);
    info.extend_from_slice(salt.as_bytes());
    let mut key = Zeroizing::new([0u8; 32]);
    hkdf.expand(&info, key.as_mut())
        .map_err(|_| CoreError::Provider)?;
    Ok(key)
}

impl TryFrom<Vec<u8>> for PasskeyBackupEnvelope {
    type Error = CoreError;

    fn try_from(bytes: Vec<u8>) -> Result<Self, Self::Error> {
        let envelope = Self(Zeroizing::new(bytes));
        envelope.parse()?;
        Ok(envelope)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backup_round_trip_binds_all_public_context() {
        let seed = IdentitySeed::from_vault(Zeroizing::new([7; 32]));
        let key = PasskeyPrfKey::from_output(&[9; 32]).unwrap();
        let salt = PasskeyBackupSalt([3; 32]);
        let backup_id = Uuid::from_u128(1);
        let device_id = Uuid::from_u128(2);
        let credential_id = b"credential";
        let envelope = PasskeyBackupEnvelope::seal_identity_seed(
            backup_id,
            device_id,
            credential_id,
            salt,
            &key,
            &seed,
        )
        .unwrap();
        let restored = envelope
            .open_identity_seed(backup_id, device_id, credential_id, &key)
            .unwrap();
        assert_eq!(restored.public_key(), seed.public_key());
        assert!(envelope
            .open_identity_seed(backup_id, Uuid::from_u128(3), credential_id, &key)
            .is_err());
    }

    #[test]
    fn malformed_or_wrong_prf_records_fail_closed() {
        assert!(PasskeyBackupEnvelope::try_from(vec![0; 128]).is_err());
        assert!(PasskeyPrfKey::from_output(&[0; 31]).is_err());
        let seed = IdentitySeed::from_vault(Zeroizing::new([1; 32]));
        let salt = PasskeyBackupSalt([2; 32]);
        let envelope = PasskeyBackupEnvelope::seal_identity_seed(
            Uuid::from_u128(1),
            Uuid::from_u128(2),
            b"credential",
            salt,
            &PasskeyPrfKey::from_output(&[3; 32]).unwrap(),
            &seed,
        )
        .unwrap();
        assert!(envelope
            .open_identity_seed(
                Uuid::from_u128(1),
                Uuid::from_u128(2),
                b"credential",
                &PasskeyPrfKey::from_output(&[4; 32]).unwrap()
            )
            .is_err());
    }
}
