//! Shared video profile limits for native hardware transcoders.
//!
//! Actual frame decode/encode stays in the platform media stacks. This module
//! keeps profile selection and output MIME contracts identical on both mobile
//! clients.

use thiserror::Error;

pub const VIDEO_MAX_INPUT_BYTES: usize = 256 * 1024 * 1024;
pub const VIDEO_FRAME_RATE: u32 = 30;
pub const VIDEO_720P_WIDTH: u32 = 1_280;
pub const VIDEO_720P_HEIGHT: u32 = 720;
pub const VIDEO_720P_BITRATE_BPS: u32 = 1_500_000;
pub const VIDEO_1080P_WIDTH: u32 = 1_920;
pub const VIDEO_1080P_HEIGHT: u32 = 1_080;
pub const VIDEO_1080P_BITRATE_BPS: u32 = 3_000_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VideoCodec {
    H264,
    Hevc,
}

impl VideoCodec {
    pub const fn mime_type(self) -> &'static str {
        match self {
            Self::H264 => "video/avc",
            Self::Hevc => "video/hevc",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VideoProfile {
    pub codec: VideoCodec,
    pub width: u32,
    pub height: u32,
    pub bitrate_bps: u32,
    pub frame_rate: u32,
}

impl VideoProfile {
    pub const fn hd_720p(codec: VideoCodec) -> Self {
        Self {
            codec,
            width: VIDEO_720P_WIDTH,
            height: VIDEO_720P_HEIGHT,
            bitrate_bps: VIDEO_720P_BITRATE_BPS,
            frame_rate: VIDEO_FRAME_RATE,
        }
    }

    pub const fn full_hd_1080p(codec: VideoCodec) -> Self {
        Self {
            codec,
            width: VIDEO_1080P_WIDTH,
            height: VIDEO_1080P_HEIGHT,
            bitrate_bps: VIDEO_1080P_BITRATE_BPS,
            frame_rate: VIDEO_FRAME_RATE,
        }
    }

    pub fn validate(self) -> Result<(), VideoError> {
        let valid_size = (self.width, self.height) == (VIDEO_720P_WIDTH, VIDEO_720P_HEIGHT)
            || (self.width, self.height) == (VIDEO_1080P_WIDTH, VIDEO_1080P_HEIGHT);
        if !valid_size
            || self.frame_rate == 0
            || self.frame_rate > 60
            || self.bitrate_bps == 0
        {
            return Err(VideoError::InvalidProfile);
        }
        Ok(())
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum VideoError {
    #[error("invalid video profile")]
    InvalidProfile,
    #[error("video input exceeds size limit")]
    TooLarge,
    #[error("hardware video encoder unavailable")]
    HardwareUnavailable,
    #[error("video container failure")]
    InvalidContainer,
}
