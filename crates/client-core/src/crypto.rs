use crate::{protocol, CoreError};
use chacha20poly1305::{
    aead::{Aead, KeyInit, Payload},
    ChaCha20Poly1305, Key, Nonce,
};
use hkdf::Hkdf;
use sha2::Sha256;
use x25519_dalek::{PublicKey, StaticSecret};
use zeroize::Zeroizing;

const SEALED_SENDER_VERSION: u8 = 1;
const SEALED_SENDER_EPHEMERAL_KEY_BYTES: usize = 32;
const SEALED_SENDER_NONCE_BYTES: usize = 12;
const SEALED_SENDER_TAG_BYTES: usize = 16;
const SEALED_SENDER_HEADER_BYTES: usize =
    1 + SEALED_SENDER_EPHEMERAL_KEY_BYTES + SEALED_SENDER_NONCE_BYTES;
const SEALED_SENDER_KDF_INFO: &[u8] = b"links/sealed-sender/key/v1\0";
const SEALED_SENDER_KDF_SALT: &[u8] = b"links/sealed-sender/salt/v1\0";

/// Secret buffers zeroize on drop; no Debug/Clone to reduce accidental exposure.
/// This cannot erase caller copies or guarantee hardware-backed memory.
pub struct SecretBytes(Zeroizing<Vec<u8>>);
impl SecretBytes {
    pub fn new(bytes: Vec<u8>) -> Self {
        Self(Zeroizing::new(bytes))
    }
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

/// A reviewed sender-hiding envelope adapter. The provider must authenticate
/// routing_context as associated data and resolve authenticated recipient keys.
/// No algorithm or PQ security claim is implied by implementing this trait.
pub trait EnvelopeCrypto {
    fn seal(
        &mut self,
        recipient_device_id: &str,
        routing_context: &[u8],
        mls_ciphertext: &[u8],
    ) -> Result<Vec<u8>, CoreError>;
    fn open(
        &mut self,
        recipient_device_id: &str,
        routing_context: &[u8],
        sealed_payload: &[u8],
    ) -> Result<SecretBytes, CoreError>;
}

/// Resolver for the recipient encryption key used by the Sealed Sender
/// wrapper. Public keys may come from the authenticated device directory.
/// Private keys must come from the platform vault and must never be exported
/// to the server.
pub trait SealedSenderKeyResolver {
    fn recipient_public_key(&self, recipient_device_id: &str) -> Result<[u8; 32], CoreError>;
    fn local_private_key(
        &self,
        recipient_device_id: &str,
    ) -> Result<Zeroizing<[u8; 32]>, CoreError>;
}

/// Mutable directory hook used by the send coordinator after it verifies a
/// recipient's signed PQXDH bundle. Implementations must replace keys only
/// through their authenticated device-directory policy.
pub trait RecipientKeyDirectory {
    fn install_recipient_public_key(
        &mut self,
        recipient_device_id: &str,
        public_key: [u8; 32],
    ) -> Result<(), CoreError>;
}

/// Sealed Sender envelope provider.
///
/// The wrapper carries only a version, an ephemeral X25519 public key, a nonce,
/// and AEAD ciphertext. MLS already authenticates the sender and conversation
/// inside that ciphertext. The routing header is authenticated as AEAD
/// associated data, but is not encrypted because the delivery service needs
/// the recipient device and expiry.
pub struct SealedSenderCrypto<R> {
    resolver: R,
}

impl<R> SealedSenderCrypto<R> {
    pub fn new(resolver: R) -> Self {
        Self { resolver }
    }

    pub fn resolver(&self) -> &R {
        &self.resolver
    }

    pub fn resolver_mut(&mut self) -> &mut R {
        &mut self.resolver
    }
}

impl<R> RecipientKeyDirectory for SealedSenderCrypto<R>
where
    R: RecipientKeyDirectory,
{
    fn install_recipient_public_key(
        &mut self,
        recipient_device_id: &str,
        public_key: [u8; 32],
    ) -> Result<(), CoreError> {
        self.resolver
            .install_recipient_public_key(recipient_device_id, public_key)
    }
}

impl<R: SealedSenderKeyResolver> EnvelopeCrypto for SealedSenderCrypto<R> {
    fn seal(
        &mut self,
        recipient_device_id: &str,
        routing_context: &[u8],
        mls_ciphertext: &[u8],
    ) -> Result<Vec<u8>, CoreError> {
        protocol::validate_id(recipient_device_id)?;
        let recipient_public = self.resolver.recipient_public_key(recipient_device_id)?;
        let ephemeral_seed = crate::pqxdh::generate_x25519_seed()?;
        let ephemeral_private = StaticSecret::from(*ephemeral_seed);
        let ephemeral_public = PublicKey::from(&ephemeral_private).to_bytes();
        let recipient_public = PublicKey::from(recipient_public);
        let shared = ephemeral_private.diffie_hellman(&recipient_public);
        if !shared.was_contributory() {
            return Err(CoreError::Authentication);
        }
        let key = sealed_sender_key(
            shared.as_bytes(),
            &ephemeral_public,
            recipient_public.as_bytes(),
        )?;
        let cipher = ChaCha20Poly1305::new(Key::from_slice(key.as_ref()));
        let mut nonce_bytes = [0u8; SEALED_SENDER_NONCE_BYTES];
        getrandom::fill(&mut nonce_bytes).map_err(|_| CoreError::Provider)?;
        let ciphertext = cipher
            .encrypt(
                Nonce::from_slice(&nonce_bytes),
                Payload {
                    msg: mls_ciphertext,
                    aad: routing_context,
                },
            )
            .map_err(|_| CoreError::Provider)?;
        let mut sealed = Vec::with_capacity(SEALED_SENDER_HEADER_BYTES + ciphertext.len());
        sealed.push(SEALED_SENDER_VERSION);
        sealed.extend_from_slice(&ephemeral_public);
        sealed.extend_from_slice(&nonce_bytes);
        sealed.extend_from_slice(&ciphertext);
        Ok(sealed)
    }

    fn open(
        &mut self,
        recipient_device_id: &str,
        routing_context: &[u8],
        sealed_payload: &[u8],
    ) -> Result<SecretBytes, CoreError> {
        protocol::validate_id(recipient_device_id)?;
        if sealed_payload.len() < SEALED_SENDER_HEADER_BYTES + SEALED_SENDER_TAG_BYTES
            || sealed_payload[0] != SEALED_SENDER_VERSION
        {
            return Err(CoreError::Authentication);
        }
        let ephemeral_start = 1;
        let ephemeral_end = ephemeral_start + SEALED_SENDER_EPHEMERAL_KEY_BYTES;
        let nonce_end = ephemeral_end + SEALED_SENDER_NONCE_BYTES;
        let ephemeral_public: [u8; 32] = sealed_payload[ephemeral_start..ephemeral_end]
            .try_into()
            .map_err(|_| CoreError::Authentication)?;
        let nonce = &sealed_payload[ephemeral_end..nonce_end];
        let private_seed = self.resolver.local_private_key(recipient_device_id)?;
        let private = StaticSecret::from(*private_seed);
        let recipient_public = PublicKey::from(&private).to_bytes();
        let shared = private.diffie_hellman(&PublicKey::from(ephemeral_public));
        if !shared.was_contributory() {
            return Err(CoreError::Authentication);
        }
        let key = sealed_sender_key(shared.as_bytes(), &ephemeral_public, &recipient_public)?;
        let cipher = ChaCha20Poly1305::new(Key::from_slice(key.as_ref()));
        let plaintext = cipher
            .decrypt(
                Nonce::from_slice(nonce),
                Payload {
                    msg: &sealed_payload[nonce_end..],
                    aad: routing_context,
                },
            )
            .map_err(|_| CoreError::Authentication)?;
        Ok(SecretBytes::new(plaintext))
    }
}

fn sealed_sender_key(
    shared_secret: &[u8; 32],
    ephemeral_public: &[u8; 32],
    recipient_public: &[u8; 32],
) -> Result<Zeroizing<[u8; 32]>, CoreError> {
    let mut info = SEALED_SENDER_KDF_INFO.to_vec();
    info.extend_from_slice(ephemeral_public);
    info.extend_from_slice(recipient_public);
    let hkdf = Hkdf::<Sha256>::new(Some(SEALED_SENDER_KDF_SALT), shared_secret);
    let mut key = Zeroizing::new([0u8; 32]);
    hkdf.expand(&info, key.as_mut())
        .map_err(|_| CoreError::Provider)?;
    Ok(key)
}

/// Explicit default for applications that have not installed an audited provider.
pub struct UnavailableCrypto;
impl EnvelopeCrypto for UnavailableCrypto {
    fn seal(&mut self, _: &str, _: &[u8], _: &[u8]) -> Result<Vec<u8>, CoreError> {
        Err(CoreError::CryptoUnavailable)
    }
    fn open(&mut self, _: &str, _: &[u8], _: &[u8]) -> Result<SecretBytes, CoreError> {
        Err(CoreError::CryptoUnavailable)
    }
}

impl RecipientKeyDirectory for UnavailableCrypto {
    fn install_recipient_public_key(
        &mut self,
        _: &str,
        _: [u8; 32],
    ) -> Result<(), CoreError> {
        Err(CoreError::CryptoUnavailable)
    }
}
