# WebRTC session signaling

Links exchanges WebRTC session descriptions through authenticated `links.v1`
signaling. WebSocket is primary; browsers may use the WebTransport/HTTP/3
fallback after a WebSocket failure. This is signaling metadata only. Media keys
and media frames use later MLS/SFrame controls and never enter the signaling
message.

## Wire contract

`ClientFrame.web_rtc_signal` contains a non-nil session UUID, target device
UUID, one kind (`OFFER`, `ANSWER`, or `ICE_CANDIDATE`), bounded SDP/candidate
text, and optional ICE media ID and m-line index.

The gateway creates `WebRtcSignalDelivery` with the authenticated sender
device ID and original request ID. A target device receives signals only on
its live socket. Signaling is not written to the durable mailbox. An offline
target produces a temporary-unavailable response, so the caller retries or
fails its WebRTC session.

Cross-region gateways forward `GatewayWebRtcSignal` on a transient signaling
subject. Production NATS adapters must use a core NATS subject for this path,
not JetStream persistence.

## Browser flow

`web/src/WebRtcSession.ts` wraps `RTCPeerConnection`:

1. Start the shared WebSocket session and wait for its ready state.
2. The offerer calls `startOffer()`, which creates the local description and
   sends `OFFER`.
3. The answerer receives `OFFER`, applies it, creates an answer, and sends
   `ANSWER`.
4. Both peers send each generated ICE candidate as `ICE_CANDIDATE`.
5. The session validates sender, target, and session IDs before applying any
   remote description or candidate.

The shared Rust client core and WASM binding encode and validate protobuf
frames. `WebRtcFileTransfer` may use a DataChannel only after this session is
connected and must still receive an ordered, reliable channel carrying
ciphertext only.

## WebTransport fallback

Pass `webTransportEndpoint: "https://<host>/v1/connect"` to
`WebRtcSession`. The WebSocket remains primary. If its connection fails, the
session closes that socket and starts one authenticated WebTransport session
with `congestionControl: "low-latency"`.

WebTransport bidirectional streams are byte streams, not WebSocket messages.
`WebTransportConnectionManager` therefore prefixes every `links.v1` protobuf
frame with a 4-byte big-endian length. The server HTTP/3 adapter must remove
that prefix, enforce the 1 MiB frame limit, and pass the exact bytes through
the existing `decode_client_frame` / `encode_server_frame` boundary. The Hello
frame remains the first frame, so access tokens stay inside the authenticated
protocol and are not put in the URL.

Signaling uses the reliable bidirectional stream. WebTransport datagrams are
not used for SDP/ICE because they can be lost or reordered; QUIC stream loss
recovery is the desired behavior on poor networks. The fallback keeps the
same exponential backoff, Hello deadline, cursor handling, and protocol
validation as WebSocket.
