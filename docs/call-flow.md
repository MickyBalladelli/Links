# End-to-end SFU call flow

`web/src/WebRtcCallFlow.ts` is the browser orchestration layer for calls over
the managed LiveKit/Mediasoup SFU boundary. It keeps provider signaling behind
`WebRtcSfuSignaling` and owns the security-sensitive ordering around MLS,
SFrame, SDP, ICE, tracks, and teardown.

## Start sequence

1. The trusted call service returns an opaque room name, regional SFU endpoint,
   and short-lived room token. The token is a room credential, never a
   provider API secret.
2. The client asks its MLS host for the initial media-session epoch key.
3. The client imports that key as a non-extractable AES-128-GCM key, creates
   the SFrame transforms, adds local send tracks and receive-only audio/video
   transceivers, and attaches transforms before creating SDP.
4. The client publishes the same key through the MLS application-control
   callback. The callback must call the existing
   `send_sframe_epoch_key()` or group equivalent and encrypt the key before it
   leaves the client.
5. The client joins the selected SFU room through the provider adapter, creates
   an SDP offer, and sends it to the SFU.
6. The adapter delivers the SFU answer and ICE candidates. The call flow
   validates the session ID, applies the answer, queues early candidates until
   a remote description exists, and attaches SFrame to every receiver.
7. The SFU forwards RTP using headers only. SFrame protects encoded media
   payloads, so the SFU never receives plaintext media or MLS keys.

## Key rotation

The MLS host calls `publishSFrameEpochKey()` after an authenticated MLS epoch
change. The flow serializes key installation, retains the current and previous
SFrame keys, publishes the update through MLS, and applies inbound updates in
order. A stale epoch, key-ID reuse, wrong media-session ID, malformed key, or
missing SFrame support fails the call closed.

## Adapter boundary

`WebRtcSfuSignaling` is intentionally small so the same flow can sit above a
LiveKit Cloud SDK or a Mediasoup client adapter. The adapter handles the
provider's room join and SDP/ICE signaling only. It must not receive MLS
control payloads, SFrame epoch keys, account credentials, or plaintext media.

The call flow always starts as the offerer. An adapter that uses server-offer
negotiation may deliver an offer through `subscribe()`; the flow answers it
with the same SFrame-protected peer connection.

## Teardown

`stop()` unsubscribes signal and MLS listeners, leaves the SFU room, closes
SFrame transforms, closes the peer connection, clears pending candidates, and
emits `ended`. Provider failure emits `failed` and leaves the room without
exposing media or key material to the failure callback.
