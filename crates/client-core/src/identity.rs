use crate::{protocol::validate_id, CoreError};
use zeroize::Zeroizing;

/// An opaque platform-keystore reference, never raw private key material.
/// Intentionally neither Clone nor Debug.
pub struct KeyHandle(Zeroizing<Vec<u8>>);
impl KeyHandle {
    pub fn new(reference: Vec<u8>) -> Result<Self, CoreError> {
        if reference.is_empty() {
            return Err(CoreError::Authentication);
        }
        Ok(Self(Zeroizing::new(reference)))
    }
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalIdentity {
    user_id: String,
    device_id: String,
}
impl LocalIdentity {
    pub fn new(user_id: String, device_id: String) -> Result<Self, CoreError> {
        validate_id(&user_id)?;
        validate_id(&device_id)?;
        Ok(Self { user_id, device_id })
    }
    pub fn user_id(&self) -> &str {
        &self.user_id
    }
    pub fn device_id(&self) -> &str {
        &self.device_id
    }
}

/// Platform adapters generate keys from a CSPRNG, not a phone number/OTP.
/// Public keys and MLS credentials are cryptographically bound during enrollment.
pub use links_identity::{
    derive_recovery_seed, generate_recovery_mnemonic, DeviceBinding, IdentitySeed,
    RecoveryMnemonic, RecoverySeed,
};

/// Native adapters must reject software-only wrapping keys. Ed25519 signing runs
/// in process memory after hardware unwrap; do not claim the seed never leaves TEE.
/// Apple/Android implementations live in native/ and are wired by links-identity-ffi.
/// No software fallback is supplied by this crate.
pub trait HardwareSeedVault {
    fn store_seed(&mut self, seed: &[u8; 32]) -> Result<KeyHandle, CoreError>;
    fn load_seed(&self, handle: &KeyHandle) -> Result<Zeroizing<[u8; 32]>, CoreError>;
    fn delete_seed(&mut self, handle: &KeyHandle) -> Result<(), CoreError>;
}

pub struct HardwareIdentityStore<V> {
    vault: V,
}
impl<V: HardwareSeedVault> HardwareIdentityStore<V> {
    pub fn new(vault: V) -> Self {
        Self { vault }
    }

    /// Derive a recovery identity locally and immediately store it in the
    /// platform vault. Use only from an explicit authenticated recovery flow.
    pub fn restore_from_recovery(
        &mut self,
        mnemonic: &RecoveryMnemonic,
        passphrase: &str,
    ) -> Result<KeyHandle, CoreError> {
        let seed = mnemonic
            .derive_identity_seed(passphrase)
            .map_err(|_| CoreError::Authentication)?;
        self.vault.store_seed(seed.expose_for_wrapping())
    }

    /// Check the persisted enrollment key against the same seed used for signing.
    /// Never sign under a substituted handle or silently enroll a replacement key.
    pub fn sign_checked(
        &self,
        key: &KeyHandle,
        expected_public_key: &[u8; 32],
        domain_separated_message: &[u8],
    ) -> Result<[u8; 64], CoreError> {
        let seed = IdentitySeed::from_vault(self.vault.load_seed(key)?);
        if seed.public_key() != *expected_public_key {
            return Err(CoreError::Authentication);
        }
        Ok(seed.sign(domain_separated_message))
    }
}
impl<V: HardwareSeedVault> IdentityStore for HardwareIdentityStore<V> {
    fn create_signing_key(&mut self) -> Result<KeyHandle, CoreError> {
        let seed = IdentitySeed::generate().map_err(|_| CoreError::Provider)?;
        self.vault.store_seed(seed.expose_for_wrapping())
    }
    fn public_key(&self, key: &KeyHandle) -> Result<Vec<u8>, CoreError> {
        Ok(IdentitySeed::from_vault(self.vault.load_seed(key)?)
            .public_key()
            .to_vec())
    }
    fn sign(&self, key: &KeyHandle, domain_separated_message: &[u8]) -> Result<Vec<u8>, CoreError> {
        Ok(IdentitySeed::from_vault(self.vault.load_seed(key)?)
            .sign(domain_separated_message)
            .to_vec())
    }
    fn delete_key(&mut self, key: KeyHandle) -> Result<(), CoreError> {
        self.vault.delete_seed(&key)
    }
}

pub trait IdentityStore {
    fn create_signing_key(&mut self) -> Result<KeyHandle, CoreError>;
    fn public_key(&self, key: &KeyHandle) -> Result<Vec<u8>, CoreError>;
    fn sign(&self, key: &KeyHandle, domain_separated_message: &[u8]) -> Result<Vec<u8>, CoreError>;
    fn delete_key(&mut self, key: KeyHandle) -> Result<(), CoreError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    // Test-only adapter. Never available to a production build.
    #[derive(Default)]
    struct FixtureVault {
        seed: Option<Zeroizing<[u8; 32]>>,
        unavailable: bool,
    }
    impl HardwareSeedVault for FixtureVault {
        fn store_seed(&mut self, seed: &[u8; 32]) -> Result<KeyHandle, CoreError> {
            if self.unavailable {
                return Err(CoreError::CryptoUnavailable);
            }
            self.seed = Some(Zeroizing::new(*seed));
            KeyHandle::new(b"test-handle".to_vec())
        }
        fn load_seed(&self, _: &KeyHandle) -> Result<Zeroizing<[u8; 32]>, CoreError> {
            self.seed
                .as_ref()
                .map(|seed| Zeroizing::new(**seed))
                .ok_or(CoreError::Authentication)
        }
        fn delete_seed(&mut self, _: &KeyHandle) -> Result<(), CoreError> {
            self.seed = None;
            Ok(())
        }
    }
    #[test]
    fn vault_backed_signing_and_deletion() {
        let mut store = HardwareIdentityStore::new(FixtureVault::default());
        let handle = store.create_signing_key().unwrap();
        let public_key: [u8; 32] = store.public_key(&handle).unwrap().try_into().unwrap();
        let signature = store.sign(&handle, b"links/test/v1").unwrap();
        links_identity::verify(&public_key, b"links/test/v1", &signature).unwrap();
        store.delete_key(handle).unwrap();
        assert!(store
            .public_key(&KeyHandle::new(b"test-handle".to_vec()).unwrap())
            .is_err());
    }
    #[test]
    fn unavailable_vault_has_no_software_fallback() {
        let mut store = HardwareIdentityStore::new(FixtureVault {
            unavailable: true,
            ..Default::default()
        });
        assert!(matches!(
            store.create_signing_key(),
            Err(CoreError::CryptoUnavailable)
        ));
    }
}
