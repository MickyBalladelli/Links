# Links

Centralized-first encrypted messaging. The foundation includes phone OTP account
authentication, Ed25519 device enrollment, hardware-backed identity custody,
WebAuthn/Passkey PRF-encrypted identity backup, the
Links X25519 + ML-KEM-768 PQXDH profile, authenticated offline pre-key
provisioning, and an OpenMLS RFC 9420 TreeKEM core. **This is not yet a working
messenger or a production post-quantum security claim.** One-to-one client
lifecycle wiring, production gateway rollout and remaining client applications
remain in later phases. Native hardware acceptance and the cryptographic audit
remain open.

## Workspace

| Path | Responsibility |
| --- | --- |
| `proto/links/v1` | Versioned protobuf message, identity, pre-key, envelope, sync and transport contracts. |
| `crates/protocol` | Generated common types, descriptors, boundary validation and bounded Zstd dictionary sync compression. |
| `crates/client-core` | Portable identity/PQXDH/MLS interfaces, encrypted conversation sequencing, PRF-encrypted passkey backup, per-device envelope fanout, background replay/decrypt, durable sync validation, Opus voice-note muxing, and shared video profiles. |
| `crates/web-client` | WASM Web identity, self-sovereign mnemonic/passkey derivation, and paired-device bootstrap facade built on `links-client-core`. |
| `crates/desktop-client` | Platform-neutral desktop identity and shared-core binding facade. |
| `crates/desktop-client/src/session.rs` | Desktop registration, encrypted sync, reconnect, and recovery session shell. |
| `crates/server-store` | PostgreSQL repository/migrations, append-only encrypted payload store, S3-compatible encrypted blob boundary, Redis Lua state adapter and memory reference adapter. |
| `crates/identity` | Self-sovereign mnemonic/passkey Ed25519 keys, signed phone/username/enrollment transcripts and MLS basic credentials. |
| `crates/account-auth` | SMS/WhatsApp Verify adapter, username-only signed registration/login, global public-key directory lookup, durable account enrollment/login, authenticated additional-device registration, WebAuthn passkeys, opaque key-backup HTTP API and sessions. |
| `crates/gateway` | Multi-region WebSocket session fencing, durable encrypted routing and configured APNs/FCM silent wakeup contracts. |
| `crates/queue` | Opaque NATS JetStream delivery wire contract and durable publish adapter for cross-region gateway routing. |
| `native/apple`, `native/android` | Hardware-backed seed-wrapping adapters, acceptance tests, iOS/Android client foundations, identity/OTP onboarding, APNs/FCM recovery, and the internal Android text-messaging shell. |
| `web` | TypeScript Web host for the shared Rust/WASM client core. |
| `docs/consumer-account.md` | Account setup, API, security boundaries and remaining hardware gates. |
| `crates/server-store/migrations/0007_pseudonymous_accounts.sql` | Allows username-only accounts to omit phone-derived authentication subjects. |
| `docs/phase-0.md` | Product scope, threat model, metadata budget, platforms and release gates. |
| `docs/contracts.md` | WebSocket choice, wire compatibility, transport and sync semantics. |
| `docs/storage.md` | PostgreSQL, payload-store and ephemeral-state guarantees and limits. |
| `docs/blob-storage.md` | S3-compatible encrypted attachment storage and Cloudflare/CloudFront edge deployment contract. |
| `docs/pqxdh.md` | Links PQXDH profile, key schedule, custody requirements and security limits. |
| `docs/prekeys.md` | Pre-key generation, automatic refill, authenticated upload and atomic claim contracts. |
| `docs/directory.md` | Global `@username` lookup, active-device public keys, revocation visibility and pre-key handoff. |
| `docs/mls.md` | OpenMLS RFC 9420 TreeKEM core, hybrid suite, credential checks and durable commit flow. |
| `docs/passkey-backup.md` | WebAuthn ceremonies, PRF-encrypted identity backup and server storage boundary. |
| `docs/pairing.md` | Canonical device-pairing QR URI, signature verification and client flow. |
| `docs/gateway.md` | Multi-region WebSocket gateway flow, routing, push fallback and deployment gates. |
| `docs/message-queue.md` | NATS JetStream subjects, opaque delivery rules and regional deployment contract. |
| `docs/android-text-milestone.md` | Internal Android one-to-one text build scope and two-device acceptance gate. |
| `docs/ios-client.md` | iOS client foundation, shared-core boundary and release limits. |
| `docs/ios-text-milestone.md` | Internal iOS one-to-one text build scope and two-device acceptance gate. |
| `docs/web-client.md` | Web/WASM client bootstrap, key custody boundary and current release limits. |
| `web/src/WebConnectionManager.ts` | Browser `wss://` connection lifecycle for binary `links.v1`. |
| `web/src/WebTextMessaging.ts` | Shared-core encrypted one-to-one Web text sync host. |
| `web/src/WebImages.ts` | Web image normalization, encrypted transfer, ciphertext cache, and render boundary. |
| `web/src/WebRtcFileTransfer.ts` | Browser WebRTC DataChannel ciphertext streaming, resume, backpressure, and integrity boundary. |
| `web/src/WebLargeFiles.ts` | Web/WASM chunked video/file encryption, staging, upload receipt, and decrypting source boundary. |
| `docs/desktop-client.md` | Desktop client foundation and shared-core integration boundary. |
| `crates/desktop-client/src/session.rs` | Desktop image encryption, transfer, ciphertext cache, and render boundary. |
| `docs/voice-notes.md` | Opus profile, Ogg container, encryption boundary and platform codec contract. |
| `docs/video-transcoding.md` | Native VideoToolbox/MediaCodec profiles and hardware-only transcode boundary. |
| `docs/p2p-file-transfer.md` | WebRTC DataChannel ciphertext-only transfer, resume, backpressure, and integrity contract. |
| `docs/large-file-encryption.md` | Chunked AEAD format and cross-client video/file transfer contract. |
| `native/apple/Sources/LinksClient/IOSVideoTranscoder.swift` | iOS hardware video decode, scale, encode, MP4 mux, and faststart boundary. |
| `native/apple/Sources/LinksClient/IOSLargeFileTransfer.swift` | iOS bounded ChaCha20-Poly1305 staging for video/files. |
| `native/apple/Sources/LinksClient/IOSLargeFileSession.swift` | iOS upload receipt, private MLS send, and decrypt orchestration. |
| `native/android/client/src/main/java/ai/links/app/AndroidVideoTranscoder.java` | Android MediaCodec surface transcode, MP4 mux, and faststart boundary. |
| `native/android/client/src/main/java/ai/links/app/Mp4FastStart.java` | Android MP4 `moov` relocation and `stco`/`co64` offset repair. |
| `native/android/client/src/main/java/ai/links/app/AndroidLargeFileTransfer.java` | Android bounded ChaCha20-Poly1305 staging for video/files. |
| `native/android/client/src/main/java/ai/links/app/AndroidLargeFileSession.java` | Android upload receipt, private MLS send, and decrypt orchestration. |

## Build and test

Install a current stable Rust toolchain with rustfmt and clippy. Cargo downloads
locked dependencies and a vendored protoc; a system protoc is not required.

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --all-targets --locked
cargo test --workspace --doc --locked
rustup target add wasm32-unknown-unknown
cargo check -p links-client-core --target wasm32-unknown-unknown --locked
cargo check -p links-web-client --target wasm32-unknown-unknown --locked
```

The ordinary test command explicitly reports PostgreSQL integration tests as
ignored. Run them separately against a disposable development database; they do
not silently pass when the database is unavailable.

## Local PostgreSQL

Docker Compose is optional; an existing disposable PostgreSQL 18 instance also
works. These example credentials and the loopback binding are for development.

```sh
cp .env.example .env
# Review .env before loading it. Change both password and URLs together if needed.
set -a; . ./.env; set +a
docker compose up -d --wait postgres
cargo run -p links-server-store --example migrate --locked
cargo test --workspace --all-targets --locked -- --ignored
```

The migration command targets `DATABASE_URL`. Integration tests use
`LINKS_TEST_DATABASE_URL`, create a random isolated schema for each test, and drop
only those schemas afterward. The test role needs CREATE SCHEMA permission. Never
point these development commands at production. `docker compose down` stops the
local service while retaining its named data volume.

CI is configured for formatting, warning-free lint, unit/doc tests, PostgreSQL
integration tests, WASM compilation and native-source builds. Hardware custody
tests require signed physical-device harnesses; compile checks do not prove TEE
protection. Mobile FFI bindings, Redis and ScyllaDB/DynamoDB network adapters remain
future work. The default outer envelope provider still fails closed; install the
platform-backed Sealed Sender resolver before sending. See [account setup](docs/consumer-account.md),
[MLS](docs/mls.md), and [the roadmap](TODO.md).
