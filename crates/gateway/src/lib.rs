//! Multi-region WebSocket gateway core.
//!
//! A transport adapter owns the actual WebSocket socket and calls this crate
//! after enforcing the frame-size limit. This crate owns authentication,
//! session fencing, encrypted-mailbox durability, cross-region forwarding and
//! offline push wakeups. It never inspects sealed payload bytes.
use async_trait::async_trait;
use links_account_auth::{service::AccountAuth, AuthError};
use links_protocol::{self as protocol, v1};
use links_server_store::{
    ephemeral::{EphemeralState, SessionLease, MAX_SESSION_TTL_MS},
    payload::{AppendRequest, EncryptedPayloadStore, ReadRequest},
    StoreError,
};
use prost::Message;
use std::sync::Arc;
use thiserror::Error;
use uuid::Uuid;

pub const HELLO_DEADLINE_MS: u64 = 5_000;
pub const HEARTBEAT_INTERVAL_MS: u64 = 30_000;
pub const HEARTBEAT_TIMEOUT_MS: u64 = 90_000;
pub const APNS_BACKGROUND_PUSH_TYPE: &str = "background";
pub const APNS_BACKGROUND_PRIORITY: &str = "5";
pub const FCM_HIGH_PRIORITY: &str = "HIGH";
const MAX_ACCESS_TOKEN_BYTES: usize = 512;

#[derive(Debug, Error)]
pub enum GatewayError {
    #[error("invalid gateway request")]
    Invalid,
    #[error("gateway authentication failed")]
    Authentication,
    #[error("unsupported protocol version")]
    UnsupportedVersion,
    #[error("gateway dependency unavailable")]
    Unavailable,
    #[error("gateway conflict")]
    Conflict,
    #[error(transparent)]
    Protocol(#[from] protocol::ProtocolError),
}
impl From<StoreError> for GatewayError {
    fn from(error: StoreError) -> Self {
        match error {
            StoreError::Invalid | StoreError::Protocol(protocol::ProtocolError::Invalid(_)) => {
                Self::Invalid
            }
            StoreError::Protocol(protocol::ProtocolError::UnsupportedVersion) => {
                Self::UnsupportedVersion
            }
            StoreError::Protocol(_) => Self::Invalid,
            StoreError::Forbidden | StoreError::NotFound => Self::Authentication,
            StoreError::Conflict => Self::Conflict,
            _ => Self::Unavailable,
        }
    }
}

#[derive(Clone)]
pub struct GatewayConfig {
    pub gateway_id: String,
    pub region: String,
}
impl GatewayConfig {
    pub fn new(gateway_id: String, region: String) -> Result<Self, GatewayError> {
        if !valid_locator(&gateway_id) || !valid_locator(&region) {
            return Err(GatewayError::Invalid);
        }
        Ok(Self { gateway_id, region })
    }
}

#[derive(Clone, Copy)]
pub struct AuthenticatedDevice {
    pub user_id: Uuid,
    pub device_id: Uuid,
}

#[async_trait]
pub trait DeviceAuthenticator: Send + Sync {
    async fn authenticate(
        &self,
        device_id: Uuid,
        access_token: &[u8],
    ) -> Result<AuthenticatedDevice, GatewayError>;
}

/// Adapter for the existing bearer-token account service. The token is used
/// only for the authentication call and is never copied into a session lease.
pub struct AccountAuthDeviceAuthenticator {
    auth: Arc<AccountAuth>,
}
impl AccountAuthDeviceAuthenticator {
    pub fn new(auth: Arc<AccountAuth>) -> Self {
        Self { auth }
    }
}
#[async_trait]
impl DeviceAuthenticator for AccountAuthDeviceAuthenticator {
    async fn authenticate(
        &self,
        device_id: Uuid,
        access_token: &[u8],
    ) -> Result<AuthenticatedDevice, GatewayError> {
        let token = std::str::from_utf8(access_token).map_err(|_| GatewayError::Authentication)?;
        let account = self.auth.authenticate(token).await.map_err(auth_error)?;
        if account.device_id != device_id {
            return Err(GatewayError::Authentication);
        }
        Ok(AuthenticatedDevice {
            user_id: account.user_id,
            device_id: account.device_id,
        })
    }
}

fn auth_error(error: AuthError) -> GatewayError {
    match error {
        AuthError::Invalid | AuthError::Denied => GatewayError::Authentication,
        AuthError::Conflict => GatewayError::Conflict,
        AuthError::RateLimited | AuthError::Unavailable => GatewayError::Unavailable,
    }
}

#[derive(Clone)]
pub struct ForwardedEnvelope {
    pub envelope: v1::Envelope,
    pub cursor: u64,
}

#[derive(Clone)]
pub struct ForwardedWebRtcSignal {
    pub delivery: v1::WebRtcSignalDelivery,
}

#[async_trait]
pub trait RegionBus: Send + Sync {
    async fn forward(
        &self,
        destination_gateway_id: &str,
        delivery: ForwardedEnvelope,
    ) -> Result<(), GatewayError>;

    /// Forward transient SDP/ICE signaling. Unlike envelopes, signals are
    /// never appended to the durable mailbox and are dropped if the peer is
    /// offline.
    async fn forward_signal(
        &self,
        destination_gateway_id: &str,
        signal: ForwardedWebRtcSignal,
    ) -> Result<(), GatewayError> {
        let _ = (destination_gateway_id, signal);
        Err(GatewayError::Unavailable)
    }
}

#[derive(Clone)]
pub struct PushWakeup {
    pub recipient_device_id: String,
    pub cursor: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApnsSilentPush {
    pub push_type: &'static str,
    pub priority: &'static str,
    pub payload: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FcmSilentPush {
    pub priority: &'static str,
    pub data: Vec<(String, String)>,
}

impl PushWakeup {
    pub fn new(recipient_device_id: String, cursor: u64) -> Result<Self, GatewayError> {
        protocol::validate_id(&recipient_device_id)?;
        if cursor == 0 || cursor > protocol::MAX_CURSOR {
            return Err(GatewayError::Invalid);
        }
        Ok(Self {
            recipient_device_id,
            cursor,
        })
    }

    /// APNs adapter input. The adapter adds its device token and topic to the
    /// request; this body is background-only and contains no notification.
    pub fn apns_request(&self) -> Result<ApnsSilentPush, GatewayError> {
        self.validate()?;
        let payload = format!(
            "{{\"aps\":{{\"content-available\":1}},\"recipient_device_id\":\"{}\",\"cursor\":\"{}\"}}",
            self.recipient_device_id, self.cursor
        )
        .into_bytes();
        Ok(ApnsSilentPush {
            push_type: APNS_BACKGROUND_PUSH_TYPE,
            priority: APNS_BACKGROUND_PRIORITY,
            payload,
        })
    }

    /// FCM HTTP v1 adapter input. `notification` is intentionally absent, so
    /// this is data-only and wakes the client to replay its mailbox.
    pub fn fcm_request(&self) -> Result<FcmSilentPush, GatewayError> {
        self.validate()?;
        Ok(FcmSilentPush {
            priority: FCM_HIGH_PRIORITY,
            data: vec![
                (
                    "recipient_device_id".into(),
                    self.recipient_device_id.clone(),
                ),
                ("cursor".into(), self.cursor.to_string()),
            ],
        })
    }

    fn validate(&self) -> Result<(), GatewayError> {
        Self::new(self.recipient_device_id.clone(), self.cursor).map(|_| ())
    }
}

#[async_trait]
pub trait PushNotifier: Send + Sync {
    /// Send a silent/data-only wakeup. Never include ciphertext, sender,
    /// conversation IDs, phone numbers or access tokens in the push payload.
    async fn notify(&self, wakeup: PushWakeup) -> Result<(), GatewayError>;
}

pub enum GatewayAction {
    Server(v1::ServerFrame),
    LocalDelivery {
        lease: SessionLease,
        delivery: ForwardedEnvelope,
    },
    LocalWebRtcSignal {
        lease: SessionLease,
        delivery: v1::WebRtcSignalDelivery,
    },
}

pub struct GatewaySession {
    user_id: Uuid,
    device_id: Uuid,
    session_id: String,
    last_ack_cursor: u64,
    sync_compression: bool,
}
impl GatewaySession {
    pub fn user_id(&self) -> Uuid {
        self.user_id
    }
    pub fn device_id(&self) -> Uuid {
        self.device_id
    }
    pub fn session_id(&self) -> &str {
        &self.session_id
    }
    pub fn last_ack_cursor(&self) -> u64 {
        self.last_ack_cursor
    }
    pub fn uses_sync_compression(&self) -> bool {
        self.sync_compression
    }
}

pub struct Gateway<S, Q, A, B, P> {
    config: GatewayConfig,
    state: Arc<S>,
    queue: Arc<Q>,
    authenticator: Arc<A>,
    bus: Arc<B>,
    push: Arc<P>,
}

impl<S, Q, A, B, P> Gateway<S, Q, A, B, P>
where
    S: EphemeralState + 'static,
    Q: EncryptedPayloadStore + 'static,
    A: DeviceAuthenticator + 'static,
    B: RegionBus + 'static,
    P: PushNotifier + 'static,
{
    pub fn new(
        config: GatewayConfig,
        state: Arc<S>,
        queue: Arc<Q>,
        authenticator: Arc<A>,
        bus: Arc<B>,
        push: Arc<P>,
    ) -> Self {
        Self {
            config,
            state,
            queue,
            authenticator,
            bus,
            push,
        }
    }

    /// The first frame must be Hello. A successful bind replaces any older
    /// session for this device, fencing that old socket in every region.
    pub async fn open(
        &self,
        frame: v1::ClientFrame,
        now_ms: u64,
    ) -> Result<(GatewaySession, Vec<GatewayAction>), GatewayError> {
        validate_request(&frame)?;
        let request_id = frame.request_id.clone();
        let Some(v1::client_frame::Body::Hello(hello)) = frame.body else {
            return Err(GatewayError::Authentication);
        };
        if hello.protocol_version != protocol::VERSION {
            return Err(GatewayError::UnsupportedVersion);
        }
        let sync_compression = hello
            .supported_sync_compression
            .contains(&protocol::SYNC_COMPRESSION_ZSTD_DICTIONARY_V1);
        let device_id = Uuid::parse_str(&hello.device_id).map_err(|_| GatewayError::Invalid)?;
        let authenticated = self
            .authenticator
            .authenticate(device_id, &hello.device_access_token)
            .await?;
        if authenticated.device_id != device_id || authenticated.device_id.is_nil() {
            return Err(GatewayError::Authentication);
        }
        let session_id = Uuid::new_v4().to_string();
        let expires_at_ms = now_ms
            .checked_add(MAX_SESSION_TTL_MS)
            .ok_or(GatewayError::Unavailable)?;
        self.state
            .bind(
                SessionLease {
                    device_id: hello.device_id.clone(),
                    session_id: session_id.clone(),
                    gateway_id: self.config.gateway_id.clone(),
                    expires_at_ms,
                },
                now_ms,
            )
            .await?;
        let batch = match self
            .queue
            .read(ReadRequest::new(
                hello.device_id.clone(),
                hello.last_seen_cursor,
                protocol::MAX_BATCH_ITEMS as u32,
                now_ms,
            )?)
            .await
        {
            Ok(batch) => batch,
            Err(error) => {
                let _ = self.state.unbind(&hello.device_id, &session_id).await;
                return Err(error.into());
            }
        };
        let mut actions = vec![GatewayAction::Server(v1::ServerFrame {
            request_id: request_id.clone(),
            body: Some(v1::server_frame::Body::Welcome(v1::Welcome {
                protocol_version: protocol::VERSION,
                heartbeat_seconds: (HEARTBEAT_INTERVAL_MS / 1000) as u32,
            })),
        })];
        if !batch.items.is_empty() {
            actions.push(sync_batch_action(request_id, batch, sync_compression)?);
        }
        Ok((
            GatewaySession {
                user_id: authenticated.user_id,
                device_id,
                session_id,
                last_ack_cursor: hello.last_seen_cursor,
                sync_compression,
            },
            actions,
        ))
    }

    pub async fn renew(&self, session: &GatewaySession, now_ms: u64) -> Result<bool, GatewayError> {
        let expires_at_ms = now_ms
            .checked_add(MAX_SESSION_TTL_MS)
            .ok_or(GatewayError::Unavailable)?;
        Ok(self
            .state
            .renew(
                &session.device_id.to_string(),
                &session.session_id,
                expires_at_ms,
                now_ms,
            )
            .await?)
    }

    pub async fn close(&self, session: &GatewaySession) -> Result<bool, GatewayError> {
        Ok(self
            .state
            .unbind(&session.device_id.to_string(), &session.session_id)
            .await?)
    }

    /// Deliver an envelope received from another region. The bus adapter must
    /// authenticate the sender gateway before calling this method. No mailbox
    /// append occurs here because the origin gateway already committed it.
    pub async fn handle_forwarded(
        &self,
        delivery: ForwardedEnvelope,
        now_ms: u64,
    ) -> Result<Option<GatewayAction>, GatewayError> {
        protocol::validate_envelope(&delivery.envelope)?;
        if delivery.envelope.expires_at_ms <= now_ms {
            return Ok(None);
        }
        let recipient_device_id = delivery.envelope.recipient_device_id.clone();
        let Some(lease) = self.state.route(&recipient_device_id, now_ms).await? else {
            self.push
                .notify(PushWakeup::new(recipient_device_id, delivery.cursor)?)
                .await?;
            return Ok(None);
        };
        if lease.gateway_id == self.config.gateway_id {
            Ok(Some(GatewayAction::LocalDelivery { lease, delivery }))
        } else {
            self.bus.forward(&lease.gateway_id, delivery).await?;
            Ok(None)
        }
    }

    /// Deliver transient SDP/ICE signaling to a currently connected peer.
    /// Signaling is intentionally not queued: the WebRTC session retries or
    /// fails through its own timeout when the target is offline.
    pub async fn handle_forwarded_signal(
        &self,
        forwarded: ForwardedWebRtcSignal,
        now_ms: u64,
    ) -> Result<Option<GatewayAction>, GatewayError> {
        protocol::validate_webrtc_signal_delivery(&forwarded.delivery)?;
        let target_device_id = forwarded
            .delivery
            .signal
            .as_ref()
            .ok_or(GatewayError::Invalid)?
            .target_device_id
            .clone();
        let Some(lease) = self.state.route(&target_device_id, now_ms).await? else {
            return Ok(None);
        };
        if lease.gateway_id == self.config.gateway_id {
            Ok(Some(GatewayAction::LocalWebRtcSignal {
                lease,
                delivery: forwarded.delivery,
            }))
        } else {
            self.bus
                .forward_signal(&lease.gateway_id, forwarded)
                .await?;
            Ok(None)
        }
    }

    /// Process one already-decoded frame. The socket adapter sends Server
    /// actions and writes LocalDelivery to the local socket. A failed forward
    /// or push leaves the durable mailbox entry intact for replay.
    pub async fn handle(
        &self,
        session: &mut GatewaySession,
        frame: v1::ClientFrame,
        now_ms: u64,
    ) -> Result<Vec<GatewayAction>, GatewayError> {
        let active = self
            .state
            .route(&session.device_id.to_string(), now_ms)
            .await?
            .is_some_and(|lease| {
                lease.session_id == session.session_id && lease.gateway_id == self.config.gateway_id
            });
        if !active {
            return Err(GatewayError::Authentication);
        }
        validate_request(&frame)?;
        let request_id = frame.request_id.clone();
        match frame.body.ok_or(GatewayError::Invalid)? {
            v1::client_frame::Body::Hello(_) => Err(GatewayError::Authentication),
            v1::client_frame::Body::Send(envelope) => {
                let recipient_device_id = envelope.recipient_device_id.clone();
                let request = AppendRequest::new(envelope.clone(), now_ms)?;
                let append = self.queue.append(request).await?;
                let delivery = ForwardedEnvelope {
                    envelope,
                    cursor: append.cursor,
                };
                let accepted = accepted(request_id, &delivery.envelope);
                if let Some(lease) = self.state.route(&recipient_device_id, now_ms).await? {
                    if lease.gateway_id == self.config.gateway_id {
                        Ok(vec![
                            GatewayAction::Server(accepted),
                            GatewayAction::LocalDelivery { lease, delivery },
                        ])
                    } else {
                        self.bus.forward(&lease.gateway_id, delivery).await?;
                        Ok(vec![GatewayAction::Server(accepted)])
                    }
                } else {
                    self.push
                        .notify(PushWakeup::new(recipient_device_id, append.cursor)?)
                        .await?;
                    Ok(vec![GatewayAction::Server(accepted)])
                }
            }
            v1::client_frame::Body::Replay(replay) => {
                if replay.after_cursor > protocol::MAX_CURSOR
                    || replay.limit == 0
                    || replay.limit as usize > protocol::MAX_BATCH_ITEMS
                {
                    return Err(GatewayError::Invalid);
                }
                let batch = self
                    .queue
                    .read(ReadRequest::new(
                        session.device_id.to_string(),
                        replay.after_cursor,
                        replay.limit,
                        now_ms,
                    )?)
                    .await?;
                if batch.encoded_len() > protocol::MAX_FRAME_BYTES - 128 {
                    return Err(GatewayError::Invalid);
                }
                Ok(vec![sync_batch_action(
                    request_id,
                    batch,
                    session.sync_compression,
                )?])
            }
            v1::client_frame::Body::Ack(ack) => {
                if ack.through_cursor > protocol::MAX_CURSOR
                    || ack.through_cursor < session.last_ack_cursor
                {
                    return Err(GatewayError::Invalid);
                }
                self.queue
                    .acknowledge(&session.device_id.to_string(), ack.through_cursor, now_ms)
                    .await?;
                session.last_ack_cursor = ack.through_cursor;
                Ok(Vec::new())
            }
            v1::client_frame::Body::WebRtcSignal(signal) => {
                let target_device_id = signal.target_device_id.clone();
                let delivery = v1::WebRtcSignalDelivery {
                    request_id,
                    sender_device_id: session.device_id.to_string(),
                    signal: Some(signal),
                };
                protocol::validate_webrtc_signal_delivery(&delivery)?;
                let Some(lease) = self.state.route(&target_device_id, now_ms).await? else {
                    return Ok(vec![temporary_unavailable(delivery.request_id)]);
                };
                let forwarded = ForwardedWebRtcSignal { delivery };
                if lease.gateway_id == self.config.gateway_id {
                    Ok(vec![GatewayAction::LocalWebRtcSignal {
                        lease,
                        delivery: forwarded.delivery,
                    }])
                } else {
                    self.bus
                        .forward_signal(&lease.gateway_id, forwarded)
                        .await?;
                    Ok(Vec::new())
                }
            }
        }
    }
}

pub fn decode_client_frame(bytes: &[u8]) -> Result<v1::ClientFrame, GatewayError> {
    if bytes.is_empty() || bytes.len() > protocol::MAX_FRAME_BYTES {
        return Err(GatewayError::Invalid);
    }
    let frame = v1::ClientFrame::decode(bytes).map_err(|_| GatewayError::Invalid)?;
    validate_request(&frame)?;
    Ok(frame)
}

pub fn encode_server_frame(frame: &v1::ServerFrame) -> Result<Vec<u8>, GatewayError> {
    protocol::validate_id(&frame.request_id)?;
    if let Some(v1::server_frame::Body::CompressedBatch(batch)) = frame.body.as_ref() {
        protocol::decompress_sync_batch(batch)?;
    }
    if let Some(v1::server_frame::Body::WebRtcSignal(delivery)) = frame.body.as_ref() {
        protocol::validate_webrtc_signal_delivery(delivery)?;
    }
    if frame.encoded_len() > protocol::MAX_FRAME_BYTES {
        return Err(GatewayError::Invalid);
    }
    let mut bytes = Vec::with_capacity(frame.encoded_len());
    frame
        .encode(&mut bytes)
        .map_err(|_| GatewayError::Unavailable)?;
    Ok(bytes)
}

fn validate_request(frame: &v1::ClientFrame) -> Result<(), GatewayError> {
    protocol::validate_id(&frame.request_id)?;
    match frame.body.as_ref().ok_or(GatewayError::Invalid)? {
        v1::client_frame::Body::Hello(hello) => {
            protocol::validate_id(&hello.device_id)?;
            if hello.protocol_version != protocol::VERSION
                || hello.device_access_token.is_empty()
                || hello.device_access_token.len() > MAX_ACCESS_TOKEN_BYTES
                || hello.last_seen_cursor > protocol::MAX_CURSOR
                || hello.supported_sync_compression.iter().any(|compression| {
                    *compression != 0
                        && *compression != protocol::SYNC_COMPRESSION_ZSTD_DICTIONARY_V1
                })
            {
                return Err(if hello.protocol_version != protocol::VERSION {
                    GatewayError::UnsupportedVersion
                } else {
                    GatewayError::Invalid
                });
            }
        }
        v1::client_frame::Body::Send(envelope) => protocol::validate_envelope(envelope)?,
        v1::client_frame::Body::Replay(replay) => {
            if replay.after_cursor > protocol::MAX_CURSOR
                || replay.limit == 0
                || replay.limit as usize > protocol::MAX_BATCH_ITEMS
            {
                return Err(GatewayError::Invalid);
            }
        }
        v1::client_frame::Body::Ack(ack) => {
            if ack.through_cursor > protocol::MAX_CURSOR {
                return Err(GatewayError::Invalid);
            }
        }
        v1::client_frame::Body::WebRtcSignal(signal) => {
            protocol::validate_webrtc_signal(signal)?;
        }
    }
    Ok(())
}

fn accepted(request_id: String, envelope: &v1::Envelope) -> v1::ServerFrame {
    v1::ServerFrame {
        request_id,
        body: Some(v1::server_frame::Body::Accepted(v1::Accepted {
            envelope_id: envelope.envelope_id.clone(),
        })),
    }
}

fn temporary_unavailable(request_id: String) -> GatewayAction {
    GatewayAction::Server(v1::ServerFrame {
        request_id,
        body: Some(v1::server_frame::Body::Error(v1::ProtocolError {
            code: 6,
            retry_after_ms: 1_000,
        })),
    })
}

fn sync_batch_action(
    request_id: String,
    batch: v1::SyncBatch,
    use_compression: bool,
) -> Result<GatewayAction, GatewayError> {
    protocol::validate_sync_batch(&batch)?;
    let plain = v1::ServerFrame {
        request_id: request_id.clone(),
        body: Some(v1::server_frame::Body::Batch(batch.clone())),
    };
    if !use_compression {
        return Ok(GatewayAction::Server(plain));
    }

    let compressed = match protocol::compress_sync_batch(&batch) {
        Ok(compressed) => compressed,
        Err(protocol::ProtocolError::TooLarge) => return Ok(GatewayAction::Server(plain)),
        Err(error) => return Err(error.into()),
    };
    let compressed_frame = v1::ServerFrame {
        request_id,
        body: Some(v1::server_frame::Body::CompressedBatch(compressed)),
    };
    if compressed_frame.encoded_len() < plain.encoded_len() {
        Ok(GatewayAction::Server(compressed_frame))
    } else {
        Ok(GatewayAction::Server(plain))
    }
}

fn valid_locator(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b':' | b'_' | b'-' | b'.'))
}
