use crate::{
    crypto::{EnvelopeCrypto, SecretBytes},
    identity::LocalIdentity,
    mls::MlsEngine,
    protocol::{self, v1},
    sequences::ConversationSequence,
    CoreError,
};
use prost::Message;
use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FanoutRecipient {
    pub recipient_device_id: String,
    pub envelope_id: String,
}

/// Build one fresh envelope identity for every non-revoked device in an
/// authenticated directory snapshot. The caller must verify the directory's
/// authenticity before passing these users to this helper.
pub fn fanout_recipients_for_users(users: &[v1::User]) -> Result<Vec<FanoutRecipient>, CoreError> {
    if users.is_empty() {
        return Err(CoreError::Protocol(protocol::ProtocolError::Invalid(
            "fanout users",
        )));
    }
    let mut recipients = Vec::new();
    for user in users {
        protocol::validate_user(user)?;
        for device in &user.devices {
            if device.revoked_at_ms.is_none() {
                recipients.push(FanoutRecipient {
                    recipient_device_id: device.device_id.clone(),
                    envelope_id: random_envelope_id()?,
                });
            }
        }
    }
    validate_fanout_recipients(&recipients)?;
    Ok(recipients)
}

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
        let recipients = [FanoutRecipient {
            recipient_device_id,
            envelope_id,
        }];
        self.seal_message_for_devices(message, &recipients, expires_at_ms, now_ms)
            .map(|mut envelopes| envelopes.remove(0))
    }

    /// Encrypt the MLS application message once and wrap that ciphertext for
    /// every active recipient device. Each outer envelope is independently
    /// bound to its device key and routing header, so the returned envelopes
    /// may be sent to separate device mailboxes without exposing conversation
    /// metadata to the gateway.
    ///
    /// The caller must obtain `recipients` from an authenticated directory and
    /// persist all returned envelopes with the MLS state in one host
    /// transaction. Retries must reuse these exact envelopes.
    pub fn seal_message_for_devices(
        &mut self,
        message: &v1::Message,
        recipients: &[FanoutRecipient],
        expires_at_ms: u64,
        now_ms: u64,
    ) -> Result<Vec<v1::Envelope>, CoreError> {
        protocol::validate_message(message)?;
        if message.sender_device_id != self.identity.device_id() {
            return Err(CoreError::Authentication);
        }
        validate_fanout_recipients(recipients)?;
        let plaintext = SecretBytes::new(message.encode_to_vec());
        let ciphertext = self.mls.encrypt(
            &message.conversation_id,
            &message.sender_device_id,
            plaintext.as_bytes(),
        )?;
        let mut envelopes = Vec::with_capacity(recipients.len());
        for recipient in recipients {
            envelopes.push(self.seal_ciphertext_for_device(
                recipient,
                expires_at_ms,
                now_ms,
                &ciphertext,
            )?);
        }
        Ok(envelopes)
    }

    /// Assign one sender-local sequence and fan it out without advancing the
    /// MLS ratchet more than once.
    pub fn seal_next_message_for_devices(
        &mut self,
        mut message: v1::Message,
        sequence: &mut ConversationSequence,
        recipients: &[FanoutRecipient],
        expires_at_ms: u64,
        now_ms: u64,
    ) -> Result<(v1::Message, Vec<v1::Envelope>), CoreError> {
        if message.conversation_id != sequence.conversation_id()
            || message.sender_device_id != sequence.sender_device_id()
            || message.sender_device_id != self.identity.device_id()
        {
            return Err(CoreError::Authentication);
        }
        message.sequence_id = sequence.reserve_next()?;
        let envelopes =
            self.seal_message_for_devices(&message, recipients, expires_at_ms, now_ms)?;
        Ok((message, envelopes))
    }

    fn seal_ciphertext_for_device(
        &mut self,
        recipient: &FanoutRecipient,
        expires_at_ms: u64,
        now_ms: u64,
        ciphertext: &[u8],
    ) -> Result<v1::Envelope, CoreError> {
        let mut envelope = v1::Envelope {
            protocol_version: protocol::VERSION,
            envelope_id: recipient.envelope_id.clone(),
            recipient_device_id: recipient.recipient_device_id.clone(),
            expires_at_ms,
            sealed_payload: vec![1],
        };
        protocol::validate_enqueue(&envelope, now_ms)?;
        envelope.sealed_payload = self.crypto.seal(
            &envelope.recipient_device_id,
            &routing_context(&envelope),
            ciphertext,
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

fn random_envelope_id() -> Result<String, CoreError> {
    let mut bytes = [0; 16];
    getrandom::fill(&mut bytes).map_err(|_| CoreError::Provider)?;
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Ok(uuid::Uuid::from_bytes(bytes).to_string())
}

fn validate_fanout_recipients(recipients: &[FanoutRecipient]) -> Result<(), CoreError> {
    if recipients.is_empty() || recipients.len() > protocol::MAX_FANOUT_DEVICES {
        return Err(CoreError::Protocol(protocol::ProtocolError::Invalid(
            "fanout recipients",
        )));
    }
    let mut device_ids = HashSet::with_capacity(recipients.len());
    let mut envelope_ids = HashSet::with_capacity(recipients.len());
    for recipient in recipients {
        protocol::validate_id(&recipient.recipient_device_id)?;
        protocol::validate_id(&recipient.envelope_id)?;
        if !device_ids.insert(&recipient.recipient_device_id)
            || !envelope_ids.insert(&recipient.envelope_id)
        {
            return Err(CoreError::Protocol(protocol::ProtocolError::Invalid(
                "duplicate fanout recipient",
            )));
        }
    }
    Ok(())
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
        encryptions: usize,
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
            self.encryptions += 1;
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
            encryptions: 0,
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

    #[test]
    fn fanout_encrypts_once_and_binds_each_device_envelope() {
        let fixture = fixture_mls();
        let message = fixture.message.clone();
        let second_device = "00000000-0000-4000-8000-000000000003";
        let recipients = [
            FanoutRecipient {
                recipient_device_id: DEVICE.into(),
                envelope_id: USER.into(),
            },
            FanoutRecipient {
                recipient_device_id: second_device.into(),
                envelope_id: second_device.into(),
            },
        ];
        let mut core = ClientCore::new(
            LocalIdentity::new(USER.into(), DEVICE.into()).unwrap(),
            FixtureCrypto,
            fixture,
        );
        let envelopes = core
            .seal_message_for_devices(&message, &recipients, 1000, 1)
            .unwrap();
        assert_eq!(core.mls.encryptions, 1);
        assert_eq!(envelopes.len(), 2);
        assert_eq!(envelopes[0].recipient_device_id, DEVICE);
        assert_eq!(envelopes[1].recipient_device_id, second_device);
        assert_ne!(envelopes[0].envelope_id, envelopes[1].envelope_id);
    }

    #[test]
    fn fanout_rejects_duplicate_devices_or_envelopes() {
        let fixture = fixture_mls();
        let message = fixture.message.clone();
        let recipients = [
            FanoutRecipient {
                recipient_device_id: DEVICE.into(),
                envelope_id: USER.into(),
            },
            FanoutRecipient {
                recipient_device_id: DEVICE.into(),
                envelope_id: "00000000-0000-4000-8000-000000000003".into(),
            },
        ];
        let mut core = ClientCore::new(
            LocalIdentity::new(USER.into(), DEVICE.into()).unwrap(),
            FixtureCrypto,
            fixture,
        );
        assert!(core
            .seal_message_for_devices(&message, &recipients, 1000, 1)
            .is_err());
    }

    #[test]
    fn directory_fanout_excludes_revoked_devices() {
        let revoked_device = "00000000-0000-4000-8000-000000000003";
        let user = v1::User {
            user_id: USER.into(),
            handle: None,
            devices: vec![
                v1::Device {
                    device_id: DEVICE.into(),
                    user_id: USER.into(),
                    identity_public_key: vec![1],
                    mls_credential: vec![2],
                    registered_at_ms: 1,
                    revoked_at_ms: None,
                },
                v1::Device {
                    device_id: revoked_device.into(),
                    user_id: USER.into(),
                    identity_public_key: vec![3],
                    mls_credential: vec![4],
                    registered_at_ms: 1,
                    revoked_at_ms: Some(2),
                },
            ],
        };
        let recipients = fanout_recipients_for_users(&[user]).unwrap();
        assert_eq!(recipients.len(), 1);
        assert_eq!(recipients[0].recipient_device_id, DEVICE);
        assert_ne!(recipients[0].envelope_id, USER);
    }
}
