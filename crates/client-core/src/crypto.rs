use crate::CoreError;
use zeroize::Zeroizing;

/// Secret buffers zeroize on drop; no Debug/Clone to reduce accidental exposure.
/// This cannot erase caller copies or guarantee hardware-backed memory.
pub struct SecretBytes(Zeroizing<Vec<u8>>);
impl SecretBytes {
    pub fn new(bytes: Vec<u8>) -> Self { Self(Zeroizing::new(bytes)) }
    pub fn as_bytes(&self) -> &[u8] { &self.0 }
}

/// A reviewed sender-hiding envelope adapter. The provider must authenticate
/// routing_context as associated data and resolve authenticated recipient keys.
/// No algorithm or PQ security claim is implied by implementing this trait.
pub trait EnvelopeCrypto {
    fn seal(&mut self, recipient_device_id: &str, routing_context: &[u8], mls_ciphertext: &[u8]) -> Result<Vec<u8>, CoreError>;
    fn open(&mut self, recipient_device_id: &str, routing_context: &[u8], sealed_payload: &[u8]) -> Result<SecretBytes, CoreError>;
}

/// Explicit default for applications that have not installed an audited provider.
pub struct UnavailableCrypto;
impl EnvelopeCrypto for UnavailableCrypto {
    fn seal(&mut self, _: &str, _: &[u8], _: &[u8]) -> Result<Vec<u8>, CoreError> { Err(CoreError::CryptoUnavailable) }
    fn open(&mut self, _: &str, _: &[u8], _: &[u8]) -> Result<SecretBytes, CoreError> { Err(CoreError::CryptoUnavailable) }
}
