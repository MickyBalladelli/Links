//! Axum WebSocket adapter for the gateway core.
//!
//! The adapter owns socket protocol details only. It accepts one binary
//! protobuf frame at a time, routes local delivery actions to the connection
//! named by the gateway lease, and leaves authentication, queue durability,
//! and frame validation to [`Gateway`].
use crate::{
    decode_client_frame, encode_server_frame, DeviceAuthenticator, ForwardedEnvelope, Gateway,
    GatewayAction, GatewayError, GatewaySession, PushNotifier, RegionBus,
};
use axum::{
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::any,
    Router,
};
use links_protocol::{self as protocol, v1};
use links_server_store::{ephemeral::SessionLease, payload::EncryptedPayloadStore};
use std::{collections::HashMap, io, sync::Arc, time::Duration};
use tokio::{
    net::TcpListener,
    sync::{mpsc, Mutex},
    time::timeout,
};

pub const WEBSOCKET_PATH: &str = "/v1/connect";
pub const WEBSOCKET_SUBPROTOCOL: &str = "links.v1";
const OUTBOUND_CAPACITY: usize = 64;

type SharedConnections = Arc<Mutex<HashMap<String, mpsc::Sender<Outbound>>>>;

pub struct WebSocketAdapter<S, Q, A, B, P> {
    gateway: Arc<Gateway<S, Q, A, B, P>>,
    connections: SharedConnections,
}

impl<S, Q, A, B, P> Clone for WebSocketAdapter<S, Q, A, B, P> {
    fn clone(&self) -> Self {
        Self {
            gateway: Arc::clone(&self.gateway),
            connections: Arc::clone(&self.connections),
        }
    }
}

enum Outbound {
    Frame(v1::ServerFrame),
    Delivery {
        lease: SessionLease,
        delivery: ForwardedEnvelope,
        now_ms: u64,
    },
    WebRtcSignal {
        lease: SessionLease,
        delivery: v1::WebRtcSignalDelivery,
    },
}

impl<S, Q, A, B, P> WebSocketAdapter<S, Q, A, B, P>
where
    S: links_server_store::ephemeral::EphemeralState + 'static,
    Q: EncryptedPayloadStore + 'static,
    A: DeviceAuthenticator + 'static,
    B: RegionBus + 'static,
    P: PushNotifier + 'static,
{
    pub fn new(gateway: Arc<Gateway<S, Q, A, B, P>>) -> Self {
        Self {
            gateway,
            connections: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn router(&self) -> Router {
        let adapter = self.clone();
        Router::new().route(
            WEBSOCKET_PATH,
            any(move |ws: WebSocketUpgrade| {
                let adapter = adapter.clone();
                async move { upgrade(adapter, ws).await }
            }),
        )
    }

    pub async fn serve(self, listener: TcpListener) -> Result<(), io::Error> {
        axum::serve(listener, self.router()).await
    }

    pub async fn serve_until_shutdown(self, listener: TcpListener) -> Result<(), io::Error> {
        axum::serve(listener, self.router())
            .with_graceful_shutdown(async {
                let _ = tokio::signal::ctrl_c().await;
            })
            .await
    }

    /// Give a region-bus consumer a safe entry point for forwarded local
    /// deliveries. The delivery is routed to the active socket lease.
    pub async fn handle_forwarded(
        &self,
        delivery: ForwardedEnvelope,
        now_ms: u64,
    ) -> Result<(), GatewayError> {
        if let Some(action) = self.gateway.handle_forwarded(delivery, now_ms).await? {
            self.dispatch_actions("", vec![action], now_ms).await?;
        }
        Ok(())
    }

    async fn upgrade_socket(&self, socket: WebSocket) {
        let (sender, mut receiver) = mpsc::channel(OUTBOUND_CAPACITY);
        let mut socket = socket;
        let first = match timeout(
            Duration::from_millis(crate::HELLO_DEADLINE_MS),
            socket.recv(),
        )
        .await
        {
            Ok(Some(Ok(message))) => message,
            _ => return,
        };
        let Message::Binary(bytes) = first else {
            return;
        };
        let frame = match decode_client_frame(&bytes) {
            Ok(frame) => frame,
            Err(_) => return,
        };
        let opened_at_ms = now_ms();
        let (mut session, actions) = match self.gateway.open(frame, opened_at_ms).await {
            Ok(result) => result,
            Err(_) => return,
        };
        let session_id = session.session_id().to_owned();
        self.connections
            .lock()
            .await
            .insert(session_id.clone(), sender.clone());

        if self
            .dispatch_actions(&session_id, actions, opened_at_ms)
            .await
            .is_err()
        {
            self.unregister(&session_id).await;
            let _ = self.gateway.close(&session).await;
            return;
        }

        let mut heartbeat =
            tokio::time::interval(Duration::from_millis(crate::HEARTBEAT_INTERVAL_MS));
        heartbeat.tick().await;
        loop {
            tokio::select! {
                _ = heartbeat.tick() => {
                    if !self.gateway.renew(&session, now_ms()).await.unwrap_or(false) {
                        break;
                    }
                    if socket.send(Message::Ping(Vec::new().into())).await.is_err() {
                        break;
                    }
                }
                incoming = socket.recv() => {
                    let Some(Ok(message)) = incoming else { break };
                    match message {
                        Message::Binary(bytes) => {
                            let frame = match decode_client_frame(&bytes) {
                                Ok(frame) => frame,
                                Err(_) => break,
                            };
                            let now = now_ms();
                            let actions = match self.gateway.handle(&mut session, frame, now).await {
                                Ok(actions) => actions,
                                Err(_) => break,
                            };
                            if self.dispatch_actions(&session_id, actions, now).await.is_err() {
                                break;
                            }
                        }
                        Message::Text(_) => break,
                        Message::Close(_) => break,
                        Message::Ping(_) | Message::Pong(_) => {
                            if !self.gateway.renew(&session, now_ms()).await.unwrap_or(false) {
                                break;
                            }
                        }
                    }
                }
                outbound = receiver.recv() => {
                    let Some(outbound) = outbound else { break };
                    let frame = match self.resolve_outbound(&session, outbound).await {
                        Ok(Some(frame)) => frame,
                        Ok(None) => continue,
                        Err(_) => break,
                    };
                    let bytes = match encode_server_frame(&frame) {
                        Ok(bytes) => bytes,
                        Err(_) => break,
                    };
                    if socket.send(Message::Binary(bytes.into())).await.is_err() {
                        break;
                    }
                }
            }
        }
        self.unregister(&session_id).await;
        let _ = self.gateway.close(&session).await;
    }

    async fn resolve_outbound(
        &self,
        session: &GatewaySession,
        outbound: Outbound,
    ) -> Result<Option<v1::ServerFrame>, GatewayError> {
        match outbound {
            Outbound::Frame(frame) => Ok(Some(frame)),
            Outbound::Delivery {
                lease,
                delivery,
                now_ms,
            } => {
                self.gateway
                    .local_delivery(session, lease, delivery, now_ms)
                    .await
            }
            Outbound::WebRtcSignal { lease, delivery } => {
                if lease.session_id != session.session_id()
                    || lease.device_id != session.device_id().to_string()
                {
                    return Err(GatewayError::Authentication);
                }
                protocol::validate_webrtc_signal_delivery(&delivery)?;
                let signal = delivery.signal.as_ref().ok_or(GatewayError::Invalid)?;
                if signal.target_device_id != session.device_id().to_string() {
                    return Err(GatewayError::Authentication);
                }
                let request_id = delivery.request_id.clone();
                Ok(Some(v1::ServerFrame {
                    request_id,
                    body: Some(v1::server_frame::Body::WebRtcSignal(delivery)),
                }))
            }
        }
    }

    async fn dispatch_actions(
        &self,
        current_session_id: &str,
        actions: Vec<GatewayAction>,
        now_ms: u64,
    ) -> Result<(), GatewayError> {
        for action in actions {
            match action {
                GatewayAction::Server(frame) => {
                    self.send_to(current_session_id, Outbound::Frame(frame))
                        .await?;
                }
                GatewayAction::LocalDelivery { lease, delivery } => {
                    let target_session_id = lease.session_id.clone();
                    self.send_to(
                        &target_session_id,
                        Outbound::Delivery {
                            lease,
                            delivery,
                            now_ms,
                        },
                    )
                    .await?;
                }
                GatewayAction::LocalWebRtcSignal { lease, delivery } => {
                    let target_session_id = lease.session_id.clone();
                    self.send_to(
                        &target_session_id,
                        Outbound::WebRtcSignal { lease, delivery },
                    )
                    .await?;
                }
            }
        }
        Ok(())
    }

    async fn send_to(&self, session_id: &str, outbound: Outbound) -> Result<(), GatewayError> {
        let sender = self
            .connections
            .lock()
            .await
            .get(session_id)
            .cloned()
            .ok_or(GatewayError::Unavailable)?;
        sender
            .send(outbound)
            .await
            .map_err(|_| GatewayError::Unavailable)
    }

    async fn unregister(&self, session_id: &str) {
        self.connections.lock().await.remove(session_id);
    }
}

async fn upgrade<S, Q, A, B, P>(
    adapter: WebSocketAdapter<S, Q, A, B, P>,
    ws: WebSocketUpgrade,
) -> Response
where
    S: links_server_store::ephemeral::EphemeralState + 'static,
    Q: EncryptedPayloadStore + 'static,
    A: DeviceAuthenticator + 'static,
    B: RegionBus + 'static,
    P: PushNotifier + 'static,
{
    if !ws
        .requested_protocols()
        .any(|protocol| protocol.as_bytes() == WEBSOCKET_SUBPROTOCOL.as_bytes())
    {
        return StatusCode::BAD_REQUEST.into_response();
    }
    ws.protocols([WEBSOCKET_SUBPROTOCOL])
        .max_message_size(protocol::MAX_FRAME_BYTES)
        .max_frame_size(protocol::MAX_FRAME_BYTES)
        .on_upgrade(move |socket| async move { adapter.upgrade_socket(socket).await })
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| {
            duration.as_millis().min(u128::from(u64::MAX)) as u64
        })
}
