//! Shared WebRTC SDP/ICE signaling contract.
//!
//! The gateway routes these short-lived messages between authenticated device
//! sockets. This module never handles media, ICE credentials beyond the SDP
//! text supplied by the WebRTC host, or media encryption keys.

use crate::{protocol, CoreError};
use prost::Message;
use uuid::Uuid;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WebRtcSignalKind {
    Offer,
    Answer,
    IceCandidate,
}

impl WebRtcSignalKind {
    fn wire_value(self) -> i32 {
        match self {
            Self::Offer => 1,
            Self::Answer => 2,
            Self::IceCandidate => 3,
        }
    }

    fn from_wire(value: i32) -> Result<Self, CoreError> {
        match value {
            1 => Ok(Self::Offer),
            2 => Ok(Self::Answer),
            3 => Ok(Self::IceCandidate),
            _ => Err(CoreError::Authentication),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WebRtcSignal {
    pub session_id: Uuid,
    pub target_device_id: Uuid,
    pub kind: WebRtcSignalKind,
    pub sdp: String,
    pub sdp_mid: Option<String>,
    pub sdp_mline_index: Option<u32>,
}

impl WebRtcSignal {
    pub fn new(
        session_id: Uuid,
        target_device_id: Uuid,
        kind: WebRtcSignalKind,
        sdp: String,
        sdp_mid: Option<String>,
        sdp_mline_index: Option<u32>,
    ) -> Result<Self, CoreError> {
        let signal = Self {
            session_id,
            target_device_id,
            kind,
            sdp,
            sdp_mid,
            sdp_mline_index,
        };
        signal.validate()?;
        Ok(signal)
    }

    pub fn validate(&self) -> Result<(), CoreError> {
        if self.session_id.is_nil() || self.target_device_id.is_nil() {
            return Err(CoreError::Authentication);
        }
        let proto = self.to_proto();
        protocol::validate_webrtc_signal(&proto)?;
        Ok(())
    }

    fn to_proto(&self) -> protocol::v1::WebRtcSignal {
        protocol::v1::WebRtcSignal {
            session_id: self.session_id.to_string(),
            target_device_id: self.target_device_id.to_string(),
            kind: self.kind.wire_value(),
            sdp: self.sdp.clone(),
            sdp_mid: self.sdp_mid.clone().unwrap_or_default(),
            sdp_mline_index: self.sdp_mline_index.unwrap_or_default(),
        }
    }

    fn from_proto(signal: protocol::v1::WebRtcSignal) -> Result<Self, CoreError> {
        protocol::validate_webrtc_signal(&signal)?;
        let session_id = Uuid::parse_str(&signal.session_id).map_err(|_| CoreError::Authentication)?;
        let target_device_id =
            Uuid::parse_str(&signal.target_device_id).map_err(|_| CoreError::Authentication)?;
        Ok(Self {
            session_id,
            target_device_id,
            kind: WebRtcSignalKind::from_wire(signal.kind)?,
            sdp: signal.sdp,
            sdp_mid: (!signal.sdp_mid.is_empty()).then_some(signal.sdp_mid),
            sdp_mline_index: (signal.sdp_mline_index != 0).then_some(signal.sdp_mline_index),
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WebRtcSignalDelivery {
    pub request_id: String,
    pub sender_device_id: Uuid,
    pub signal: WebRtcSignal,
}

impl WebRtcSignalDelivery {
    pub fn new(
        request_id: String,
        sender_device_id: Uuid,
        signal: WebRtcSignal,
    ) -> Result<Self, CoreError> {
        let delivery = Self {
            request_id,
            sender_device_id,
            signal,
        };
        delivery.validate()?;
        Ok(delivery)
    }

    pub fn validate(&self) -> Result<(), CoreError> {
        protocol::validate_id(&self.request_id)?;
        if self.sender_device_id.is_nil() || self.sender_device_id == self.signal.target_device_id {
            return Err(CoreError::Authentication);
        }
        self.signal.validate()?;
        let proto = protocol::v1::WebRtcSignalDelivery {
            request_id: self.request_id.clone(),
            sender_device_id: self.sender_device_id.to_string(),
            signal: Some(self.signal.to_proto()),
        };
        protocol::validate_webrtc_signal_delivery(&proto)?;
        Ok(())
    }

    fn to_proto(&self) -> protocol::v1::WebRtcSignalDelivery {
        protocol::v1::WebRtcSignalDelivery {
            request_id: self.request_id.clone(),
            sender_device_id: self.sender_device_id.to_string(),
            signal: Some(self.signal.to_proto()),
        }
    }

    fn from_proto(delivery: protocol::v1::WebRtcSignalDelivery) -> Result<Self, CoreError> {
        protocol::validate_webrtc_signal_delivery(&delivery)?;
        let sender_device_id =
            Uuid::parse_str(&delivery.sender_device_id).map_err(|_| CoreError::Authentication)?;
        let signal = WebRtcSignal::from_proto(
            delivery
                .signal
                .ok_or(CoreError::Authentication)?,
        )?;
        Self::new(delivery.request_id, sender_device_id, signal)
    }
}

pub fn encode_client_signal(
    request_id: &str,
    signal: &WebRtcSignal,
) -> Result<Vec<u8>, CoreError> {
    protocol::validate_id(request_id)?;
    signal.validate()?;
    let frame = protocol::v1::ClientFrame {
        request_id: request_id.to_owned(),
        body: Some(protocol::v1::client_frame::Body::WebRtcSignal(signal.to_proto())),
    };
    if frame.encoded_len() > protocol::MAX_FRAME_BYTES {
        return Err(protocol::ProtocolError::TooLarge.into());
    }
    Ok(frame.encode_to_vec())
}

pub fn decode_server_signal(bytes: &[u8]) -> Result<WebRtcSignalDelivery, CoreError> {
    if bytes.is_empty() || bytes.len() > protocol::MAX_FRAME_BYTES {
        return Err(CoreError::Protocol(protocol::ProtocolError::TooLarge));
    }
    let frame = protocol::v1::ServerFrame::decode(bytes)
        .map_err(|_| CoreError::Protocol(protocol::ProtocolError::Malformed))?;
    protocol::validate_id(&frame.request_id)?;
    let delivery = match frame.body {
        Some(protocol::v1::server_frame::Body::WebRtcSignal(delivery)) => delivery,
        _ => return Err(CoreError::Authentication),
    };
    WebRtcSignalDelivery::from_proto(delivery)
}

pub fn is_server_signal(bytes: &[u8]) -> bool {
    if bytes.is_empty() || bytes.len() > protocol::MAX_FRAME_BYTES {
        return false;
    }
    let Ok(frame) = protocol::v1::ServerFrame::decode(bytes) else {
        return false;
    };
    matches!(
        frame.body,
        Some(protocol::v1::server_frame::Body::WebRtcSignal(_))
    )
}
