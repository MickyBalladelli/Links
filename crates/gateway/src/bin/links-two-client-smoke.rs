//! Disposable local two-client smoke harness.
//!
//! The harness talks to the existing Debug loopback composition. It creates
//! two signed username devices, opens two concurrent `links.v1` sockets, and
//! routes opaque envelopes in both directions. It never prints credentials,
//! IDs, message text, or sealed payload bytes.

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use futures_util::{SinkExt, StreamExt};
use links_gateway::decode_client_frame;
use links_identity::{username_registration_transcript, IdentitySeed};
use links_protocol::{self as protocol, v1};
use prost::Message;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::{
    env,
    fs,
    path::Path,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tokio::time::timeout;
use tokio_tungstenite::{
    connect_async,
    tungstenite::{http::Request, Message as WebSocketMessage},
    MaybeTlsStream, WebSocketStream,
};
use uuid::Uuid;
use zeroize::Zeroize;

const ALICE_HANDLE: &str = "alice_test";
const BOB_HANDLE: &str = "bob_test";
const DEFAULT_AUTH_URL: &str = "http://127.0.0.1:8080";
const DEFAULT_GATEWAY_URL: &str = "ws://127.0.0.1:8081/v1/connect";
const OPERATION_TIMEOUT: Duration = Duration::from_secs(10);

type Socket = WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;

struct SmokeFailure;

struct Account {
    user_id: Uuid,
    device_id: Uuid,
    access_token: String,
}

impl Drop for Account {
    fn drop(&mut self) {
        self.access_token.zeroize();
    }
}

#[derive(Serialize)]
struct UsernameChallengeRequest {
    handle: String,
    purpose: &'static str,
    device_id: Uuid,
    mls_node_id: Uuid,
    public_key: String,
}

#[derive(Deserialize)]
struct UsernameChallenge {
    challenge_id: Uuid,
    handle: String,
    purpose: String,
    device_id: Uuid,
    mls_node_id: Uuid,
    public_key: String,
    challenge: String,
    expires_at_ms: u64,
}

#[derive(Serialize)]
struct UsernameRegistrationRequest {
    challenge_id: Uuid,
    signature: String,
}

#[derive(Deserialize)]
struct UsernameAuthResponse {
    session: AuthSession,
}

#[derive(Deserialize)]
struct AuthSession {
    access_token: String,
    expires_at_ms: u64,
    user_id: Uuid,
    device_id: Uuid,
}

#[derive(Default)]
struct Timings {
    auth_ms: u64,
    connect_ms: u64,
    duplicate_device_ms: u64,
    alice_to_bob_ms: u64,
    bob_to_alice_ms: u64,
}

#[derive(Serialize)]
struct SmokeReport {
    result: &'static str,
    total_ms: u64,
    auth_ms: u64,
    connect_ms: u64,
    duplicate_device_ms: u64,
    alice_to_bob_ms: u64,
    bob_to_alice_ms: u64,
}

#[tokio::main]
async fn main() {
    let started = Instant::now();
    let mut timings = Timings::default();
    let result = run(&mut timings).await;
    let report = SmokeReport {
        result: if result.is_ok() { "pass" } else { "fail" },
        total_ms: elapsed_ms(started),
        auth_ms: timings.auth_ms,
        connect_ms: timings.connect_ms,
        duplicate_device_ms: timings.duplicate_device_ms,
        alice_to_bob_ms: timings.alice_to_bob_ms,
        bob_to_alice_ms: timings.bob_to_alice_ms,
    };
    let encoded = serde_json::to_string(&report)
        .unwrap_or_else(|_| "{\"result\":\"fail\",\"total_ms\":0}".to_owned());
    if let Some(path) = env::var_os("SMOKE_RESULT_PATH") {
        let _ = write_report(Path::new(&path), encoded.as_bytes());
    }
    println!("{encoded}");
    if result.is_err() {
        eprintln!("two-client smoke failed");
        std::process::exit(1);
    }
}

async fn run(timings: &mut Timings) -> Result<(), SmokeFailure> {
    let auth_url = loopback_http_url(
        &env::var("SMOKE_AUTH_URL").unwrap_or_else(|_| DEFAULT_AUTH_URL.to_owned()),
    )?;
    let gateway_url = loopback_ws_url(
        &env::var("SMOKE_GATEWAY_URL").unwrap_or_else(|_| DEFAULT_GATEWAY_URL.to_owned()),
    )?;
    let http = Client::builder()
        .no_proxy()
        .timeout(OPERATION_TIMEOUT)
        .build()
        .map_err(|_| SmokeFailure)?;

    // The wire handle contract uses `_test`; the UI display labels are
    // @alice-test and @bob-test.
    let auth_started = Instant::now();
    let (alice_result, bob_result) = tokio::join!(
        register_account(&http, &auth_url, ALICE_HANDLE),
        register_account(&http, &auth_url, BOB_HANDLE),
    );
    let alice = alice_result?;
    let bob = bob_result?;
    if alice.user_id == bob.user_id || alice.device_id == bob.device_id {
        return Err(SmokeFailure);
    }
    timings.auth_ms = elapsed_ms(auth_started);

    let connect_started = Instant::now();
    let (alice_socket, bob_socket) = tokio::join!(
        connect_account(&gateway_url, &alice),
        connect_account(&gateway_url, &bob),
    );
    let mut alice_socket = alice_socket?;
    let mut bob_socket = bob_socket?;
    timings.connect_ms = elapsed_ms(connect_started);

    let duplicate_started = Instant::now();
    verify_reused_device(&gateway_url, &alice).await?;
    timings.duplicate_device_ms = elapsed_ms(duplicate_started);

    let direction_started = Instant::now();
    send_and_receive(&mut alice_socket, &mut bob_socket, bob.device_id).await?;
    timings.alice_to_bob_ms = elapsed_ms(direction_started);

    let direction_started = Instant::now();
    send_and_receive(&mut bob_socket, &mut alice_socket, alice.device_id).await?;
    timings.bob_to_alice_ms = elapsed_ms(direction_started);

    let _ = alice_socket.close(None).await;
    let _ = bob_socket.close(None).await;
    Ok(())
}

async fn register_account(
    http: &Client,
    auth_url: &str,
    handle: &str,
) -> Result<Account, SmokeFailure> {
    let seed = IdentitySeed::generate().map_err(|_| SmokeFailure)?;
    let device_id = Uuid::new_v4();
    let mls_node_id = Uuid::new_v4();
    let public_key = seed.public_key();
    let challenge = http
        .post(format!("{auth_url}/v1/auth/username/challenge"))
        .json(&UsernameChallengeRequest {
            handle: handle.to_owned(),
            purpose: "registration",
            device_id,
            mls_node_id,
            public_key: URL_SAFE_NO_PAD.encode(public_key),
        })
        .send()
        .await
        .map_err(|_| SmokeFailure)?
        .json::<UsernameChallenge>()
        .await
        .map_err(|_| SmokeFailure)?;
    if challenge.handle != handle
        || challenge.purpose != "registration"
        || challenge.device_id != device_id
        || challenge.mls_node_id != mls_node_id
        || challenge.public_key != URL_SAFE_NO_PAD.encode(public_key)
        || challenge.expires_at_ms <= now_ms()?
    {
        return Err(SmokeFailure);
    }
    let challenge_bytes: [u8; 32] = URL_SAFE_NO_PAD
        .decode(&challenge.challenge)
        .map_err(|_| SmokeFailure)?
        .try_into()
        .map_err(|_| SmokeFailure)?;
    let transcript = username_registration_transcript(
        challenge.challenge_id,
        handle,
        device_id,
        mls_node_id,
        &public_key,
        &challenge_bytes,
        challenge.expires_at_ms,
    )
    .map_err(|_| SmokeFailure)?;
    let signature = seed.sign(&transcript);
    let request = UsernameRegistrationRequest {
        challenge_id: challenge.challenge_id,
        signature: URL_SAFE_NO_PAD.encode(signature),
    };
    let response = http
        .post(format!("{auth_url}/v1/auth/username/register"))
        .json(&request)
        .send()
        .await
        .map_err(|_| SmokeFailure)?;
    if !response.status().is_success() {
        return Err(SmokeFailure);
    }
    let response = response
        .json::<UsernameAuthResponse>()
        .await
        .map_err(|_| SmokeFailure)?;
    if response.session.access_token.is_empty()
        || response.session.user_id.is_nil()
        || response.session.device_id != device_id
        || response.session.expires_at_ms <= now_ms()?
    {
        return Err(SmokeFailure);
    }
    Ok(Account {
        user_id: response.session.user_id,
        device_id,
        access_token: response.session.access_token,
    })
}

async fn connect_account(endpoint: &str, account: &Account) -> Result<Socket, SmokeFailure> {
    let request = Request::builder()
        .uri(endpoint)
        .header("Sec-WebSocket-Protocol", "links.v1")
        .body(())
        .map_err(|_| SmokeFailure)?;
    let (mut socket, response) = connect_async(request).await.map_err(|_| SmokeFailure)?;
    let negotiated = response
        .headers()
        .get("sec-websocket-protocol")
        .and_then(|value| value.to_str().ok());
    if negotiated != Some("links.v1") {
        return Err(SmokeFailure);
    }

    let hello = v1::ClientFrame {
        request_id: Uuid::new_v4().to_string(),
        body: Some(v1::client_frame::Body::Hello(v1::Hello {
            protocol_version: protocol::VERSION,
            device_id: account.device_id.to_string(),
            device_access_token: account.access_token.as_bytes().to_vec(),
            last_seen_cursor: 0,
            supported_sync_compression: Vec::new(),
        })),
    };
    send_client_frame(&mut socket, hello).await?;
    let frame = next_server_frame(&mut socket).await?;
    match frame.body {
        Some(v1::server_frame::Body::Welcome(welcome))
            if welcome.protocol_version == protocol::VERSION => Ok(socket),
        _ => Err(SmokeFailure),
    }
}

async fn verify_reused_device(endpoint: &str, account: &Account) -> Result<(), SmokeFailure> {
    let request = Request::builder()
        .uri(endpoint)
        .header("Sec-WebSocket-Protocol", "links.v1")
        .body(())
        .map_err(|_| SmokeFailure)?;
    let (mut socket, response) = connect_async(request).await.map_err(|_| SmokeFailure)?;
    let negotiated = response
        .headers()
        .get("sec-websocket-protocol")
        .and_then(|value| value.to_str().ok());
    if negotiated != Some("links.v1") {
        return Err(SmokeFailure);
    }

    let hello = v1::ClientFrame {
        request_id: Uuid::new_v4().to_string(),
        body: Some(v1::client_frame::Body::Hello(v1::Hello {
            protocol_version: protocol::VERSION,
            device_id: account.device_id.to_string(),
            device_access_token: account.access_token.as_bytes().to_vec(),
            last_seen_cursor: 0,
            supported_sync_compression: Vec::new(),
        })),
    };
    send_client_frame(&mut socket, hello).await?;
    let frame = next_server_frame(&mut socket).await?;
    let conflict = matches!(
        frame.body,
        Some(v1::server_frame::Body::Error(error))
            if error.code == v1::protocol_error::Code::SessionConflict as i32
    );
    let _ = socket.close(None).await;
    if conflict {
        Ok(())
    } else {
        Err(SmokeFailure)
    }
}

async fn send_and_receive(
    sender: &mut Socket,
    recipient: &mut Socket,
    recipient_device_id: Uuid,
) -> Result<(), SmokeFailure> {
    let now = now_ms()?;
    let envelope = v1::Envelope {
        protocol_version: protocol::VERSION,
        envelope_id: Uuid::new_v4().to_string(),
        recipient_device_id: recipient_device_id.to_string(),
        expires_at_ms: now
            .checked_add(protocol::MAX_RETENTION_MS)
            .ok_or(SmokeFailure)?,
        sealed_payload: opaque_payload(),
    };
    let request_id = Uuid::new_v4().to_string();
    send_client_frame(
        sender,
        v1::ClientFrame {
            request_id: request_id.clone(),
            body: Some(v1::client_frame::Body::Send(envelope.clone())),
        },
    )
    .await?;
    let accepted = next_server_frame(sender).await?;
    match accepted.body {
        Some(v1::server_frame::Body::Accepted(accepted))
            if accepted.envelope_id == envelope.envelope_id => {}
        _ => return Err(SmokeFailure),
    }

    let delivery = next_server_frame(recipient).await?;
    let batch = match delivery.body {
        Some(v1::server_frame::Body::Batch(batch)) => batch,
        _ => return Err(SmokeFailure),
    };
    let item = batch
        .items
        .iter()
        .find(|item| item.entry.as_ref().is_some_and(|entry| {
            matches!(entry, v1::queue_item::Entry::Envelope(_))
        }))
        .ok_or(SmokeFailure)?;
    let delivered = match item.entry.as_ref() {
        Some(v1::queue_item::Entry::Envelope(envelope)) => envelope,
        _ => return Err(SmokeFailure),
    };
    if delivered != &envelope || batch.next_cursor != item.cursor {
        return Err(SmokeFailure);
    }

    send_client_frame(
        recipient,
        v1::ClientFrame {
            request_id: Uuid::new_v4().to_string(),
            body: Some(v1::client_frame::Body::Ack(v1::QueueAck {
                through_cursor: item.cursor,
            })),
        },
    )
    .await
}

async fn send_client_frame(socket: &mut Socket, frame: v1::ClientFrame) -> Result<(), SmokeFailure> {
    let bytes = frame.encode_to_vec();
    decode_client_frame(&bytes).map_err(|_| SmokeFailure)?;
    socket
        .send(WebSocketMessage::Binary(bytes.into()))
        .await
        .map_err(|_| SmokeFailure)
}

async fn next_server_frame(socket: &mut Socket) -> Result<v1::ServerFrame, SmokeFailure> {
    loop {
        let message = timeout(OPERATION_TIMEOUT, socket.next())
            .await
            .map_err(|_| SmokeFailure)?
            .ok_or(SmokeFailure)?
            .map_err(|_| SmokeFailure)?;
        match message {
            WebSocketMessage::Binary(bytes) => {
                if bytes.is_empty() || bytes.len() > protocol::MAX_FRAME_BYTES {
                    return Err(SmokeFailure);
                }
                let frame = v1::ServerFrame::decode(bytes.as_ref()).map_err(|_| SmokeFailure)?;
                protocol::validate_id(&frame.request_id).map_err(|_| SmokeFailure)?;
                return Ok(frame);
            }
            WebSocketMessage::Ping(payload) => {
                socket
                    .send(WebSocketMessage::Pong(payload))
                    .await
                    .map_err(|_| SmokeFailure)?;
            }
            WebSocketMessage::Pong(_) => {}
            WebSocketMessage::Text(_)
            | WebSocketMessage::Close(_)
            | WebSocketMessage::Frame(_) => return Err(SmokeFailure),
        }
    }
}

fn fresh_nonce() -> [u8; 32] {
    let first = Uuid::new_v4();
    let second = Uuid::new_v4();
    let mut nonce = [0u8; 32];
    nonce[..16].copy_from_slice(first.as_bytes());
    nonce[16..].copy_from_slice(second.as_bytes());
    nonce
}

fn opaque_payload() -> Vec<u8> {
    fresh_nonce().to_vec()
}

fn now_ms() -> Result<u64, SmokeFailure> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| SmokeFailure)
        .and_then(|duration| u64::try_from(duration.as_millis()).map_err(|_| SmokeFailure))
}

fn elapsed_ms(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

fn loopback_http_url(value: &str) -> Result<String, SmokeFailure> {
    let parsed = reqwest::Url::parse(value).map_err(|_| SmokeFailure)?;
    if parsed.scheme() != "http"
        || !matches!(parsed.host_str(), Some("127.0.0.1" | "localhost" | "[::1]" | "::1"))
        || parsed.query().is_some()
        || parsed.fragment().is_some()
    {
        return Err(SmokeFailure);
    }
    Ok(value.trim_end_matches('/').to_owned())
}

fn loopback_ws_url(value: &str) -> Result<String, SmokeFailure> {
    let parsed = reqwest::Url::parse(value).map_err(|_| SmokeFailure)?;
    if parsed.scheme() != "ws"
        || !matches!(parsed.host_str(), Some("127.0.0.1" | "localhost" | "[::1]" | "::1"))
        || parsed.path() != "/v1/connect"
        || parsed.query().is_some()
        || parsed.fragment().is_some()
    {
        return Err(SmokeFailure);
    }
    Ok(value.to_owned())
}

fn write_report(path: &Path, bytes: &[u8]) -> Result<(), SmokeFailure> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).map_err(|_| SmokeFailure)?;
        }
    }
    fs::write(path, bytes).map_err(|_| SmokeFailure)
}
