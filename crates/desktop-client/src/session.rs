use links_client_core::{
    attachments::{decrypt_large_file, validate_large_file_metadata, LargeFileEncryptor},
    protocol, CoreError,
};
use sha2::{Digest, Sha256};
use std::{
    collections::VecDeque,
    fs,
    io::{Read, Write},
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

pub const DESKTOP_HEARTBEAT_INTERVAL_MS: u64 = 30_000;
pub const DESKTOP_INITIAL_BACKOFF_MS: u64 = 1_000;
pub const DESKTOP_MAX_BACKOFF_MS: u64 = 30_000;
pub const DESKTOP_STABLE_CONNECTION_MS: u64 = 30_000;
pub const DESKTOP_MAX_FRAME_BYTES: usize = protocol::MAX_FRAME_BYTES;
pub const DESKTOP_MAX_TEXT_BYTES: usize = 64 * 1024;
pub const DESKTOP_IMAGE_MAX_EDGE: u32 = 1_600;
pub const DESKTOP_IMAGE_MAX_PLAINTEXT_BYTES: usize = 32 * 1024 * 1024;
pub const DESKTOP_IMAGE_MAX_CIPHERTEXT_BYTES: usize = DESKTOP_IMAGE_MAX_PLAINTEXT_BYTES + 16;
pub const DESKTOP_LARGE_FILE_CIPHERTEXT_CHUNK_BYTES: usize =
    links_client_core::attachments::LARGE_FILE_CIPHERTEXT_CHUNK_BYTES;

#[derive(Clone, PartialEq, Eq)]
pub struct DesktopEncryptedLargeFile {
    pub metadata: protocol::v1::MediaMetadata,
    pub ciphertext_path: PathBuf,
}

impl DesktopEncryptedLargeFile {
    pub fn validate(&self) -> Result<(), CoreError> {
        validate_large_file_metadata(&self.metadata)?;
        let file_metadata = fs::metadata(&self.ciphertext_path).map_err(|_| CoreError::Provider)?;
        if file_metadata.len() != self.metadata.ciphertext_size_bytes {
            return Err(CoreError::Authentication);
        }
        let mut file = fs::File::open(&self.ciphertext_path).map_err(|_| CoreError::Provider)?;
        let mut digest = Sha256::new();
        let mut size = 0u64;
        let mut buffer = [0u8; 64 * 1024];
        loop {
            let read = file.read(&mut buffer).map_err(|_| CoreError::Provider)?;
            if read == 0 {
                break;
            }
            digest.update(&buffer[..read]);
            size = size
                .checked_add(read as u64)
                .ok_or(CoreError::Authentication)?;
        }
        if size != self.metadata.ciphertext_size_bytes
            || digest.finalize().as_slice() != self.metadata.ciphertext_sha256.as_slice()
        {
            return Err(CoreError::Authentication);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DesktopLargeFileUploadReceipt {
    pub attachment_id: String,
    pub ciphertext_size_bytes: u64,
    pub ciphertext_sha256: Vec<u8>,
}

impl DesktopLargeFileUploadReceipt {
    pub fn matches(&self, metadata: &protocol::v1::MediaMetadata) -> bool {
        self.attachment_id == metadata.attachment_id
            && self.ciphertext_size_bytes == metadata.ciphertext_size_bytes
            && self.ciphertext_sha256 == metadata.ciphertext_sha256
    }
}

pub trait DesktopLargeFileUploader: Send {
    fn upload(
        &mut self,
        access_token: &str,
        file: &DesktopEncryptedLargeFile,
    ) -> Result<DesktopLargeFileUploadReceipt, CoreError>;
    /// Return a disposable ciphertext staging path.
    fn download(
        &mut self,
        access_token: &str,
        metadata: &protocol::v1::MediaMetadata,
    ) -> Result<PathBuf, CoreError>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DesktopImageMetadata {
    pub attachment_id: String,
    pub mime_type: String,
    pub ciphertext_size_bytes: u64,
    pub content_key: Vec<u8>,
    pub nonce: Vec<u8>,
    pub ciphertext_sha256: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub blur_hash: String,
}

impl DesktopImageMetadata {
    pub fn new(
        attachment_id: impl Into<String>,
        mime_type: impl Into<String>,
        ciphertext_size_bytes: u64,
        content_key: Vec<u8>,
        nonce: Vec<u8>,
        ciphertext_sha256: Vec<u8>,
        width: u32,
        height: u32,
        blur_hash: impl Into<String>,
    ) -> Result<Self, CoreError> {
        let metadata = Self {
            attachment_id: attachment_id.into(),
            mime_type: mime_type.into(),
            ciphertext_size_bytes,
            content_key,
            nonce,
            ciphertext_sha256,
            width,
            height,
            blur_hash: blur_hash.into(),
        };
        metadata.validate()?;
        Ok(metadata)
    }

    pub fn validate(&self) -> Result<(), CoreError> {
        protocol::validate_id(&self.attachment_id)?;
        if !matches!(self.mime_type.as_str(), "image/webp" | "image/avif")
            || !(17..=DESKTOP_IMAGE_MAX_CIPHERTEXT_BYTES as u64)
                .contains(&self.ciphertext_size_bytes)
            || self.content_key.len() != 32
            || self.nonce.len() != 12
            || self.ciphertext_sha256.len() != 32
            || self.width == 0
            || self.width > DESKTOP_IMAGE_MAX_EDGE
            || self.height == 0
            || self.height > DESKTOP_IMAGE_MAX_EDGE
        {
            return Err(CoreError::Authentication);
        }
        links_client_core::images::resized_dimensions(self.width, self.height)?;
        protocol::validate_blur_hash(&self.blur_hash)?;
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DesktopEncryptedImage {
    pub metadata: DesktopImageMetadata,
    pub ciphertext: Vec<u8>,
}

impl DesktopEncryptedImage {
    pub fn new(metadata: DesktopImageMetadata, ciphertext: Vec<u8>) -> Result<Self, CoreError> {
        let image = Self {
            metadata,
            ciphertext,
        };
        image.validate()?;
        Ok(image)
    }

    pub fn validate(&self) -> Result<(), CoreError> {
        self.metadata.validate()?;
        if self.ciphertext.len() < 17
            || self.ciphertext.len() > DESKTOP_IMAGE_MAX_CIPHERTEXT_BYTES
            || self.metadata.ciphertext_size_bytes != self.ciphertext.len() as u64
            || Sha256::digest(&self.ciphertext).as_slice()
                != self.metadata.ciphertext_sha256.as_slice()
        {
            return Err(CoreError::Authentication);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DesktopImageUploadReceipt {
    pub attachment_id: String,
    pub ciphertext_size_bytes: u64,
    pub ciphertext_sha256: Vec<u8>,
}

impl DesktopImageUploadReceipt {
    pub fn matches(&self, metadata: &DesktopImageMetadata) -> bool {
        self.attachment_id == metadata.attachment_id
            && self.ciphertext_size_bytes == metadata.ciphertext_size_bytes
            && self.ciphertext_sha256 == metadata.ciphertext_sha256
    }
}

pub trait DesktopImageUploader: Send {
    fn upload(
        &mut self,
        access_token: &str,
        image: &DesktopEncryptedImage,
    ) -> Result<DesktopImageUploadReceipt, CoreError>;
    fn download(
        &mut self,
        access_token: &str,
        metadata: &DesktopImageMetadata,
    ) -> Result<Vec<u8>, CoreError>;
}

pub trait DesktopImageRenderer: Send {
    /// The renderer must not retain plaintext after this call returns.
    fn render(
        &mut self,
        plaintext: &[u8],
        metadata: &DesktopImageMetadata,
    ) -> Result<(), CoreError>;
}

pub trait DesktopImageCache: Send {
    fn read(&mut self, metadata: &DesktopImageMetadata) -> Result<Option<Vec<u8>>, CoreError>;
    fn write(&mut self, image: &DesktopEncryptedImage) -> Result<(), CoreError>;
}

/// Ciphertext-only desktop cache. The UI chooses the directory, typically an
/// OS cache directory. It never writes keys, metadata, or decrypted pixels.
pub struct DesktopImageFileCache {
    directory: PathBuf,
}

impl DesktopImageFileCache {
    pub fn new(directory: impl Into<PathBuf>) -> Result<Self, CoreError> {
        let directory = directory.into();
        fs::create_dir_all(&directory).map_err(|_| CoreError::Provider)?;
        Ok(Self { directory })
    }

    fn path_for(&self, attachment_id: &str) -> Result<PathBuf, CoreError> {
        protocol::validate_id(attachment_id)?;
        Ok(self.directory.join(format!("{attachment_id}.blob")))
    }
}

impl DesktopImageCache for DesktopImageFileCache {
    fn read(&mut self, metadata: &DesktopImageMetadata) -> Result<Option<Vec<u8>>, CoreError> {
        metadata.validate()?;
        let path = self.path_for(&metadata.attachment_id)?;
        if !path.is_file() {
            return Ok(None);
        }
        let size = fs::metadata(&path).map_err(|_| CoreError::Provider)?.len();
        if size != metadata.ciphertext_size_bytes
            || size > DESKTOP_IMAGE_MAX_CIPHERTEXT_BYTES as u64
        {
            return Ok(None);
        }
        let ciphertext = fs::read(path).map_err(|_| CoreError::Provider)?;
        if ciphertext.len() < 17
            || ciphertext.len() > DESKTOP_IMAGE_MAX_CIPHERTEXT_BYTES
            || metadata.ciphertext_size_bytes != ciphertext.len() as u64
            || Sha256::digest(&ciphertext).as_slice() != metadata.ciphertext_sha256.as_slice()
        {
            return Ok(None);
        }
        Ok(Some(ciphertext))
    }

    fn write(&mut self, image: &DesktopEncryptedImage) -> Result<(), CoreError> {
        image.validate()?;
        let path = self.path_for(&image.metadata.attachment_id)?;
        let temporary = path.with_extension("blob.tmp");
        fs::write(&temporary, &image.ciphertext).map_err(|_| CoreError::Provider)?;
        fs::rename(temporary, path).map_err(|_| CoreError::Provider)?;
        Ok(())
    }
}

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

        if self.connected_at.is_some_and(|connected| {
            now.saturating_duration_since(connected)
                >= Duration::from_millis(DESKTOP_STABLE_CONNECTION_MS)
        }) {
            self.backoff_ms = DESKTOP_INITIAL_BACKOFF_MS;
        }

        let heartbeat_due = self.last_heartbeat_at.is_some_and(|last| {
            now.saturating_duration_since(last)
                >= Duration::from_millis(DESKTOP_HEARTBEAT_INTERVAL_MS)
        });
        if heartbeat_due {
            let result = self
                .socket
                .as_mut()
                .map_or(Err(CoreError::Provider), |socket| socket.ping());
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
    fn create_hello(
        &mut self,
        access_token: &str,
        last_seen_cursor: u64,
    ) -> Result<Vec<u8>, CoreError>;
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
    /// Encode the private fixed-size BlurHash from normalized RGB pixels.
    fn encode_image_blur_hash(
        &mut self,
        _rgb_pixels: &[u8],
        _width: u32,
        _height: u32,
    ) -> Result<String, CoreError> {
        Err(CoreError::Provider)
    }
    /// Encrypt normalized image bytes and return metadata for the private MLS
    /// message plus ciphertext for opaque blob storage.
    fn encrypt_image(
        &mut self,
        _image: &[u8],
        _attachment_id: &str,
        _mime_type: &str,
        _width: u32,
        _height: u32,
        _blur_hash: &str,
    ) -> Result<DesktopEncryptedImage, CoreError> {
        Err(CoreError::Provider)
    }
    fn decrypt_image(
        &mut self,
        _metadata: &DesktopImageMetadata,
        _ciphertext: &[u8],
    ) -> Result<Vec<u8>, CoreError> {
        Err(CoreError::Provider)
    }
    fn send_image(
        &mut self,
        _conversation_id: &str,
        _recipient_user_id: &str,
        _metadata: &DesktopImageMetadata,
        _receipt: &DesktopImageUploadReceipt,
        _transport: &mut dyn DesktopFrameTransport,
    ) -> Result<(), CoreError> {
        Err(CoreError::Provider)
    }
    fn send_large_file(
        &mut self,
        _conversation_id: &str,
        _recipient_user_id: &str,
        _metadata: &protocol::v1::MediaMetadata,
        _receipt: &DesktopLargeFileUploadReceipt,
        _transport: &mut dyn DesktopFrameTransport,
    ) -> Result<(), CoreError> {
        Err(CoreError::Provider)
    }
}

pub type DesktopAccessTokenProvider = Arc<dyn Fn() -> Result<String, CoreError> + Send + Sync>;

/// Desktop text/session host. It owns no crypto: the injected core owns MLS,
/// Sealed Sender, durable inbox/outbox state, cursors, and acknowledgements.
pub struct DesktopTextSession<C, F: DesktopSocketFactory> {
    core: Arc<Mutex<C>>,
    manager: DesktopConnectionManager<F>,
    access_token: DesktopAccessTokenProvider,
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
        Ok(Self {
            core,
            manager,
            access_token,
        })
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
        core.send_text(conversation_id, recipient_user_id, text, &mut transport)
    }

    /// Encrypt normalized image bytes. The host supplies decoded RGB pixels so
    /// the shared core, not the UI, creates the private BlurHash.
    pub fn prepare_and_encrypt_image(
        &mut self,
        image: &[u8],
        rgb_pixels: &[u8],
        mime_type: &str,
        width: u32,
        height: u32,
    ) -> Result<DesktopEncryptedImage, CoreError> {
        if image.is_empty()
            || !matches!(mime_type, "image/webp" | "image/avif")
            || width == 0
            || height == 0
        {
            return Err(CoreError::Authentication);
        }
        let attachment_id = uuid::Uuid::new_v4().to_string();
        let mut core = self.core.lock().map_err(|_| CoreError::Provider)?;
        let blur_hash = core.encode_image_blur_hash(rgb_pixels, width, height)?;
        let image =
            core.encrypt_image(image, &attachment_id, mime_type, width, height, &blur_hash)?;
        image.metadata.validate()?;
        Ok(image)
    }

    pub fn upload_image(
        &mut self,
        uploader: &mut dyn DesktopImageUploader,
        image: &DesktopEncryptedImage,
    ) -> Result<DesktopImageUploadReceipt, CoreError> {
        image.validate()?;
        let token = self.require_access_token()?;
        let receipt = uploader.upload(&token, image)?;
        if !receipt.matches(&image.metadata) {
            return Err(CoreError::Authentication);
        }
        Ok(receipt)
    }

    /// Send private metadata only after the opaque ciphertext upload receipt
    /// exactly matches the bytes and attachment ID.
    pub fn send_image(
        &mut self,
        now: Instant,
        conversation_id: &str,
        recipient_user_id: &str,
        image: &DesktopEncryptedImage,
        receipt: &DesktopImageUploadReceipt,
    ) -> Result<(), CoreError> {
        protocol::validate_id(conversation_id)?;
        protocol::validate_id(recipient_user_id)?;
        image.metadata.validate()?;
        if receipt.matches(&image.metadata)
            && recipient_user_id != self.core.lock().map_err(|_| CoreError::Provider)?.user_id()
            && self.is_connected()
        {
            let mut core = self.core.lock().map_err(|_| CoreError::Provider)?;
            let mut transport = ManagerTransport {
                manager: &mut self.manager,
                now,
            };
            return core.send_image(
                conversation_id,
                recipient_user_id,
                &image.metadata,
                receipt,
                &mut transport,
            );
        }
        Err(CoreError::Authentication)
    }

    /// Fetch ciphertext from cache or authenticated blob storage, verify it,
    /// decrypt it in the shared core, render it, then wipe plaintext bytes.
    pub fn download_and_render_image(
        &mut self,
        uploader: &mut dyn DesktopImageUploader,
        cache: &mut dyn DesktopImageCache,
        metadata: &DesktopImageMetadata,
        renderer: &mut dyn DesktopImageRenderer,
    ) -> Result<(), CoreError> {
        metadata.validate()?;
        let mut ciphertext = match cache.read(metadata)? {
            Some(ciphertext) => ciphertext,
            None => {
                let token = self.require_access_token()?;
                let ciphertext = uploader.download(&token, metadata)?;
                let image = DesktopEncryptedImage::new(metadata.clone(), ciphertext)?;
                cache.write(&image)?;
                image.ciphertext
            }
        };
        let verified = DesktopEncryptedImage::new(metadata.clone(), ciphertext)?;
        ciphertext = verified.ciphertext;
        let mut plaintext = {
            let mut core = self.core.lock().map_err(|_| CoreError::Provider)?;
            core.decrypt_image(metadata, &ciphertext)?
        };
        let result = renderer.render(&plaintext, metadata);
        plaintext.fill(0);
        ciphertext.fill(0);
        result
    }

    /// Stream-encrypt a transcoded MP4 or arbitrary file into a ciphertext
    /// staging path. The key and metadata remain private to the MLS message.
    pub fn prepare_and_encrypt_large_file(
        &mut self,
        source: impl AsRef<std::path::Path>,
        destination_directory: impl AsRef<std::path::Path>,
        mime_type: &str,
        width: Option<u32>,
        height: Option<u32>,
        duration_ms: Option<u64>,
    ) -> Result<DesktopEncryptedLargeFile, CoreError> {
        let source = source.as_ref();
        let destination_directory = destination_directory.as_ref();
        let source_metadata = fs::metadata(source).map_err(|_| CoreError::Provider)?;
        if !source_metadata.is_file() || source_metadata.len() == 0 {
            return Err(CoreError::Authentication);
        }
        fs::create_dir_all(destination_directory).map_err(|_| CoreError::Provider)?;
        let attachment_id = uuid::Uuid::new_v4().to_string();
        let ciphertext_path =
            destination_directory.join(format!(".links-encrypted-{attachment_id}.blob"));
        let mut encryptor = LargeFileEncryptor::new(
            attachment_id,
            mime_type.to_owned(),
            width,
            height,
            duration_ms,
        )?;
        let result = (|| {
            let input = fs::File::open(source).map_err(|_| CoreError::Provider)?;
            let mut output = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&ciphertext_path)
                .map_err(|_| CoreError::Provider)?;
            let encrypted = encryptor.encrypt_reader(input, &mut output)?;
            output.flush().map_err(|_| CoreError::Provider)?;
            Ok(DesktopEncryptedLargeFile {
                metadata: encrypted.media,
                ciphertext_path: ciphertext_path.clone(),
            })
        })();
        if result.is_err() {
            let _ = fs::remove_file(&ciphertext_path);
        }
        result
    }

    pub fn upload_large_file(
        &mut self,
        uploader: &mut dyn DesktopLargeFileUploader,
        file: &DesktopEncryptedLargeFile,
    ) -> Result<DesktopLargeFileUploadReceipt, CoreError> {
        file.validate()?;
        let token = self.require_access_token()?;
        let receipt = uploader.upload(&token, file)?;
        if !receipt.matches(&file.metadata) {
            return Err(CoreError::Authentication);
        }
        Ok(receipt)
    }

    /// Send private video/file metadata only after the opaque upload receipt.
    pub fn send_large_file(
        &mut self,
        now: Instant,
        conversation_id: &str,
        recipient_user_id: &str,
        file: &DesktopEncryptedLargeFile,
        receipt: &DesktopLargeFileUploadReceipt,
    ) -> Result<(), CoreError> {
        protocol::validate_id(conversation_id)?;
        protocol::validate_id(recipient_user_id)?;
        file.validate()?;
        if !receipt.matches(&file.metadata)
            || recipient_user_id == self.core.lock().map_err(|_| CoreError::Provider)?.user_id()
            || !self.is_connected()
        {
            return Err(CoreError::Authentication);
        }
        let mut core = self.core.lock().map_err(|_| CoreError::Provider)?;
        let mut transport = ManagerTransport {
            manager: &mut self.manager,
            now,
        };
        core.send_large_file(
            conversation_id,
            recipient_user_id,
            &file.metadata,
            receipt,
            &mut transport,
        )
    }

    /// Download opaque ciphertext and decrypt it into an atomically published
    /// destination after full chunk authentication and digest verification.
    pub fn download_and_decrypt_large_file(
        &mut self,
        uploader: &mut dyn DesktopLargeFileUploader,
        metadata: &protocol::v1::MediaMetadata,
        destination: impl AsRef<std::path::Path>,
    ) -> Result<(), CoreError> {
        validate_large_file_metadata(metadata)?;
        let destination = destination.as_ref();
        if destination.exists() {
            return Err(CoreError::Authentication);
        }
        let token = self.require_access_token()?;
        let ciphertext_path = uploader.download(&token, metadata)?;
        let parent = destination.parent().ok_or(CoreError::Provider)?;
        fs::create_dir_all(parent).map_err(|_| CoreError::Provider)?;
        let staging = parent.join(format!(".links-decrypted-{}.tmp", uuid::Uuid::new_v4()));
        let result = (|| {
            let input = fs::File::open(&ciphertext_path).map_err(|_| CoreError::Provider)?;
            let output = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&staging)
                .map_err(|_| CoreError::Provider)?;
            decrypt_large_file(metadata, input, output)?;
            fs::rename(&staging, destination).map_err(|_| CoreError::Provider)
        })();
        let _ = fs::remove_file(&ciphertext_path);
        if result.is_err() {
            let _ = fs::remove_file(&staging);
        }
        result
    }

    fn require_access_token(&self) -> Result<String, CoreError> {
        let token = (self.access_token)()?;
        if token.is_empty() {
            return Err(CoreError::Authentication);
        }
        Ok(token)
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
            })
        };
        let result = match result {
            Ok(result) => result,
            Err(error) => {
                self.manager.stop();
                return Err(error);
            }
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
