//! Managed global SFU placement for encrypted WebRTC rooms.
//!
//! The provider owns the actual media clusters. This module keeps the
//! gateway-side contract small: validate public LiveKit Cloud endpoints,
//! select a healthy region, and return an opaque room placement. It never
//! stores LiveKit API secrets, room credentials, or media keys.

use super::valid_locator;
use links_protocol::ProtocolError;
use thiserror::Error;

pub const LIVEKIT_CLOUD_PROVIDER: &str = "livekit-cloud";
pub const MIN_GLOBAL_SFU_REGIONS: usize = 2;
pub const MAX_GLOBAL_SFU_REGIONS: usize = 32;
const MAX_ENDPOINT_BYTES: usize = 512;
const MIN_OPAQUE_ROOM_NAME_BYTES: usize = 16;
const MAX_OPAQUE_ROOM_NAME_BYTES: usize = 128;

/// SFU media policy for Links calls. SFrame encrypts the encoded media
/// payload; the SFU only needs ordinary RTP headers for forwarding and
/// congestion control.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SfuMediaPolicy {
    route_using_rtp_headers: bool,
    allow_media_decryption: bool,
    require_sframe: bool,
}

impl SfuMediaPolicy {
    pub const fn encrypted_sframe() -> Self {
        Self {
            route_using_rtp_headers: true,
            allow_media_decryption: false,
            require_sframe: true,
        }
    }

    pub fn validate(self) -> Result<(), SfuError> {
        if self.route_using_rtp_headers && !self.allow_media_decryption && self.require_sframe {
            Ok(())
        } else {
            Err(SfuError::Invalid)
        }
    }

    pub fn routes_using_rtp_headers(self) -> bool {
        self.route_using_rtp_headers
    }

    pub fn allows_media_decryption(self) -> bool {
        self.allow_media_decryption
    }

    pub fn requires_sframe(self) -> bool {
        self.require_sframe
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum SfuError {
    #[error("invalid SFU configuration")]
    Invalid,
    #[error("duplicate SFU region")]
    DuplicateRegion,
    #[error("duplicate SFU endpoint")]
    DuplicateEndpoint,
    #[error("unknown SFU region")]
    UnknownRegion,
    #[error("no healthy SFU region is available")]
    Unavailable,
    #[error("SFU DHT is unavailable")]
    DhtUnavailable,
    #[error("SFU discovery record is not trusted")]
    UntrustedNode,
    #[error("SFU discovery signature is invalid")]
    InvalidSignature,
    #[error("SFU discovery record is expired")]
    ExpiredRecord,
    #[error(transparent)]
    Protocol(#[from] ProtocolError),
}

/// A public, provider-issued regional endpoint. No LiveKit API secret belongs
/// in this value; token issuance remains a trusted backend operation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiveKitRegionEndpoint {
    region: String,
    websocket_url: String,
    turn_url: Option<String>,
    healthy: bool,
}

impl LiveKitRegionEndpoint {
    pub fn new(
        region: String,
        websocket_url: String,
        turn_url: Option<String>,
    ) -> Result<Self, SfuError> {
        if !valid_locator(&region) || !valid_websocket_url(&websocket_url) {
            return Err(SfuError::Invalid);
        }
        if turn_url.as_deref().is_some_and(|url| !valid_turn_url(url)) {
            return Err(SfuError::Invalid);
        }
        Ok(Self {
            region,
            websocket_url,
            turn_url,
            healthy: true,
        })
    }

    pub fn region(&self) -> &str {
        &self.region
    }

    pub fn websocket_url(&self) -> &str {
        &self.websocket_url
    }

    pub fn turn_url(&self) -> Option<&str> {
        self.turn_url.as_deref()
    }

    pub fn is_healthy(&self) -> bool {
        self.healthy
    }
}

/// The gateway-side configuration for a managed LiveKit Cloud deployment.
/// `new` fails closed unless at least two independent regional endpoints are
/// configured, and every placement requires SFrame media encryption.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiveKitCloudDeployment {
    project: String,
    default_region: String,
    regions: Vec<LiveKitRegionEndpoint>,
    media_policy: SfuMediaPolicy,
}

impl LiveKitCloudDeployment {
    pub fn new(
        project: String,
        default_region: String,
        regions: Vec<LiveKitRegionEndpoint>,
    ) -> Result<Self, SfuError> {
        if !valid_locator(&project)
            || !valid_locator(&default_region)
            || !(MIN_GLOBAL_SFU_REGIONS..=MAX_GLOBAL_SFU_REGIONS).contains(&regions.len())
        {
            return Err(SfuError::Invalid);
        }
        let media_policy = SfuMediaPolicy::encrypted_sframe();
        media_policy.validate()?;

        for (index, endpoint) in regions.iter().enumerate() {
            if regions[..index]
                .iter()
                .any(|existing| existing.region == endpoint.region)
            {
                return Err(SfuError::DuplicateRegion);
            }
            if regions[..index].iter().any(|existing| {
                existing.websocket_url == endpoint.websocket_url
                    || (endpoint.turn_url.is_some() && existing.turn_url == endpoint.turn_url)
            }) {
                return Err(SfuError::DuplicateEndpoint);
            }
        }

        if !regions
            .iter()
            .any(|endpoint| endpoint.region == default_region)
        {
            return Err(SfuError::UnknownRegion);
        }

        Ok(Self {
            project,
            default_region,
            regions,
            media_policy,
        })
    }

    pub fn provider(&self) -> &'static str {
        LIVEKIT_CLOUD_PROVIDER
    }

    pub fn project(&self) -> &str {
        &self.project
    }

    pub fn default_region(&self) -> &str {
        &self.default_region
    }

    pub fn regions(&self) -> &[LiveKitRegionEndpoint] {
        &self.regions
    }

    pub fn media_policy(&self) -> SfuMediaPolicy {
        self.media_policy
    }

    pub fn healthy_region_count(&self) -> usize {
        self.regions
            .iter()
            .filter(|endpoint| endpoint.healthy)
            .count()
    }

    /// Update health from an authenticated provider health check or control
    /// plane. The registry itself never makes a network request.
    pub fn mark_healthy(&mut self, region: &str, healthy: bool) -> Result<(), SfuError> {
        let endpoint = self
            .regions
            .iter_mut()
            .find(|endpoint| endpoint.region == region)
            .ok_or(SfuError::UnknownRegion)?;
        endpoint.healthy = healthy;
        Ok(())
    }

    /// Select a healthy regional endpoint. A preferred region is a latency
    /// hint; if it is unhealthy, the default region or another healthy region
    /// is used for availability.
    pub fn select_region(
        &self,
        preferred_region: Option<&str>,
    ) -> Result<&LiveKitRegionEndpoint, SfuError> {
        if let Some(preferred_region) = preferred_region {
            if !valid_locator(preferred_region) {
                return Err(SfuError::Invalid);
            }
            if let Some(endpoint) = self
                .regions
                .iter()
                .find(|endpoint| endpoint.region == preferred_region && endpoint.healthy)
            {
                return Ok(endpoint);
            }
        }

        self.regions
            .iter()
            .find(|endpoint| endpoint.region == self.default_region && endpoint.healthy)
            .or_else(|| self.regions.iter().find(|endpoint| endpoint.healthy))
            .ok_or(SfuError::Unavailable)
    }

    /// Select exactly one region for callers with a data-residency or pinned
    /// deployment requirement. It never silently falls back across regions.
    pub fn select_pinned_region(&self, region: &str) -> Result<&LiveKitRegionEndpoint, SfuError> {
        if !valid_locator(region) {
            return Err(SfuError::Invalid);
        }
        let endpoint = self
            .regions
            .iter()
            .find(|endpoint| endpoint.region == region)
            .ok_or(SfuError::UnknownRegion)?;
        if !endpoint.healthy {
            return Err(SfuError::Unavailable);
        }
        Ok(endpoint)
    }

    /// Place an opaque room on a healthy region. Room names must be random or
    /// otherwise opaque so routing infrastructure cannot learn conversation
    /// identifiers or user handles.
    pub fn place_room(
        &self,
        room_name: String,
        preferred_region: Option<&str>,
    ) -> Result<LiveKitRoomPlacement, SfuError> {
        if !valid_opaque_room_name(&room_name) {
            return Err(SfuError::Invalid);
        }
        let endpoint = self.select_region(preferred_region)?;
        Ok(LiveKitRoomPlacement {
            room_name,
            region: endpoint.region.clone(),
            websocket_url: endpoint.websocket_url.clone(),
            turn_url: endpoint.turn_url.clone(),
            media_policy: self.media_policy,
        })
    }
}

/// Provider connection details returned to a trusted call-session service.
/// The client receives a short-lived access token separately; this value is
/// deliberately not a token and contains no media key material.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiveKitRoomPlacement {
    room_name: String,
    region: String,
    websocket_url: String,
    turn_url: Option<String>,
    media_policy: SfuMediaPolicy,
}

impl LiveKitRoomPlacement {
    pub fn room_name(&self) -> &str {
        &self.room_name
    }

    pub fn region(&self) -> &str {
        &self.region
    }

    pub fn websocket_url(&self) -> &str {
        &self.websocket_url
    }

    pub fn turn_url(&self) -> Option<&str> {
        self.turn_url.as_deref()
    }

    pub fn sframe_required(&self) -> bool {
        self.media_policy.requires_sframe()
    }

    pub fn media_policy(&self) -> SfuMediaPolicy {
        self.media_policy
    }
}

fn valid_websocket_url(value: &str) -> bool {
    valid_url(value, "wss://") && !value.bytes().any(|byte| matches!(byte, b'?' | b'#'))
}

fn valid_turn_url(value: &str) -> bool {
    (value.starts_with("turn:") || value.starts_with("turns:")) && valid_url(value, "turn")
}

fn valid_url(value: &str, prefix: &str) -> bool {
    value.len() <= MAX_ENDPOINT_BYTES
        && value.starts_with(prefix)
        && value.len() > prefix.len()
        && value
            .bytes()
            .all(|byte| !byte.is_ascii_control() && !byte.is_ascii_whitespace())
        && !value.contains('#')
}

fn valid_opaque_room_name(value: &str) -> bool {
    (MIN_OPAQUE_ROOM_NAME_BYTES..=MAX_OPAQUE_ROOM_NAME_BYTES).contains(&value.len())
        && value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.')
        })
}
