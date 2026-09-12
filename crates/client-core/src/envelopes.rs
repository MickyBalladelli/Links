use crate::{crypto::{EnvelopeCrypto, SecretBytes}, identity::LocalIdentity, mls::MlsEngine, protocol::{self, v1}, CoreError};
use prost::Message;

pub struct ClientCore<C, M> {
    identity: LocalIdentity,
    crypto: C,
    mls: M,
}
impl<C: EnvelopeCrypto, M: MlsEngine> ClientCore<C, M> {
    pub fn new(identity: LocalIdentity, crypto: C, mls: M) -> Self { Self { identity, crypto, mls } }

    /// Callers retain the returned bytes for retries: do not re-encrypt on retry.
    /// MLS mutations and the outbox must be persisted transactionally by the host.
    pub fn seal_message(&mut self, message: &v1::Message, recipient_device_id: String, envelope_id: String, expires_at_ms: u64, now_ms: u64) -> Result<v1::Envelope, CoreError> {
        protocol::validate_message(message)?;
        if message.sender_device_id != self.identity.device_id() { return Err(CoreError::Authentication); }
        let mut envelope = v1::Envelope { protocol_version: protocol::VERSION, envelope_id, recipient_device_id, expires_at_ms, sealed_payload: vec![1] };
        protocol::validate_enqueue(&envelope, now_ms)?;
        let plaintext = SecretBytes::new(message.encode_to_vec());
        let ciphertext = self.mls.encrypt(&message.conversation_id, &message.sender_device_id, plaintext.as_bytes())?;
        envelope.sealed_payload = self.crypto.seal(&envelope.recipient_device_id, &routing_context(&envelope), &ciphertext)?;
        protocol::validate_enqueue(&envelope, now_ms)?;
        Ok(envelope)
    }

    pub fn open_envelope(&mut self, envelope: &v1::Envelope, now_ms: u64) -> Result<v1::Message, CoreError> {
        protocol::validate_enqueue(envelope, now_ms)?;
        if envelope.recipient_device_id != self.identity.device_id() { return Err(CoreError::Authentication); }
        let ciphertext = self.crypto.open(&envelope.recipient_device_id, &routing_context(envelope), &envelope.sealed_payload)?;
        let application = self.mls.decrypt(ciphertext.as_bytes())?;
        let message = protocol::decode_message(application.plaintext.as_bytes())?;
        if application.conversation_id != message.conversation_id || application.sender_device_id != message.sender_device_id {
            return Err(CoreError::Authentication);
        }
        Ok(message)
    }
}

fn routing_context(envelope: &v1::Envelope) -> Vec<u8> {
    // Domain separated and deterministically encoded with no opaque ciphertext.
    let header = v1::Envelope { sealed_payload: vec![], ..envelope.clone() };
    let mut context = b"links/envelope/v1\0".to_vec();
    context.extend(header.encode_to_vec());
    context
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::UnavailableCrypto;
    const USER: &str = "00000000-0000-4000-8000-000000000001";
    const DEVICE: &str = "00000000-0000-4000-8000-000000000002";
    #[test]
    fn unavailable_crypto_never_emits_plaintext() {
        let mut core = ClientCore::new(LocalIdentity::new(USER.into(), DEVICE.into()).unwrap(), UnavailableCrypto, UnavailableCrypto);
        let message = v1::Message { message_id: USER.into(), conversation_id: USER.into(), sender_device_id: DEVICE.into(), sent_at_ms: 1, content: Some(v1::message::Content::Text("secret".into())) };
        assert!(matches!(core.seal_message(&message, DEVICE.into(), USER.into(), 1000, 1), Err(CoreError::CryptoUnavailable)));
    }
    #[test]
    fn routing_context_binds_header_not_ciphertext() {
        let a = v1::Envelope { protocol_version: 1, envelope_id: USER.into(), recipient_device_id: DEVICE.into(), expires_at_ms: 10, sealed_payload: vec![1] };
        let mut b = a.clone();
        b.sealed_payload = vec![2];
        assert_eq!(routing_context(&a), routing_context(&b));
        b.expires_at_ms = 11;
        assert_ne!(routing_context(&a), routing_context(&b));
    }
}
