use thiserror::Error;
use zeroize::Zeroizing;

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

#[derive(Debug, Error, PartialEq, Eq)]
pub enum SFrameError {
    #[error("invalid SFrame epoch key")]
    InvalidKey,
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
