//! Desktop voice/video/live-stream call orchestration.
//!
//! The desktop UI supplies native WebRTC and provider adapters. This module
//! owns the ordering that must stay identical across platforms: install the
//! MLS-authenticated SFrame key, configure media, join the opaque SFU room,
//! then exchange SDP and ICE. The SFU adapter never sees the key.

use links_client_core::CoreError;
use std::mem;
use uuid::Uuid;

const MAX_TOKEN_BYTES: usize = 4_096;
const MAX_ROOM_NAME_BYTES: usize = 128;
const MIN_ROOM_NAME_BYTES: usize = 16;
const MAX_SIGNAL_BYTES: usize = 256 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DesktopCallMode {
    Voice,
    Video,
    LiveStream,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DesktopCallState {
    Idle,
    Preparing,
    Joining,
    Negotiating,
    Connected,
    Ended,
    Failed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DesktopCallSignalKind {
    Offer,
    Answer,
    IceCandidate,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DesktopCallPlacement {
    pub room_name: String,
    pub region: String,
    pub endpoint: String,
    pub access_token: String,
    pub require_sframe: bool,
}

impl DesktopCallPlacement {
    pub fn new(
        room_name: impl Into<String>,
        region: impl Into<String>,
        endpoint: impl Into<String>,
        access_token: impl Into<String>,
    ) -> Result<Self, CoreError> {
        let placement = Self {
            room_name: room_name.into(),
            region: region.into(),
            endpoint: endpoint.into(),
            access_token: access_token.into(),
            require_sframe: true,
        };
        if valid_room_name(&placement.room_name)
            && valid_region(&placement.region)
            && valid_endpoint(&placement.endpoint)
            && !placement.access_token.is_empty()
            && placement.access_token.len() <= MAX_TOKEN_BYTES
        {
            Ok(placement)
        } else {
            Err(CoreError::Provider)
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DesktopCallSignal {
    pub session_id: Uuid,
    pub kind: DesktopCallSignalKind,
    pub sdp: String,
    pub sdp_mid: Option<String>,
    pub sdp_mline_index: Option<u32>,
}

impl DesktopCallSignal {
    pub fn new(
        session_id: Uuid,
        kind: DesktopCallSignalKind,
        sdp: impl Into<String>,
        sdp_mid: Option<String>,
        sdp_mline_index: Option<u32>,
    ) -> Result<Self, CoreError> {
        let signal = Self {
            session_id,
            kind,
            sdp: sdp.into(),
            sdp_mid,
            sdp_mline_index,
        };
        if signal.session_id.is_nil()
            || signal.sdp.is_empty()
            || signal.sdp.len() > MAX_SIGNAL_BYTES
            || signal
                .sdp_mid
                .as_ref()
                .is_some_and(|value| value.len() > 256)
        {
            return Err(CoreError::Provider);
        }
        Ok(signal)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DesktopCallEpochKey {
    pub media_session_id: Uuid,
    pub key_id: u64,
    pub epoch: u64,
    pub key: Vec<u8>,
}

impl DesktopCallEpochKey {
    pub fn new(
        media_session_id: Uuid,
        key_id: u64,
        epoch: u64,
        key: Vec<u8>,
    ) -> Result<Self, CoreError> {
        if media_session_id.is_nil() || key.len() != 16 || key.iter().all(|byte| *byte == 0) {
            return Err(CoreError::Provider);
        }
        Ok(Self {
            media_session_id,
            key_id,
            epoch,
            key,
        })
    }

    fn wipe(&mut self) {
        self.key.fill(0);
    }
}

impl Drop for DesktopCallEpochKey {
    fn drop(&mut self) {
        self.wipe();
    }
}

/// Provider SDK boundary. It handles room join and SDP/ICE only.
pub trait DesktopCallSignaling {
    fn join(
        &mut self,
        placement: &DesktopCallPlacement,
        session_id: Uuid,
        media_session_id: Uuid,
    ) -> Result<(), CoreError>;
    fn send(&mut self, signal: &DesktopCallSignal) -> Result<(), CoreError>;
    fn leave(&mut self) -> Result<(), CoreError>;
}

/// MLS application-control boundary. It must encrypt the key before sending.
pub trait DesktopCallMlsKeyProvider {
    fn create_initial_key(&mut self, media_session_id: Uuid) -> Result<DesktopCallEpochKey, CoreError>;
    fn publish_epoch_key(&mut self, key: &DesktopCallEpochKey) -> Result<(), CoreError>;
}

/// Native WebRTC binding, including the platform SFrame transform hooks.
pub trait DesktopCallMediaEngine {
    fn prepare_sframe(&mut self, key: &DesktopCallEpochKey) -> Result<(), CoreError>;
    fn install_sframe(&mut self, key: &DesktopCallEpochKey) -> Result<(), CoreError>;
    fn configure(&mut self, mode: DesktopCallMode) -> Result<(), CoreError>;
    fn create_offer(&mut self) -> Result<String, CoreError>;
    fn create_answer(&mut self) -> Result<String, CoreError>;
    fn set_remote_description(&mut self, signal: &DesktopCallSignal) -> Result<(), CoreError>;
    fn add_ice_candidate(&mut self, signal: &DesktopCallSignal) -> Result<(), CoreError>;
    fn attach_sframe_to_receivers(&mut self) -> Result<(), CoreError>;
    fn close(&mut self);
}

/// Desktop call and live-stream state machine. Feed provider callbacks into
/// `handle_signal` from the desktop event loop and call `mark_connected` once
/// the WebRTC peer reports an established connection.
pub struct DesktopCallSession<S, M, E>
where
    S: DesktopCallSignaling,
    M: DesktopCallMlsKeyProvider,
    E: DesktopCallMediaEngine,
{
    mode: DesktopCallMode,
    placement: DesktopCallPlacement,
    signaling: S,
    mls: M,
    media_engine: E,
    session_id: Uuid,
    media_session_id: Uuid,
    state: DesktopCallState,
    joined: bool,
    remote_description_set: bool,
    pending_candidates: Vec<DesktopCallSignal>,
}

impl<S, M, E> DesktopCallSession<S, M, E>
where
    S: DesktopCallSignaling,
    M: DesktopCallMlsKeyProvider,
    E: DesktopCallMediaEngine,
{
    pub fn new(
        mode: DesktopCallMode,
        placement: DesktopCallPlacement,
        signaling: S,
        mls: M,
        media_engine: E,
        session_id: Uuid,
        media_session_id: Option<Uuid>,
    ) -> Result<Self, CoreError> {
        if session_id.is_nil() {
            return Err(CoreError::Provider);
        }
        let media_session_id = media_session_id.unwrap_or(session_id);
        if media_session_id.is_nil() {
            return Err(CoreError::Provider);
        }
        validate_placement(&placement)?;
        Ok(Self {
            mode,
            placement,
            signaling,
            mls,
            media_engine,
            session_id,
            media_session_id,
            state: DesktopCallState::Idle,
            joined: false,
            remote_description_set: false,
            pending_candidates: Vec::new(),
        })
    }

    pub fn state(&self) -> DesktopCallState {
        self.state
    }

    pub fn mode(&self) -> DesktopCallMode {
        self.mode
    }

    pub fn session_id(&self) -> Uuid {
        self.session_id
    }

    pub fn media_session_id(&self) -> Uuid {
        self.media_session_id
    }

    pub fn start(&mut self) -> Result<(), CoreError> {
        if self.state != DesktopCallState::Idle {
            return Err(CoreError::Provider);
        }
        self.set_state(DesktopCallState::Preparing);
        let result = (|| {
            let initial = self.mls.create_initial_key(self.media_session_id)?;
            validate_key(&initial, self.media_session_id)?;
            self.media_engine.prepare_sframe(&initial)?;
            self.publish_epoch_key(&initial)?;
            self.media_engine.configure(self.mode)?;
            self.joined = true;
            self.set_state(DesktopCallState::Joining);
            self.signaling
                .join(&self.placement, self.session_id, self.media_session_id)?;
            self.set_state(DesktopCallState::Negotiating);
            let offer = self.media_engine.create_offer()?;
            let signal = DesktopCallSignal::new(
                self.session_id,
                DesktopCallSignalKind::Offer,
                offer,
                None,
                None,
            )?;
            self.signaling.send(&signal)
        })();
        if let Err(error) = result {
            self.fail();
            return Err(error);
        }
        Ok(())
    }

    pub fn publish_sframe_epoch_key(&mut self, key: &DesktopCallEpochKey) -> Result<(), CoreError> {
        validate_key(key, self.media_session_id)?;
        let working = key.clone();
        self.media_engine.install_sframe(&working)?;
        self.mls.publish_epoch_key(&working)
    }

    pub fn install_sframe_epoch_key(&mut self, key: &DesktopCallEpochKey) -> Result<(), CoreError> {
        validate_key(key, self.media_session_id)?;
        let working = key.clone();
        self.media_engine.install_sframe(&working)
    }

    pub fn handle_signal(&mut self, signal: DesktopCallSignal) -> Result<(), CoreError> {
        let result = (|| {
            if !self.joined || signal.session_id != self.session_id {
                return Err(CoreError::Provider);
            }
            validate_signal(&signal, self.session_id)?;
            match signal.kind {
                DesktopCallSignalKind::IceCandidate => {
                    if !self.remote_description_set {
                        self.pending_candidates.push(signal);
                        return Ok(());
                    }
                    self.media_engine.add_ice_candidate(&signal)
                }
                DesktopCallSignalKind::Offer => {
                    self.media_engine.set_remote_description(&signal)?;
                    self.remote_description_set = true;
                    self.media_engine.attach_sframe_to_receivers()?;
                    self.flush_candidates()?;
                    let answer = self.media_engine.create_answer()?;
                    let response = DesktopCallSignal::new(
                        self.session_id,
                        DesktopCallSignalKind::Answer,
                        answer,
                        None,
                        None,
                    )?;
                    self.signaling.send(&response)
                }
                DesktopCallSignalKind::Answer => {
                    self.media_engine.set_remote_description(&signal)?;
                    self.remote_description_set = true;
                    self.media_engine.attach_sframe_to_receivers()?;
                    self.flush_candidates()
                }
            }
        })();
        if result.is_err() {
            self.fail();
        }
        result
    }

    pub fn mark_connected(&mut self) -> Result<(), CoreError> {
        if self.state != DesktopCallState::Negotiating {
            return Err(CoreError::Provider);
        }
        self.set_state(DesktopCallState::Connected);
        Ok(())
    }

    pub fn stop(&mut self) {
        if self.state == DesktopCallState::Ended {
            return;
        }
        self.joined = false;
        let _ = self.signaling.leave();
        self.media_engine.close();
        self.pending_candidates.clear();
        self.set_state(DesktopCallState::Ended);
    }

    fn publish_epoch_key(&mut self, key: &DesktopCallEpochKey) -> Result<(), CoreError> {
        let outbound = key.clone();
        self.mls.publish_epoch_key(&outbound)
    }

    fn flush_candidates(&mut self) -> Result<(), CoreError> {
        let candidates = mem::take(&mut self.pending_candidates);
        for candidate in candidates {
            self.media_engine.add_ice_candidate(&candidate)?;
        }
        Ok(())
    }

    fn fail(&mut self) {
        if self.state == DesktopCallState::Ended || self.state == DesktopCallState::Failed {
            return;
        }
        self.joined = false;
        let _ = self.signaling.leave();
        self.media_engine.close();
        self.pending_candidates.clear();
        self.set_state(DesktopCallState::Failed);
    }

    fn set_state(&mut self, state: DesktopCallState) {
        self.state = state;
    }
}

fn validate_key(key: &DesktopCallEpochKey, expected_media_session_id: Uuid) -> Result<(), CoreError> {
    if key.media_session_id != expected_media_session_id
        || key.key.len() != 16
        || key.key.iter().all(|byte| *byte == 0)
    {
        return Err(CoreError::Provider);
    }
    Ok(())
}

fn validate_placement(placement: &DesktopCallPlacement) -> Result<(), CoreError> {
    if !placement.require_sframe
        || !valid_room_name(&placement.room_name)
        || !valid_region(&placement.region)
        || !valid_endpoint(&placement.endpoint)
        || placement.access_token.is_empty()
        || placement.access_token.len() > MAX_TOKEN_BYTES
    {
        return Err(CoreError::Provider);
    }
    Ok(())
}

fn validate_signal(signal: &DesktopCallSignal, session_id: Uuid) -> Result<(), CoreError> {
    if signal.session_id != session_id
        || signal.session_id.is_nil()
        || signal.sdp.is_empty()
        || signal.sdp.len() > MAX_SIGNAL_BYTES
        || signal
            .sdp_mid
            .as_ref()
            .is_some_and(|value| value.len() > 256)
    {
        return Err(CoreError::Provider);
    }
    Ok(())
}

fn valid_room_name(value: &str) -> bool {
    let bytes = value.as_bytes();
    (MIN_ROOM_NAME_BYTES..=MAX_ROOM_NAME_BYTES).contains(&bytes.len())
        && bytes.iter().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_')
        })
}

fn valid_region(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value.as_bytes().iter().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_' | b':')
        })
}

fn valid_endpoint(value: &str) -> bool {
    let Some(authority) = value.strip_prefix("wss://") else {
        return false;
    };
    if value.contains('?') || value.contains('#') {
        return false;
    }
    let authority = authority.split('/').next().unwrap_or_default();
    !authority.is_empty()
        && authority
            .as_bytes()
            .iter()
            .all(|byte| !byte.is_ascii_control() && !byte.is_ascii_whitespace() && *byte != b'@')
}
