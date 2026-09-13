# TODO.md: Master Architecture & Implementation Roadmap for "Links"

This is the build order for **Links**, an end-to-end encrypted, post-quantum resilient messaging platform supporting text, voice notes, images, videos, 1-to-1, many-to-many, and 1-to-many communication.

## Recommended build order

Build the centralized product first. Make one shared protocol and crypto core, then build clients in this order:

1. Shared client core
2. Android consumer client
3. iOS consumer client
4. Web companion client
5. Desktop client
6. Channels, business, and bot clients
7. Decentralized clients and network support

Ship in small gates: reliable 1-to-1 text first, then media, groups, calls, channels, and finally decentralized infrastructure.

---

## Phase 0 — Product scope, contracts, and shared core

Centralized v1 is the first release target. Decentralized support comes after the centralized protocol and client behavior are proven.

**Status: Complete — foundation implemented and verified.** See [scope and threat model](docs/phase-0.md), [protocol contracts](docs/contracts.md), and [storage contracts](docs/storage.md).

- [x] Define the v1 threat model, metadata privacy goals, supported platforms, and release gates.
- [x] Choose **Scenario A: Centralized Architecture** for v1.
- [x] Define Protocol Buffers schemas (`.proto`) for `Message`, `User`, `MediaMetadata`, and `Receipts`.
- [x] Create a shared client core library for identity, cryptography, MLS, envelopes, sync, and common protocol types.
- [x] Choose the initial bidirectional transport: **WebSockets / gRPC over HTTP/2**. Selected WebSocket over TLS for clients; gRPC reserved for future internal RPCs.
- [x] Design PostgreSQL schemas for user accounts, handle registry, device node mappings, and group membership RBAC.
- [x] Build the initial relational data store using PostgreSQL.
- [x] Define the encrypted payload storage interface for ScyllaDB / DynamoDB.
- [x] Build the in-memory state interface for Redis session sockets, routing tables, and rate-limit buckets.

Verification: 17 unit tests and 6 PostgreSQL 18.4 integration tests passed, including concurrent handle claims and last-owner RBAC protection. Formatting, warning-free Clippy, doc-test command, and the shared-core WASM compile check passed. CI and a local PostgreSQL Compose configuration are included; hosted CI and Docker Compose were not executed locally.

Phase 0 supplies crypto/MLS provider contracts with fail-closed defaults, not production cryptography. Redis has a bounded local reference adapter; ScyllaDB/DynamoDB have a storage contract, not deployed adapters. Those implementations remain in Phases 1–2.

---

## Phase 1 — Consumer identity and E2EE foundation

Build the security foundation needed for the first two clients and 1-to-1 messaging.

### Standard consumer account

- [x] Implement SMS / WhatsApp OTP authentication flow.
- [x] Generate Ed25519 identity keypairs and MLS credentials bound to verified phone accounts, using CSPRNG seeds rather than phone-derived seeds.
- [x] Secure identity keys using native device hardware keystores: iOS Secure Enclave and Android Keystore TEE.
  - [ ] Complete signed physical-device acceptance on iOS and Android before public release.


Implementation: `crates/account-auth` provides the Verify-backed HTTP flow, durable rate-limited challenges, signed device enrollment, returning-device login, hashed expiring sessions, and revocation checks. `crates/identity` provides Ed25519 operations and RFC 9420 basic credentials. See [consumer account setup and contracts](docs/consumer-account.md).

The hardware implementation includes Apple Secure Enclave P-256 and Android Keystore AES-GCM seed wrapping, the Rust vault interface, and Swift/C plus Android JNI wiring through `crates/identity-ffi`. Native identity APIs create, restore/validate, sign against the enrolled public key, and delete without exporting seeds to callers or falling back to software storage. Android adds secure-screen-lock checks and bounded crash-recovering record reads. Signed physical-device acceptance remains a separate release gate; see [hardware identity builds, verification and acceptance](docs/hardware-identity.md). Ed25519 signing still occurs briefly in app memory after hardware unwrap, not inside Secure Enclave/TEE.

Hardware integration verification: 32 Rust unit tests passed; formatting, warning-free Clippy, doc tests and the shared-core WASM check passed. Six Apple tests passed with three hardware tests skipped; the Swift/Rust library linked for arm64 iOS. Rust archives cross-built for arm64/x86_64 Android, the real JNI/Rust bridge passed host JVM lifecycle/signature/failure/wiping tests, and Android production Java compiled against API classes. Full Gradle/NDK packaging, signed physical-device tests, process-restart and locked-device acceptance remain unverified. The 14 PostgreSQL tests were not rerun for this hardware-only change.

Verification: 25 Rust unit tests and 14 PostgreSQL integration tests passed; formatting, Clippy, doc-test commands and WASM compilation passed. Three Apple non-hardware tests and iOS typechecking passed; Android production source compiled against Android API classes. The opt-in Secure Enclave round-trip test was blocked by missing Keychain entitlement (`-34018`). Full Android Gradle/device tests require the Android SDK and a physical TEE/StrongBox test harness. Live SMS/WhatsApp delivery was not exercised; provider credentials and explicit acceptance sends remain required.

### Post-quantum session security

- [x] Implement PQXDH pairing X25519 with ML-KEM-768 (standardized Kyber-768 successor).
- [x] Build Pre-Key Bundle generation and automatic upload for offline message initiation.
- [x] Integrate the core MLS ratcheting engine with TreeKEM (`O(log N)` member update complexity).
- [x] Standardize 1-to-1 chats as 2-member MLS groups.
- [x] Wrap encrypted payloads in Sealed Sender envelopes to hide origin metadata from routing nodes.

PQXDH implementation: `links-client-core::pqxdh` provides the Links X25519 +
ML-KEM-768 + HKDF-SHA-512 profile, signed prekey transcripts, initiator/responder
agreement, strict key/ID validation, one-time-key consumption markers and
zeroized secret buffers. Account authentication uses a separate hardware-backed
Ed25519 key. `links-client-core::prekeys` adds hardware-vault persistence contracts,
durable automatic refill, claimed-bundle verification, and protobuf upload/status
contracts. The account API verifies signatures and PostgreSQL atomically consumes
one-time keys, with last-resort ML-KEM fallback. OpenMLS now provides the RFC 9420
TreeKEM ratchet, encrypted MLS application messages, verified device credentials,
ratchet-tree extension transport, and durable-provider/pending-commit contracts
through `links-client-core::mls`. The selected ML-KEM-768 + X25519 ciphersuite is
an OpenMLS draft suite, so interoperability testing and independent review remain
release gates. Direct groups are ready only when exactly two distinct user
identities are present; each physical device may still occupy its own MLS leaf.
Authenticated first-payload composition remains in the following roadmap items.
The server’s direct-group metadata also caps account members at two.
`crypto::SealedSenderCrypto` now wraps the opaque MLS bytes with an ephemeral
X25519 key and authenticated ChaCha20-Poly1305 ciphertext; recipient device
keys resolve through a platform-backed provider, while the routing header stays
visible and sender/conversation metadata stays inside MLS.
See [PQXDH profile and security
limits](docs/pqxdh.md), [pre-key provisioning](docs/prekeys.md), and [MLS/TreeKEM](docs/mls.md).

### Account recovery

- BIP-39 recovery is local-only: `links-identity` accepts English 12/24-word
  mnemonics, applies standard PBKDF2-HMAC-SHA512 with an optional passphrase,
  and domain-separates the root into an Ed25519 identity seed. Mnemonics and
  derived seeds stay in zeroizing memory; server-side account recovery and
  restored pre-key inventory remain separate work.

- [x] Implement BIP-39 12/24-word seed phrase key derivation for zero-knowledge device restoration.
- [x] Build WebAuthn / Passkey cloud backup for hardware-bound private keys without unencrypted server state.

Passkey backup is client-side encrypted with the WebAuthn PRF output and a
random per-backup salt. `links-client-core::passkey_backup` binds the ciphertext
to the backup ID, device ID and credential ID before sealing the hardware-loaded
Ed25519 seed. `crates/account-auth` validates WebAuthn create/get ceremonies and
monotonic authenticator counters, while PostgreSQL stores only passkey public
keys, challenges and opaque encrypted envelopes. The PRF output and plaintext
seed never enter the server API. Use `AccountAuth::new_with_passkey` with an
explicit RP ID and exact origin; ordinary `new` leaves passkey routes disabled.

---

## Phase 2 — Centralized delivery, sync, and storage

Make encrypted messages reliably move between devices before adding richer client features.

### Edge and routing infrastructure

- [x] Deploy multi-region WebSocket connection gateways managing client sockets and APNs / FCM fallbacks.
-  `links-gateway` provides the strict WebSocket frame/session state machine,
   cross-region forwarding and silent push fallback contracts. Production Redis,
   bus, APNs/FCM adapters and cloud rollout remain an operational release gate.
- [x] Deploy distributed message queues using NATS, Kafka, or RabbitMQ to route encrypted Sealed Sender envelopes without inspecting contents.
-  NATS JetStream is the first queue target. `links-queue` defines the opaque
   protobuf delivery wrapper, exact per-gateway subjects and durable-publish
   adapter. Cluster provisioning, mTLS credentials and the concrete consumer
   client remain an operational release gate.
- [x] Build ephemeral Redis state for active WebSocket sessions, routing tables, and rate-limit buckets.
-  `RedisEphemeralState` provides single-key Lua/CAS session fencing, Redis
   server-time expiry, per-device routing, and atomic token buckets. A concrete
   TLS Redis client implementing `RedisScriptExecutor` and production rollout
   remain deployment work.
- [x] Deploy the append-only encrypted payload store for undelivered envelopes.
-  `RelationalStore` now implements the encrypted mailbox with transactional
   per-device cursors, envelope-id idempotency, replay tombstones, cumulative
   acknowledgement and bounded expiry compaction. PostgreSQL hardening,
   encrypted-at-rest configuration, backups and recovery validation remain
   deployment gates.

### Synchronization and multi-device state

- [x] Assign strictly increasing `sequence_id` / `cursor_id` counters per conversation and user queue.
-  `sequence_id` is assigned inside E2EE per `(conversation_id, sender_device_id)`;
   `cursor` is assigned transactionally per recipient-device mailbox by the
   payload store. Both reject reuse and preserve replay ordering without
   exposing conversation metadata to routing services.
- [x] Implement connection replay: the client sends `last_seen_cursor`, and the server returns the missing delta payloads.
-  `Gateway::open` now reads the authenticated device mailbox after Hello and
   returns Welcome plus the first contiguous SyncBatch. Explicit Replay remains
   available for pagination; stale cursors fail with CursorExpired.
- [ ] Register every physical client as a distinct MLS identity node under the primary user account.
- [ ] Fan out outbound encrypted envelopes to all active device queues for target users.

### Push and retention

- [ ] Configure silent / data-only APNs and FCM pushes carrying only `conversation_id` and `sequence_id`.
- [ ] Implement client background workers that fetch and decrypt missing payloads over TLS/E2EE channels.
- [ ] Purge encrypted blobs after receipt confirmation (`delivery_receipt`).
- [ ] Add TTL deletion for uncollected offline messages, with a 30-day maximum retention target.

---

## Phase 3 — Android consumer client first

The first usable client. Keep the first slice small: phone account, 1-to-1 text, reliable delivery, and recovery.

- [ ] Build the Android client on top of the shared client core.
- [ ] Add phone OTP onboarding and Android Keystore key storage.
- [ ] Implement Android connection manager with exponential backoff, reconnect, and heartbeating.
- [ ] Implement the 1-to-1 send flow: query recipient pre-key bundle, establish/resume MLS group, encrypt payload, and send envelope.
- [ ] Implement the 1-to-1 receive flow: fetch envelope, decrypt Sealed Sender wrapper, process MLS epoch update, render message, and return delivery receipt.
- [ ] Add FCM background fetch and missing-message recovery.
- [ ] Add zero-knowledge account recovery using seed phrase and passkey flows.
- [ ] Release an internal Android text-messaging milestone before adding groups or calls.

---

## Phase 4 — iOS consumer client

Bring iOS to feature parity with Android. Validate hardware-backed keys and background delivery early.

- [ ] Build the iOS client on top of the shared client core.
- [ ] Add phone OTP onboarding and Secure Enclave key storage.
- [ ] Implement iOS connection manager with exponential backoff, reconnect, and heartbeating.
- [ ] Implement the same 1-to-1 send and receive flows as Android.
- [ ] Add APNs background fetch and missing-message recovery.
- [ ] Add zero-knowledge account recovery using seed phrase and passkey flows.
- [ ] Release an internal iOS text-messaging milestone before adding groups or calls.

---

## Phase 5 — Web companion and desktop clients

These clients join an existing account as additional MLS device nodes. They do not become the first primary identity client.

### Device pairing and discovery

- [ ] Build QR code generation and parsing for public identity keys and signature payloads (`links://connect?...`).
- [ ] Pair a Web or desktop device with a mobile device and register it as an MLS identity node.

### Web companion client

- [ ] Build the Web client on top of the shared client core.
- [ ] Implement Web connection manager with exponential backoff, reconnect, and heartbeating.
- [ ] Implement encrypted 1-to-1 text sync across mobile and Web.

### Desktop client

- [ ] Build the desktop client on top of the shared client core.
- [ ] Implement desktop device registration, encrypted sync, reconnect, and recovery behavior.

---

## Phase 6 — Media and payload efficiency

Add media after text delivery and multi-device sync are stable.

### Text and synchronization

- [ ] Integrate Zstandard (Zstd) dictionary compression for bulk state synchronization.

### Voice notes

- [ ] Integrate Opus audio in `.ogg` / `.opus` containers at 16–24 kbps.
- [ ] Enable VBR and DTX to suppress quiet pauses.
- [ ] Add voice-note recording, upload, download, decryption, and playback to Android and iOS.

### Images

- [ ] Resize images client-side to a maximum 1600px longest edge.
- [ ] Strip EXIF location metadata.
- [ ] Transcode to WebP / AVIF at approximately 80% lossy quality.
- [ ] Embed low-resolution BlurHash placeholders in text payloads.
- [ ] Deploy S3-compatible encrypted blob storage with Cloudflare / CloudFront CDN edge caching.
- [ ] Add encrypted image send, receive, caching, and rendering to Android and iOS, then Web and desktop.

### Videos and large files

- [ ] Transcode video with native hardware acceleration: iOS VideoToolbox and Android MediaCodec.
- [ ] Use H.264/H.265 MP4 with target bitrates of 720p at 1.5 Mbps and 1080p at 3.0 Mbps.
- [ ] Place the `moov` atom at the file start (`faststart`).
- [ ] Implement WebRTC DataChannel P2P direct file streaming for uncapped large transfers.
- [ ] Add encrypted video and large-file transfer to clients after images and voice notes work.

---

## Phase 7 — Private discovery, account types, and anti-spam

Add discovery options after the basic phone-based account works.

### Pseudonymous account

- [ ] Implement `@username` self-registration without requiring a phone number.
- [ ] Support self-sovereign key generation with passkeys / BIP-39 mnemonic seed phrases.
- [ ] Deploy global key directory lookup for `@usernames`.

### Private contact discovery

- [ ] Hash local address-book phone numbers with Argon2id and a client salt.
- [ ] Build the server/network PSI zero-knowledge matching API without leaking full contact books.

### Anti-spam

- [ ] Integrate Privacy Pass anonymous blind signatures to rate-limit new chat requests without tracking identities.
- [ ] Implement client-side proof-of-work micro-challenges for unverified accounts initiating 1-to-1 connections.

---

## Phase 8 — Groups, channels, business, and bot accounts

Add richer communication modes once 1-to-1 messaging, identity, sync, and media are reliable.

### Many-to-many groups

- [ ] Enable MLS group messaging for many-to-many conversations.
- [ ] Implement group membership updates, device changes, epoch processing, and RBAC.
- [ ] Extend the send and receive flows to many-to-many groups.

### Broadcast channels

- [ ] Configure MLS broadcast profiles where subscribers join as passive/read-only leaves.
- [ ] Enforce Ed25519 signatures on all broadcast posts by admin devices.
- [ ] Implement the publish flow: admin signs post, encrypts with broadcast master key, and dispatches to the broker.
- [ ] Implement the receive flow: subscriber fetches signed payload, verifies the admin signature, decrypts, and renders.

### Organization accounts

- [ ] Support multi-device and multi-admin key delegation through signed sub-certificates.
- [ ] Implement cryptographic proof-of-verification badges.
- [ ] Build channel, business, and bot client surfaces for Android, iOS, Web, and desktop.

---

## Phase 9 — Real-time voice, video, and live streams

Add real-time media after messaging and file media are stable.

- [ ] Implement WebRTC session SDP exchange.
- [ ] Implement SFrame frame-level encryption hooks through the WebRTC Encoded Transform API.
- [ ] Transmit SFrame epoch keys through the MLS control channel.
- [ ] Deploy managed global SFU clusters using LiveKit or Mediasoup.
- [ ] Configure SFUs to route encrypted media frames using unencrypted RTP headers without decrypting media.
- [ ] Add WebTransport (QUIC) as a fallback channel for low-latency media signaling in high-packet-loss environments.
- [ ] Implement the complete call flow: SDP exchange, MLS key exchange, client-side frame encryption, and SFU streaming.
- [ ] Add voice/video calls and live streams to mobile first, then Web and desktop.

---

## Phase 10 — Mini-app and bot runtime

Add programmable features only after the account, permission, and messaging boundaries are stable.

- [ ] Embed a client-side WebAssembly (WASM) sandbox runtime in mobile and desktop clients.
- [ ] Implement a fine-grained Mini-App permission SDK that restricts direct network calls and isolates cryptographic keys.
- [ ] Expose Mini-Apps and bots through organization account controls.

---

## Phase 11 — Decentralized / federated / P2P network

This is a separate expansion track. Do not block the centralized release on it.

### Protocol and identity

- [ ] Select one protocol layer: federated server nodes (Matrix-style), decentralized relay network (Nostr/XMTP-style), or P2P Libp2p mesh.
- [ ] Implement W3C Decentralized Identifiers (DIDs) or smart-contract key registries on Solana / Base to map `@handles` to public identity keys.

### Routing and storage

- [ ] Implement Libp2p / PubSub or relay gossip for cross-node envelope propagation.
- [ ] Deploy store-and-forward offline buffering nodes across independent relays.
- [ ] Integrate IPFS / Arweave / Filecoin for client-side encrypted chunk storage and content-addressed retrieval (`ipfs://CID`).

### Decentralized real-time media

- [ ] Build SFU node discovery through Distributed Hash Tables (DHT).
- [ ] Implement open-node or token-incentivized media relay networks for WebRTC call routing.
- [ ] Add decentralized transport, storage, and media support to the existing Android, iOS, Web, and desktop clients.

---

## Phase 12 — Performance, security, and release gates

Run these checks continuously at the relevant phase boundary, with the full audit before public launch.

- [ ] Conduct a third-party cryptographic code audit of PQXDH, MLS ratcheting, and SFrame integration.
- [ ] Verify the zero-knowledge routing stance for centralized and decentralized deployments.
- [ ] Centralized benchmark: verify less than 50ms delivery latency for 1M concurrent WebSocket connections.
- [ ] Decentralized benchmark: measure multi-hop gossip propagation across 50 international nodes.
- [ ] Review battery use, reconnect behavior, offline delivery, key recovery, and multi-device removal before public release.
