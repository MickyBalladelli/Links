# Decentralized client support

All four client surfaces now share one decentralized contract:

- Android: `AndroidDecentralizedClient`;
- iOS: `IOSDecentralizedClient`;
- Web: `WebDecentralizedClient`;
- desktop and Web/WASM Rust bindings: `links-client-core::decentralized`.

## Transport

The client receives an ordered list of authenticated `wss://` relay endpoints.
It publishes the exact sealed envelope bytes to the first available endpoint
and retries the others on failure. Replay uses the same endpoint list and a
cursor/limit. A client never gives a relay plaintext message content, MLS
state, or a private key.

The platform transport adapter owns TLS/Noise, peer authentication, framing,
and the selected federation or independent relay implementation. It must not
rewrite the opaque envelope while retrying.

## Storage

The client receives HTTPS gateways for IPFS, Arweave, or Filecoin-backed block
adapters. Upload and download APIs carry a CID plus ciphertext only. Before
upload, and again after every download, the clients derive CIDv1 raw/SHA-256
from the ciphertext and reject a mismatch. Decryption happens only after that
check and the existing attachment AEAD check.

## Media

The client accepts only relay records already verified by the shared
Rust/WASM verifier against the trusted Ed25519 directory. It picks a fresh
open relay first. If none exists, it accepts a short-lived access token only
when its node, opaque call session, expiry, and maximum duration match the
selected token relay.

Every returned media route requires SFrame. The native/browser WebRTC engine
still installs SFrame before SDP and passes only encrypted frames through the
relay. The relay sees RTP routing headers but no media keys or plaintext.

## Host boundary

`DecentralizedTransportAdapter` and `DecentralizedChunkStorage` in
`links-client-core` are the Rust boundary. The Android, iOS, and Web classes
mirror that boundary for their native/browser network stacks. Desktop uses the
Rust types directly. Operators provide the DHT verification result, relay
endpoints, storage gateways, and media token authority; no decentralized
network is silently replaced by a centralized fallback.
