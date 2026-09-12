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
pub trait IdentityStore {
    fn create_signing_key(&mut self) -> Result<KeyHandle, CoreError>;
    fn public_key(&self, key: &KeyHandle) -> Result<Vec<u8>, CoreError>;
    fn sign(&self, key: &KeyHandle, domain_separated_message: &[u8]) -> Result<Vec<u8>, CoreError>;
    fn delete_key(&mut self, key: KeyHandle) -> Result<(), CoreError>;
}
