use thiserror::Error;
use zeroize::Zeroizing;

use crate::{protocol, CoreError};

pub const SFRAME_KEY_BYTES: usize = 16;
pub const SFRAME_MAX_ACTIVE_KEYS: usize = 2;

/// Cipher suite shared with the browser's native SFrame transform.
///
/// Keep this list intentionally small until every platform client has an
/// audited implementation for the selected suite.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SFrameCipherSuite {
    Aes128GcmSha256_128,
}

impl SFrameCipherSuite {
    pub const fn wire_name(self) -> &'static str {
        match self {
            Self::Aes128GcmSha256_128 => "AES_128_GCM_SHA256_128",
        }
    }
}

impl TryFrom<u32> for SFrameCipherSuite {
    type Error = SFrameError;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        match value {
            protocol::SFRAME_CIPHER_SUITE_AES_128_GCM_SHA256_128 => {
                Ok(Self::Aes128GcmSha256_128)
            }
            _ => Err(SFrameError::UnsupportedCipherSuite),
        }
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum SFrameError {
    #[error("invalid SFrame epoch key")]
    InvalidKey,
    #[error("unsupported SFrame cipher suite")]
    UnsupportedCipherSuite,
    #[error("SFrame epoch moved backwards")]
    StaleEpoch,
    #[error("SFrame key ID is already active")]
    KeyIdReuse,
}

/// One MLS-derived SFrame epoch key. The key is retained only in zeroizing
/// memory and is never intended to cross a server boundary.
pub struct SFrameEpochKey {
    key_id: u64,
    epoch: u64,
    key: Zeroizing<[u8; SFRAME_KEY_BYTES]>,
}

impl SFrameEpochKey {
    pub fn new(
        key_id: u64,
        epoch: u64,
        key: [u8; SFRAME_KEY_BYTES],
    ) -> Result<Self, SFrameError> {
        if key.iter().all(|byte| *byte == 0) {
            return Err(SFrameError::InvalidKey);
        }
        Ok(Self {
            key_id,
            epoch,
            key: Zeroizing::new(key),
        })
    }

    pub const fn key_id(&self) -> u64 {
        self.key_id
    }

    pub const fn epoch(&self) -> u64 {
        self.epoch
    }

    pub fn key_bytes(&self) -> &[u8; SFRAME_KEY_BYTES] {
        &self.key
    }
}

/// Bounded sender/receiver key schedule for an SFrame media session.
///
/// The current and immediately previous epoch remain available so packets
/// crossing a rotation boundary can be authenticated without accepting an
/// unbounded collection of stale keys.
pub struct SFrameKeySchedule {
    cipher_suite: SFrameCipherSuite,
    current: Option<SFrameEpochKey>,
    previous: Option<SFrameEpochKey>,
}

impl SFrameKeySchedule {
    pub const fn new(cipher_suite: SFrameCipherSuite) -> Self {
        Self {
            cipher_suite,
            current: None,
            previous: None,
        }
    }

    pub const fn cipher_suite(&self) -> SFrameCipherSuite {
        self.cipher_suite
    }

    pub fn install(&mut self, key: SFrameEpochKey) -> Result<(), SFrameError> {
        if let Some(active) = self
            .current
            .as_ref()
            .filter(|active| active.key_id() == key.key_id())
            .or_else(|| {
                self.previous
                    .as_ref()
                    .filter(|active| active.key_id() == key.key_id())
            })
        {
            if active.epoch() == key.epoch() && active.key_bytes() == key.key_bytes() {
                return Ok(());
            }
            return Err(SFrameError::KeyIdReuse);
        }
        if let Some(current) = self.current.as_ref() {
            if key.epoch() <= current.epoch() {
                return Err(SFrameError::StaleEpoch);
            }
        }
        if self
            .current
            .as_ref()
            .is_some_and(|active| active.key_id() == key.key_id())
            || self
                .previous
                .as_ref()
                .is_some_and(|active| active.key_id() == key.key_id())
        {
            return Err(SFrameError::KeyIdReuse);
        }
        self.previous = self.current.take();
        self.current = Some(key);
        Ok(())
    }

    pub fn current(&self) -> Option<&SFrameEpochKey> {
        self.current.as_ref()
    }

    pub fn previous(&self) -> Option<&SFrameEpochKey> {
        self.previous.as_ref()
    }

    pub const fn active_key_count(&self) -> usize {
        match (&self.current, &self.previous) {
            (Some(_), Some(_)) => SFRAME_MAX_ACTIVE_KEYS,
            (Some(_), None) | (None, Some(_)) => 1,
            (None, None) => 0,
        }
    }
}

/// Authenticated MLS application control payload for rotating one WebRTC
/// media session's SFrame key. The key is copied into the protobuf only while
/// the caller is constructing the MLS plaintext; the resulting bytes must be
/// encrypted immediately by `MlsEngine::encrypt`.
pub struct SFrameEpochKeyUpdate {
    media_session_id: String,
    cipher_suite: SFrameCipherSuite,
    key: SFrameEpochKey,
}

impl SFrameEpochKeyUpdate {
    pub fn new(
        media_session_id: String,
        key_id: u64,
        epoch: u64,
        key: [u8; SFRAME_KEY_BYTES],
    ) -> Result<Self, CoreError> {
        protocol::validate_id(&media_session_id)?;
        let key = SFrameEpochKey::new(key_id, epoch, key)?;
        Ok(Self {
            media_session_id,
            cipher_suite: SFrameCipherSuite::Aes128GcmSha256_128,
            key,
        })
    }

    pub fn from_proto(update: &protocol::v1::SFrameEpochKeyUpdate) -> Result<Self, CoreError> {
        protocol::validate_sframe_epoch_key_update(update)?;
        let key = update
            .key
            .as_slice()
            .try_into()
            .map_err(|_| CoreError::Authentication)?;
        Ok(Self {
            media_session_id: update.media_session_id.clone(),
            cipher_suite: SFrameCipherSuite::try_from(update.cipher_suite)?,
            key: SFrameEpochKey::new(update.key_id, update.epoch, key)?,
        })
    }

    pub fn from_control(control: &protocol::v1::MlsControl) -> Result<Self, CoreError> {
        protocol::validate_mls_control(control)?;
        match control.body.as_ref() {
            Some(protocol::v1::mls_control::Body::SframeEpochKey(update)) => {
                Self::from_proto(update)
            }
            None => Err(CoreError::Authentication),
        }
    }

    pub fn from_message(
        message: &protocol::v1::Message,
    ) -> Result<Option<Self>, CoreError> {
        match message.content.as_ref() {
            Some(protocol::v1::message::Content::MlsControl(control)) => {
                Self::from_control(control).map(Some)
            }
            _ => Ok(None),
        }
    }

    pub fn media_session_id(&self) -> &str {
        &self.media_session_id
    }

    pub const fn cipher_suite(&self) -> SFrameCipherSuite {
        self.cipher_suite
    }

    pub fn epoch_key(&self) -> &SFrameEpochKey {
        &self.key
    }

    pub fn to_proto(&self) -> protocol::v1::SFrameEpochKeyUpdate {
        protocol::v1::SFrameEpochKeyUpdate {
            media_session_id: self.media_session_id.clone(),
            epoch: self.key.epoch(),
            key_id: self.key.key_id(),
            cipher_suite: protocol::SFRAME_CIPHER_SUITE_AES_128_GCM_SHA256_128,
            key: self.key.key_bytes().to_vec(),
        }
    }

    pub fn control(&self) -> protocol::v1::MlsControl {
        protocol::v1::MlsControl {
            protocol_version: protocol::VERSION,
            body: Some(protocol::v1::mls_control::Body::SframeEpochKey(
                self.to_proto(),
            )),
        }
    }

    pub fn message_content(&self) -> protocol::v1::message::Content {
        protocol::v1::message::Content::MlsControl(self.control())
    }
}
