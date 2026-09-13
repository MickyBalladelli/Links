use crate::{
    crypto::{EnvelopeCrypto, SecretBytes},
    identity::LocalIdentity,
    mls::MlsEngine,
    protocol::{self, v1},
    sequences::ConversationSequence,
    CoreError,
};
use prost::Message;

pub struct ClientCore<C, M> {
    identity: LocalIdentity,
    crypto: C,
    mls: M,
}
impl<C: EnvelopeCrypto, M: MlsEngine> ClientCore<C, M> {
    pub fn new(identity: LocalIdentity, crypto: C, mls: M) -> Self {
        Self {
            identity,
            crypto,
            mls,
        }
    }

    /// Assign the next sender-local conversation sequence before MLS sealing.
    /// Persist the returned message, envelope, MLS state and sequence counter
    /// in one host transaction before retrying or reporting durable send.
    pub fn seal_next_message(
        &mut self,
        mut message: v1::Message,
        sequence: &mut ConversationSequence,
        recipient_device_id: String,
        envelope_id: String,
        expires_at_ms: u64,
        now_ms: u64,
    ) -> Result<(v1::Message, v1::Envelope), CoreError> {
        if message.conversation_id != sequence.conversation_id()
            || message.sender_device_id != sequence.sender_device_id()
            || message.sender_device_id != self.identity.device_id()
        {
            return Err(CoreError::Authentication);
        }
        message.sequence_id = sequence.reserve_next()?;
        let envelope = self.seal_message(
            &message,
            recipient_device_id,
            envelope_id,
            expires_at_ms,
            now_ms,
        )?;
        Ok((message, envelope))
    }

    /// Callers retain the returned bytes for retries: do not re-encrypt on retry.
    /// MLS mutations and the outbox must be persisted transactionally by the host.
    pub fn seal_message(
        &mut self,
        message: &v1::Message,
        recipient_device_id: String,
        envelope_id: String,
        expires_at_ms: u64,
        now_ms: u64,
    ) -> Result<v1::Envelope, CoreError> {
        protocol::validate_message(message)?;
        if message.sender_device_id != self.identity.device_id() {
            return Err(CoreError::Authentication);
        }
        let mut envelope = v1::Envelope {
            protocol_version: protocol::VERSION,
            envelope_id,
            recipient_device_id,
            expires_at_ms,
            sealed_payload: vec![1],
        };
        protocol::validate_enqueue(&envelope, now_ms)?;
        let plaintext = SecretBytes::new(message.encode_to_vec());
        let ciphertext = self.mls.encrypt(
            &message.conversation_id,
            &message.sender_device_id,
            plaintext.as_bytes(),
        )?;
        envelope.sealed_payload = self.crypto.seal(
            &envelope.recipient_device_id,
            &routing_context(&envelope),
            &ciphertext,
        )?;
        protocol::validate_enqueue(&envelope, now_ms)?;
        Ok(envelope)
    }

    pub fn open_envelope(
        &mut self,
        envelope: &v1::Envelope,
        now_ms: u64,
    ) -> Result<v1::Message, CoreError> {
        protocol::validate_enqueue(envelope, now_ms)?;
        if envelope.recipient_device_id != self.identity.device_id() {
            return Err(CoreError::Authentication);
        }
        let ciphertext = self.crypto.open(
            &envelope.recipient_device_id,
            &routing_context(envelope),
            &envelope.sealed_payload,
        )?;
        let application = self.mls.decrypt(ciphertext.as_bytes())?;
        let message = protocol::decode_message(application.plaintext.as_bytes())?;
        if application.conversation_id != message.conversation_id
            || application.sender_device_id != message.sender_device_id
        {
            return Err(CoreError::Authentication);
        }
        Ok(message)
    }
}

fn routing_context(envelope: &v1::Envelope) -> Vec<u8> {
    // Domain separated and deterministically encoded with no opaque ciphertext.
    let header = v1::Envelope {
        sealed_payload: vec![],
        ..envelope.clone()
    };
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
        let mut core = ClientCore::new(
            LocalIdentity::new(USER.into(), DEVICE.into()).unwrap(),
            UnavailableCrypto,
            UnavailableCrypto,
        );
        let message = v1::Message {
            message_id: USER.into(),
            conversation_id: USER.into(),
            sender_device_id: DEVICE.into(),
            sent_at_ms: 1,
            sequence_id: 1,
            content: Some(v1::message::Content::Text("secret".into())),
        };
        assert!(matches!(
            core.seal_message(&message, DEVICE.into(), USER.into(), 1000, 1),
            Err(CoreError::CryptoUnavailable)
        ));
    }
    // Test-only deterministic providers validate orchestration, not encryption.
    struct FixtureCrypto;
    impl EnvelopeCrypto for FixtureCrypto {
        fn seal(
            &mut self,
            _: &str,
            context: &[u8],
            ciphertext: &[u8],
        ) -> Result<Vec<u8>, CoreError> {
            assert!(context.starts_with(b"links/envelope/v1\0"));
            assert_eq!(ciphertext, b"opaque-mls");
            Ok(vec![42])
        }
        fn open(&mut self, _: &str, _: &[u8], sealed: &[u8]) -> Result<SecretBytes, CoreError> {
            if sealed != [42] {
                return Err(CoreError::Authentication);
            }
            Ok(SecretBytes::new(b"opaque-mls".to_vec()))
        }
    }
    struct FixtureMls {
        message: v1::Message,
        authenticated_sender: String,
        authenticated_group: String,
    }
    impl MlsEngine for FixtureMls {
        fn create_group(&mut self, _: &str, _: &[u8]) -> Result<(), CoreError> {
            Err(CoreError::CryptoUnavailable)
        }
        fn join_group(&mut self, _: &str, _: &[u8]) -> Result<(), CoreError> {
            Err(CoreError::CryptoUnavailable)
        }
        fn process_commit(&mut self, _: &str, _: &[u8]) -> Result<(), CoreError> {
            Err(CoreError::CryptoUnavailable)
        }
        fn encrypt(
            &mut self,
            group: &str,
            sender: &str,
            plaintext: &[u8],
        ) -> Result<Vec<u8>, CoreError> {
            assert_eq!(group, self.authenticated_group);
            assert_eq!(sender, self.authenticated_sender);
            assert_eq!(plaintext, self.message.encode_to_vec());
            Ok(b"opaque-mls".to_vec())
        }
        fn decrypt(
            &mut self,
            ciphertext: &[u8],
        ) -> Result<crate::mls::AuthenticatedApplication, CoreError> {
            assert_eq!(ciphertext, b"opaque-mls");
            Ok(crate::mls::AuthenticatedApplication {
                conversation_id: self.authenticated_group.clone(),
                sender_device_id: self.authenticated_sender.clone(),
                plaintext: SecretBytes::new(self.message.encode_to_vec()),
            })
        }
    }
    fn fixture_mls() -> FixtureMls {
        FixtureMls {
            message: v1::Message {
                message_id: USER.into(),
                conversation_id: USER.into(),
                sender_device_id: DEVICE.into(),
                sent_at_ms: 1,
                sequence_id: 1,
                content: Some(v1::message::Content::Text("private message".into())),
            },
            authenticated_sender: DEVICE.into(),
            authenticated_group: USER.into(),
        }
    }
    #[test]
    fn send_receive_orchestration_and_recipient_expiry_validation() {
        let fixture = fixture_mls();
        let message = fixture.message.clone();
        let mut core = ClientCore::new(
            LocalIdentity::new(USER.into(), DEVICE.into()).unwrap(),
            FixtureCrypto,
            fixture,
        );
        let envelope = core
            .seal_message(&message, DEVICE.into(), USER.into(), 1000, 1)
            .unwrap();
        assert_eq!(envelope.sealed_payload, vec![42]);
        assert!(core.open_envelope(&envelope, 1).unwrap() == message);
        assert!(core.open_envelope(&envelope, 1000).is_err());
        let mut wrong_recipient = envelope.clone();
        wrong_recipient.recipient_device_id = USER.into();
        assert!(matches!(
            core.open_envelope(&wrong_recipient, 1),
            Err(CoreError::Authentication)
        ));
        let mut tampered = envelope;
        tampered.sealed_payload = vec![43];
        assert!(matches!(
            core.open_envelope(&tampered, 1),
            Err(CoreError::Authentication)
        ));
    }
    #[test]
    fn authenticated_mls_identity_must_match_plaintext_claims() {
        for wrong_sender in [true, false] {
            let mut fixture = fixture_mls();
            if wrong_sender {
                fixture.authenticated_sender = USER.into();
            } else {
                fixture.authenticated_group = DEVICE.into();
            }
            let mut core = ClientCore::new(
                LocalIdentity::new(USER.into(), DEVICE.into()).unwrap(),
                FixtureCrypto,
                fixture,
            );
            let envelope = v1::Envelope {
                protocol_version: 1,
                envelope_id: USER.into(),
                recipient_device_id: DEVICE.into(),
                expires_at_ms: 1000,
                sealed_payload: vec![42],
            };
            assert!(matches!(
                core.open_envelope(&envelope, 1),
                Err(CoreError::Authentication)
            ));
        }
    }
    #[test]
    fn routing_context_binds_header_not_ciphertext() {
        let a = v1::Envelope {
            protocol_version: 1,
            envelope_id: USER.into(),
            recipient_device_id: DEVICE.into(),
            expires_at_ms: 10,
            sealed_payload: vec![1],
        };
        let mut b = a.clone();
        b.sealed_payload = vec![2];
        assert_eq!(routing_context(&a), routing_context(&b));
        b.expires_at_ms = 11;
        assert_ne!(routing_context(&a), routing_context(&b));
    }
}
