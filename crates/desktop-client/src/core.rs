//! Desktop binding for the shared Rust client core.
//!
//! The desktop session is deliberately synchronous at its host boundary. A
//! macOS host can run its async directory, inbox, and outbox work on its own
//! executor, while this adapter keeps the actual `ClientCore` and MLS state
//! behind one serialized session mutex.

use crate::session::{
    DesktopFrameResult, DesktopFrameTransport, DesktopMessagingCore, DesktopReceivedTextMessage,
    DesktopSocketFactory, DesktopTextSession,
};
use links_client_core::{
    crypto::EnvelopeCrypto,
    envelopes::ClientCore,
    mls::MlsEngine,
    protocol::{self, v1},
    CoreError,
};
use prost::Message;
use uuid::Uuid;

/// Host-owned durable and protocol orchestration boundary around the shared
/// `ClientCore`. Implementations persist the cursor/inbox/outbox and execute
/// the shared-core receive/send order with the supplied core. No private key
/// or bearer token belongs in this host object.
pub trait DesktopCoreHost<C, M>: Send
where
    C: EnvelopeCrypto,
    M: MlsEngine,
{
    fn durable_cursor(&self) -> Result<u64, CoreError>;

    /// Decode and process one already validated server frame. The callback may
    /// be invoked only after the host has durably committed the message and
    /// cursor, then sent the corresponding QueueAck.
    fn handle_server_frame(
        &mut self,
        core: &mut ClientCore<C, M>,
        frame: &v1::ServerFrame,
        transport: &mut dyn DesktopFrameTransport,
        full_sync: bool,
        on_text_message: &mut dyn FnMut(DesktopReceivedTextMessage),
    ) -> Result<DesktopFrameResult, CoreError>;

    /// Run the existing shared-core text send coordinator. It must persist the
    /// exact encrypted outbox envelope before returning success.
    fn send_text(
        &mut self,
        core: &mut ClientCore<C, M>,
        conversation_id: &str,
        recipient_user_id: &str,
        text: &str,
        transport: &mut dyn DesktopFrameTransport,
    ) -> Result<(), CoreError>;
}

/// Concrete desktop `DesktopMessagingCore` backed by `links-client-core`.
///
/// `H` is the macOS durable/network host adapter. It receives a mutable
/// `ClientCore`, so the host cannot accidentally create a second crypto or MLS
/// implementation. Use [`bind_desktop_text_session`] to attach it to the
/// reconnecting binary desktop socket.
pub struct RustDesktopMessagingCore<C, M, H> {
    core: ClientCore<C, M>,
    host: H,
}

impl<C, M, H> RustDesktopMessagingCore<C, M, H> {
    pub fn new(core: ClientCore<C, M>, host: H) -> Self {
        Self { core, host }
    }

    pub fn core(&self) -> &ClientCore<C, M> {
        &self.core
    }

    pub fn core_mut(&mut self) -> &mut ClientCore<C, M> {
        &mut self.core
    }

    pub fn host(&self) -> &H {
        &self.host
    }

    pub fn host_mut(&mut self) -> &mut H {
        &mut self.host
    }

    pub fn into_parts(self) -> (ClientCore<C, M>, H) {
        (self.core, self.host)
    }
}

impl<C, M, H> DesktopMessagingCore for RustDesktopMessagingCore<C, M, H>
where
    C: EnvelopeCrypto + Send,
    M: MlsEngine + Send,
    H: DesktopCoreHost<C, M>,
{
    fn user_id(&self) -> &str {
        self.core.user_id()
    }

    fn device_id(&self) -> &str {
        self.core.device_id()
    }

    fn durable_cursor(&self) -> Result<u64, CoreError> {
        let cursor = self.host.durable_cursor()?;
        if cursor > protocol::MAX_CURSOR {
            return Err(CoreError::InvalidSync);
        }
        Ok(cursor)
    }

    fn create_hello(
        &mut self,
        access_token: &str,
        last_seen_cursor: u64,
    ) -> Result<Vec<u8>, CoreError> {
        if access_token.is_empty()
            || access_token.len() > protocol::MAX_FRAME_BYTES
            || last_seen_cursor > protocol::MAX_CURSOR
        {
            return Err(CoreError::Authentication);
        }

        let frame = v1::ClientFrame {
            request_id: Uuid::new_v4().to_string(),
            body: Some(v1::client_frame::Body::Hello(v1::Hello {
                protocol_version: protocol::VERSION,
                device_id: self.core.device_id().to_owned(),
                device_access_token: access_token.as_bytes().to_vec(),
                last_seen_cursor,
                supported_sync_compression: vec![protocol::SYNC_COMPRESSION_ZSTD_DICTIONARY_V1],
            })),
        };
        protocol::validate_id(&frame.request_id)?;
        let mut bytes = Vec::with_capacity(frame.encoded_len());
        frame.encode(&mut bytes).map_err(|_| CoreError::Provider)?;
        validate_frame(&bytes)
    }

    fn handle_server_frame(
        &mut self,
        frame: &[u8],
        transport: &mut dyn DesktopFrameTransport,
        full_sync: bool,
        on_text_message: &mut dyn FnMut(DesktopReceivedTextMessage),
    ) -> Result<DesktopFrameResult, CoreError> {
        let frame = decode_server_frame(frame)?;
        self.host.handle_server_frame(
            &mut self.core,
            &frame,
            transport,
            full_sync,
            on_text_message,
        )
    }

    fn send_text(
        &mut self,
        conversation_id: &str,
        recipient_user_id: &str,
        text: &str,
        transport: &mut dyn DesktopFrameTransport,
    ) -> Result<(), CoreError> {
        protocol::validate_id(conversation_id)?;
        protocol::validate_id(recipient_user_id)?;
        if recipient_user_id == self.core.user_id()
            || text.is_empty()
            || text.len() > crate::session::DESKTOP_MAX_TEXT_BYTES
        {
            return Err(CoreError::Authentication);
        }
        self.host.send_text(
            &mut self.core,
            conversation_id,
            recipient_user_id,
            text,
            transport,
        )
    }
}

/// Bind a shared Rust `ClientCore` to the reconnecting desktop text session.
/// The returned session uses the supplied `DesktopSocketFactory`, including
/// the macOS TLS WebSocket adapter, and regenerates Hello from the current
/// in-memory bearer token and durable cursor on every connection attempt.
pub fn bind_desktop_text_session<C, M, H, F, P>(
    endpoint: impl Into<String>,
    core: ClientCore<C, M>,
    host: H,
    access_token: P,
    factory: F,
) -> Result<DesktopTextSession<RustDesktopMessagingCore<C, M, H>, F>, CoreError>
where
    C: EnvelopeCrypto + Send + 'static,
    M: MlsEngine + Send + 'static,
    H: DesktopCoreHost<C, M> + 'static,
    F: DesktopSocketFactory,
    P: Fn() -> Result<String, CoreError> + Send + Sync + 'static,
{
    DesktopTextSession::new(
        endpoint,
        RustDesktopMessagingCore::new(core, host),
        access_token,
        factory,
    )
}

fn decode_server_frame(bytes: &[u8]) -> Result<v1::ServerFrame, CoreError> {
    validate_frame(bytes)?;
    let frame = v1::ServerFrame::decode(bytes)
        .map_err(|_| CoreError::Protocol(protocol::ProtocolError::Malformed))?;
    protocol::validate_id(&frame.request_id)?;
    match frame.body.as_ref().ok_or(CoreError::InvalidSync)? {
        v1::server_frame::Body::Welcome(welcome) => {
            if welcome.protocol_version != protocol::VERSION {
                return Err(CoreError::Protocol(
                    protocol::ProtocolError::UnsupportedVersion,
                ));
            }
        }
        v1::server_frame::Body::Accepted(accepted) => {
            protocol::validate_id(&accepted.envelope_id)?;
        }
        v1::server_frame::Body::Batch(batch) => {
            protocol::validate_sync_batch(&batch)?;
        }
        v1::server_frame::Body::CompressedBatch(batch) => {
            protocol::decompress_sync_batch(&batch)?;
        }
        v1::server_frame::Body::Error(_) => {}
        v1::server_frame::Body::WebRtcSignal(delivery) => {
            protocol::validate_webrtc_signal_delivery(&delivery)?;
            if delivery.request_id != frame.request_id {
                return Err(CoreError::Authentication);
            }
        }
    }
    Ok(frame)
}

fn validate_frame(frame: &[u8]) -> Result<Vec<u8>, CoreError> {
    if frame.is_empty() {
        return Err(CoreError::Protocol(protocol::ProtocolError::Malformed));
    }
    if frame.len() > protocol::MAX_FRAME_BYTES {
        return Err(CoreError::Protocol(protocol::ProtocolError::TooLarge));
    }
    Ok(frame.to_vec())
}
