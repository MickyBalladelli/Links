# WebTransport signaling fallback

The browser signaling path uses WebSocket first and WebTransport over HTTP/3
as a fallback for high-loss networks. The browser implementation is
`web/src/WebTransportConnectionManager.ts` and is enabled through
`WebRtcSessionOptions.webTransportEndpoint`.

## Wire contract

The WebTransport endpoint is `https://<host>/v1/connect`, with no credentials,
query parameters, or fragments. The client opens one authenticated
bidirectional stream and sends the normal `links.v1` protobuf `Hello` first.
Every frame on that byte stream is:

```text
uint32 big-endian frame_bytes
frame_bytes bytes of one links.v1 protobuf frame
```

Frame lengths must be between 1 byte and `WEB_MAX_FRAME_BYTES` (1 MiB). The
server adapter must reject malformed or oversized prefixes before protobuf
decode and then reuse the existing gateway validation and response encoder.

The access token stays in the Hello protobuf. Never put it in the WebTransport
URL, stream metadata, room name, or telemetry.

## Fallback behavior

`WebRtcSession` starts WebSocket. On a failed WebSocket attempt it fences and
closes that manager, then starts WebTransport with low-latency congestion
control. WebTransport uses one reliable ordered stream: QUIC handles packet
loss and retransmission while the existing signaling semantics stay unchanged.
The manager preserves the 5-second Hello deadline, 30-second liveness probe,
jittered exponential reconnect, 1 MiB frame bound, and callback isolation.

Datagrams are intentionally not used for SDP/ICE. They are unreliable and can
arrive out of order; losing a candidate without a separate replay protocol
would make call setup nondeterministic.

## Server deployment boundary

The repository provides the browser client and the matching Rust
`links_gateway::webtransport::FrameDecoder` / `encode_frame` framing helpers.
The public gateway still needs an HTTP/3/WebTransport adapter that terminates
TLS, authenticates the Hello through the existing `Gateway` core, converts
stream frames to WebSocket-equivalent callbacks, and writes length-prefixed
server frames back on the stream. It must not persist signaling or media
payloads.

The SFU remains separate: it receives WebRTC media after SDP setup and routes
SFrame-encrypted payloads using RTP headers only. WebTransport carries
signaling, not media keys or media frames.
