use crate::{crypto::SecretBytes, CoreError};

/// Metadata must come from verified MLS authentication, never plaintext claims.
pub struct AuthenticatedApplication {
    pub conversation_id: String,
    pub sender_device_id: String,
    pub plaintext: SecretBytes,
}

/// Phase 1 integrates a reviewed MLS engine. Every physical device is a leaf;
/// a two-user chat may contain more than two leaves when devices are paired.
/// Providers serialize epoch updates, reject replay, verify membership and bind
/// conversation IDs to groups. Persist MLS state atomically with application state.
pub trait MlsEngine {
    fn create_group(&mut self, conversation_id: &str, credential: &[u8]) -> Result<(), CoreError>;
    fn join_group(&mut self, conversation_id: &str, welcome: &[u8]) -> Result<(), CoreError>;
    fn process_commit(&mut self, conversation_id: &str, commit: &[u8]) -> Result<(), CoreError>;
    fn encrypt(
        &mut self,
        conversation_id: &str,
        sender_device_id: &str,
        plaintext: &[u8],
    ) -> Result<Vec<u8>, CoreError>;
    fn decrypt(&mut self, ciphertext: &[u8]) -> Result<AuthenticatedApplication, CoreError>;
}

impl MlsEngine for crate::crypto::UnavailableCrypto {
    fn create_group(&mut self, _: &str, _: &[u8]) -> Result<(), CoreError> {
        Err(CoreError::CryptoUnavailable)
    }
    fn join_group(&mut self, _: &str, _: &[u8]) -> Result<(), CoreError> {
        Err(CoreError::CryptoUnavailable)
    }
    fn process_commit(&mut self, _: &str, _: &[u8]) -> Result<(), CoreError> {
        Err(CoreError::CryptoUnavailable)
    }
    fn encrypt(&mut self, _: &str, _: &str, _: &[u8]) -> Result<Vec<u8>, CoreError> {
        Err(CoreError::CryptoUnavailable)
    }
    fn decrypt(&mut self, _: &[u8]) -> Result<AuthenticatedApplication, CoreError> {
        Err(CoreError::CryptoUnavailable)
    }
}
