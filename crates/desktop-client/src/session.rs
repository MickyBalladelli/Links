use links_client_core::{protocol, CoreError};
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

pub const DESKTOP_HEARTBEAT_INTERVAL_MS: u64 = 30_000;
pub const DESKTOP_INITIAL_BACKOFF_MS: u64 = 1_000;
pub const DESKTOP_MAX_BACKOFF_MS: u64 = 30_000;
pub const DESKTOP_STABLE_CONNECTION_MS: u64 = 30_000;
pub const DESKTOP_MAX_FRAME_BYTES: usize = protocol::MAX_FRAME_BYTES;
pub const DESKTOP_MAX_TEXT_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DesktopConnectionState {
    Stopped,
    Connecting,
    Ready,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DesktopFrameResult {
    Pending,
    RecoveryComplete,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesktopReceivedTextMessage {
    pub conversation_id: String,
    pub sender_device_id: String,
    pub text: String,
    pub sequence_id: u64,
    pub sent_at_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DesktopEvent {
    State(DesktopConnectionState),
    Text(DesktopReceivedTextMessage),
    Failure,
}

/// Transport used by the shared core for opaque protobuf frames.
pub trait DesktopFrameTransport {
    fn send(&mut self, frame: &[u8]) -> Result<(), CoreError>;
}

/// Native socket boundary. The implementation must provide TLS, the binary
/// `links.v1` subprotocol, Hello authentication, and WebSocket ping/pong.
pub trait DesktopSocket: Send {
    fn send_binary(&mut self, frame: &[u8]) -> Result<(), CoreError>;
    fn receive_binary(&mut self) -> Result<Option<Vec<u8>>, CoreError>;
    fn ping(&mut self) -> Result<(), CoreError>;
    fn close(&mut self);
}

/// Desktop network adapters implement this boundary with a native WebSocket
/// library. `connect` receives a fresh core-produced Hello on every attempt.
pub trait DesktopSocketFactory: Send {
    type Socket: DesktopSocket;

    fn connect(&mut self, endpoint: &str, hello: &[u8]) -> Result<Self::Socket, CoreError>;
}

/// Reconnecting desktop transport with bounded full-jitter backoff.
pub struct DesktopConnectionManager<F: DesktopSocketFactory> {
    endpoint: String,
    hello_provider: Box<dyn FnMut() -> Result<Vec<u8>, CoreError> + Send>,
    factory: F,
    socket: Option<F::Socket>,
    state: DesktopConnectionState,
    state_events: VecDeque<DesktopConnectionState>,
    started: bool,
    backoff_ms: u64,
    next_retry_at: Option<Instant>,
    connected_at: Option<Instant>,
    last_heartbeat_at: Option<Instant>,
}

impl<F: DesktopSocketFactory> DesktopConnectionManager<F> {
    pub fn new<P>(
        endpoint: impl Into<String>,
        hello_provider: P,
        factory: F,
    ) -> Result<Self, CoreError>
    where
        P: FnMut() -> Result<Vec<u8>, CoreError> + Send + 'static,
    {
        let endpoint = validate_endpoint(endpoint.into())?;
        Ok(Self {
            endpoint,
            hello_provider: Box::new(hello_provider),
            factory,
            socket: None,
            state: DesktopConnectionState::Stopped,
            state_events: VecDeque::new(),
            started: false,
            backoff_ms: DESKTOP_INITIAL_BACKOFF_MS,
            next_retry_at: None,
            connected_at: None,
            last_heartbeat_at: None,
        })
    }

    pub fn state(&self) -> DesktopConnectionState {
        self.state
    }

    pub fn is_connected(&self) -> bool {
        self.started && self.socket.is_some() && self.state == DesktopConnectionState::Ready
    }

    pub fn start(&mut self, now: Instant) {
        if self.started {
            return;
        }
        self.started = true;
        self.backoff_ms = DESKTOP_INITIAL_BACKOFF_MS;
        self.set_state(DesktopConnectionState::Connecting);
        self.connect_now(now);
    }

    pub fn stop(&mut self) {
        self.started = false;
        self.next_retry_at = None;
        self.connected_at = None;
        self.last_heartbeat_at = None;
        if let Some(mut socket) = self.socket.take() {
            socket.close();
        }
        self.set_state(DesktopConnectionState::Stopped);
    }

    /// Advance retry and heartbeat timers. The desktop event loop should call
    /// this regularly, without blocking on network I/O.
    pub fn poll(&mut self, now: Instant) {
        if !self.started {
            return;
        }
        if self.socket.is_none() {
            if self.next_retry_at.is_some_and(|retry| now >= retry) {
                self.connect_now(now);
            }
            return;
        }

        if self
            .connected_at
            .is_some_and(|connected| now.saturating_duration_since(connected)
                >= Duration::from_millis(DESKTOP_STABLE_CONNECTION_MS))
        {
            self.backoff_ms = DESKTOP_INITIAL_BACKOFF_MS;
        }

        let heartbeat_due = self.last_heartbeat_at.is_some_and(|last| {
            now.saturating_duration_since(last)
                >= Duration::from_millis(DESKTOP_HEARTBEAT_INTERVAL_MS)
        });
        if heartbeat_due {
            let result = self.socket.as_mut().map_or(Err(CoreError::Provider), |socket| {
                socket.ping()
            });
            match result {
                Ok(()) => self.last_heartbeat_at = Some(now),
                Err(_) => self.connection_failed(now),
            }
        }
    }

    pub fn take_state_events(&mut self) -> Vec<DesktopConnectionState> {
        self.state_events.drain(..).collect()
    }

    pub fn send(&mut self, now: Instant, frame: &[u8]) -> Result<(), CoreError> {
        validate_frame(frame)?;
        if !self.is_connected() {
            return Err(CoreError::Provider);
        }
        let result = self
            .socket
            .as_mut()
            .ok_or(CoreError::Provider)?
            .send_binary(frame);
        if let Err(error) = result {
            self.connection_failed(now);
            return Err(error);
        }
        Ok(())
    }

    pub fn receive(&mut self, now: Instant) -> Result<Option<Vec<u8>>, CoreError> {
        if !self.is_connected() {
            return Ok(None);
        }
        let result = self
            .socket
            .as_mut()
            .ok_or(CoreError::Provider)?
            .receive_binary();
        let frame = match result {
            Ok(frame) => frame,
            Err(error) => {
                self.connection_failed(now);
                return Err(error);
            }
        };
        if let Some(frame) = &frame {
            validate_frame(frame).inspect_err(|_| self.connection_failed(now))?;
        }
        Ok(frame)
    }

    fn connect_now(&mut self, now: Instant) {
        self.next_retry_at = None;
        let hello = match (self.hello_provider)() {
            Ok(hello) if validate_frame(&hello).is_ok() => hello,
            Ok(_) | Err(_) => {
                self.connection_failed(now);
                return;
            }
        };
        match self.factory.connect(&self.endpoint, &hello) {
            Ok(socket) => {
                self.socket = Some(socket);
                self.connected_at = Some(now);
                self.last_heartbeat_at = Some(now);
                self.set_state(DesktopConnectionState::Ready);
            }
            Err(_) => self.connection_failed(now),
        }
    }

    fn connection_failed(&mut self, now: Instant) {
        if let Some(mut socket) = self.socket.take() {
            socket.close();
        }
        self.connected_at = None;
        self.last_heartbeat_at = None;
        if self.started {
            self.set_state(DesktopConnectionState::Failed);
            self.schedule_retry(now);
        } else {
            self.set_state(DesktopConnectionState::Stopped);
        }
    }

    fn schedule_retry(&mut self, now: Instant) {
        let ceiling = self.backoff_ms.min(DESKTOP_MAX_BACKOFF_MS);
        let delay = jitter_ms(ceiling);
        self.next_retry_at = Some(now + Duration::from_millis(delay));
        self.backoff_ms = self
            .backoff_ms
            .saturating_mul(2)
            .clamp(DESKTOP_INITIAL_BACKOFF_MS, DESKTOP_MAX_BACKOFF_MS);
    }

    fn set_state(&mut self, state: DesktopConnectionState) {
        if self.state == state {
            return;
        }
        self.state = state;
        self.state_events.push_back(state);
    }
}

/// Core boundary used by the desktop transport and UI shell.
pub trait DesktopMessagingCore: Send {
    fn user_id(&self) -> &str;
    fn device_id(&self) -> &str;
    fn durable_cursor(&self) -> Result<u64, CoreError>;
    fn create_hello(&mut self, access_token: &str, last_seen_cursor: u64)
        -> Result<Vec<u8>, CoreError>;
    /// Invoke the callback only for messages after local inbox/cursor commit.
    fn handle_server_frame(
        &mut self,
        frame: &[u8],
        transport: &mut dyn DesktopFrameTransport,
        full_sync: bool,
        on_text_message: &mut dyn FnMut(DesktopReceivedTextMessage),
    ) -> Result<DesktopFrameResult, CoreError>;
    /// Persist encrypted outbox state before returning success.
    fn send_text(
        &mut self,
        conversation_id: &str,
        recipient_user_id: &str,
        text: &str,
        transport: &mut dyn DesktopFrameTransport,
    ) -> Result<(), CoreError>;
}

pub type DesktopAccessTokenProvider = Arc<dyn Fn() -> Result<String, CoreError> + Send + Sync>;

/// Desktop text/session host. It owns no crypto: the injected core owns MLS,
/// Sealed Sender, durable inbox/outbox state, cursors, and acknowledgements.
pub struct DesktopTextSession<C, F: DesktopSocketFactory> {
    core: Arc<Mutex<C>>,
    manager: DesktopConnectionManager<F>,
}

impl<C: DesktopMessagingCore + 'static, F: DesktopSocketFactory> DesktopTextSession<C, F> {
    pub fn new<P>(
        endpoint: impl Into<String>,
        core: C,
        access_token: P,
        factory: F,
    ) -> Result<Self, CoreError>
    where
        P: Fn() -> Result<String, CoreError> + Send + Sync + 'static,
    {
        let core = Arc::new(Mutex::new(core));
        let access_token: DesktopAccessTokenProvider = Arc::new(access_token);
        let hello_core = Arc::clone(&core);
        let hello_token = Arc::clone(&access_token);
        let hello_provider = move || {
            let token = hello_token()?;
            if token.is_empty() {
                return Err(CoreError::Authentication);
            }
            let mut core = hello_core.lock().map_err(|_| CoreError::Provider)?;
            let cursor = core.durable_cursor()?;
            let hello = core.create_hello(&token, cursor)?;
            validate_frame(&hello)?;
            Ok(hello)
        };
        let manager = DesktopConnectionManager::new(endpoint, hello_provider, factory)?;
        Ok(Self { core, manager })
    }

    pub fn state(&self) -> DesktopConnectionState {
        self.manager.state()
    }

    pub fn is_connected(&self) -> bool {
        self.manager.is_connected()
    }

    pub fn durable_cursor(&self) -> Result<u64, CoreError> {
        self.core
            .lock()
            .map_err(|_| CoreError::Provider)?
            .durable_cursor()
    }

    pub fn start(&mut self, now: Instant) -> Vec<DesktopEvent> {
        self.manager.start(now);
        self.events()
    }

    pub fn stop(&mut self) -> Vec<DesktopEvent> {
        self.manager.stop();
        self.events()
    }

    pub fn poll(&mut self, now: Instant) -> Result<Vec<DesktopEvent>, CoreError> {
        self.manager.poll(now);
        let mut events = self.events();
        self.drain_frames(now, false, &mut events)?;
        events.extend(self.events());
        Ok(events)
    }

    /// Force a fresh Hello with the latest durable cursor, then drain replay.
    /// The host should call this after a wakeup, process restart, or gap signal.
    pub fn recover(&mut self, now: Instant) -> Result<Vec<DesktopEvent>, CoreError> {
        self.manager.stop();
        self.manager.start(now);
        let mut events = self.events();
        self.drain_frames(now, true, &mut events)?;
        events.extend(self.events());
        Ok(events)
    }

    pub fn send_text(
        &mut self,
        now: Instant,
        conversation_id: &str,
        recipient_user_id: &str,
        text: &str,
    ) -> Result<(), CoreError> {
        protocol::validate_id(conversation_id)?;
        protocol::validate_id(recipient_user_id)?;
        if recipient_user_id == self.core.lock().map_err(|_| CoreError::Provider)?.user_id()
            || text.is_empty()
            || text.len() > DESKTOP_MAX_TEXT_BYTES
        {
            return Err(CoreError::Authentication);
        }
        let mut core = self.core.lock().map_err(|_| CoreError::Provider)?;
        let mut transport = ManagerTransport {
            manager: &mut self.manager,
            now,
        };
        core.send_text(
            conversation_id,
            recipient_user_id,
            text,
            &mut transport,
        )
    }

    fn drain_frames(
        &mut self,
        now: Instant,
        full_sync: bool,
        events: &mut Vec<DesktopEvent>,
    ) -> Result<(), CoreError> {
        loop {
            let frame = match self.manager.receive(now)? {
                Some(frame) => frame,
                None => return Ok(()),
            };
            let result = self.process_frame(now, &frame, full_sync, events)?;
            if full_sync && result == DesktopFrameResult::RecoveryComplete {
                return Ok(());
            }
        }
    }

    fn process_frame(
        &mut self,
        now: Instant,
        frame: &[u8],
        full_sync: bool,
        events: &mut Vec<DesktopEvent>,
    ) -> Result<DesktopFrameResult, CoreError> {
        let mut messages = Vec::new();
        let result = {
            let mut core = self.core.lock().map_err(|_| CoreError::Provider)?;
            let mut transport = ManagerTransport {
                manager: &mut self.manager,
                now,
            };
            core.handle_server_frame(frame, &mut transport, full_sync, &mut |message| {
                messages.push(message)
            })?
        };
        events.extend(messages.into_iter().map(DesktopEvent::Text));
        Ok(result)
    }

    fn events(&mut self) -> Vec<DesktopEvent> {
        self.manager
            .take_state_events()
            .into_iter()
            .map(DesktopEvent::State)
            .collect()
    }
}

struct ManagerTransport<'a, F: DesktopSocketFactory> {
    manager: &'a mut DesktopConnectionManager<F>,
    now: Instant,
}

impl<F: DesktopSocketFactory> DesktopFrameTransport for ManagerTransport<'_, F> {
    fn send(&mut self, frame: &[u8]) -> Result<(), CoreError> {
        self.manager.send(self.now, frame)
    }
}

fn validate_endpoint(endpoint: String) -> Result<String, CoreError> {
    let rest = endpoint
        .strip_prefix("wss://")
        .ok_or(CoreError::Authentication)?;
    let (authority, path) = rest.split_once('/').ok_or(CoreError::Authentication)?;
    if authority.is_empty()
        || authority.contains('@')
        || path != "v1/connect"
        || endpoint.contains('?')
        || endpoint.contains('#')
    {
        return Err(CoreError::Authentication);
    }
    Ok(endpoint)
}

fn validate_frame(frame: &[u8]) -> Result<(), CoreError> {
    if frame.is_empty() {
        return Err(CoreError::Protocol(protocol::ProtocolError::Malformed));
    }
    if frame.len() > DESKTOP_MAX_FRAME_BYTES {
        return Err(CoreError::Protocol(protocol::ProtocolError::TooLarge));
    }
    Ok(())
}

fn jitter_ms(ceiling: u64) -> u64 {
    let mut bytes = [0u8; 8];
    if getrandom::fill(&mut bytes).is_err() {
        return ceiling;
    }
    u64::from_le_bytes(bytes) % ceiling.saturating_add(1)
}
