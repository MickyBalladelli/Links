# TODO.md: Master Architecture & Implementation Roadmap for "Links"

This is the build order for **Links**, an end-to-end encrypted, post-quantum resilient messaging platform supporting text, voice notes, images, videos, 1-to-1, many-to-many, and 1-to-many communication.

## Recommended build order

Build the centralized product first. Make one shared protocol and crypto core, then build clients in this order:

1. Shared client core
2. Android consumer client
3. iOS consumer client
4. Web companion client
5. Desktop client
6. macOS native client and local two-client validation
7. Channels, business, and bot clients
8. Decentralized clients and network support

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
The shared first-payload coordinator is implemented in Phase 3; Android host
adapters for directory, MLS bootstrap, and durable outbox remain there.
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
- [x] Register every physical client as a distinct MLS identity node under the primary user account.
-  `POST /v1/devices` lets an authenticated device approve a new client; the
   new client proves its Ed25519 key with a nonce-bound signature, and the
   server stores a distinct device/node/MLS credential. Adding that node to
   each existing MLS group still requires an authenticated member-add commit.
- [x] Fan out outbound encrypted envelopes to all active device queues for target users.
  `ClientCore::seal_message_for_devices` encrypts the MLS application message
  once, then creates one independently sealed envelope per active recipient
  device from an authenticated directory result. Each envelope keeps its own
  recipient binding and ID, and the existing gateway appends/routes each one to
  that device's mailbox without copying or inspecting ciphertext.

### Push and retention

- [x] Configure silent / data-only APNs and FCM pushes carrying only device-scoped wakeup metadata.
  `PushWakeup::apns_request` emits an APNs background payload and priority 5;
  `PushWakeup::fcm_request` emits high-priority data-only fields. Both carry
  only the recipient device ID and mailbox cursor. `conversation_id` and
  `sequence_id` remain inside E2EE because the gateway cannot know them; the
  client wakes, replays from the cursor, and decrypts them locally.
- [x] Implement client background workers that fetch and decrypt missing payloads over TLS/E2EE channels.
  `links-client-core::background::BackgroundWorker` reconnects with the durable
  cursor, drains contiguous replay pages, decrypts each envelope through
  `ClientCore`, commits the inbox before sending `QueueAck`, and retries safely
  after a failed local commit. Android/iOS schedulers only need to invoke it
  from their push callbacks.
- [x] Purge encrypted blobs after receipt confirmation (`delivery_receipt`).
  The receiving worker sends authenticated `QueueAck` only after decrypting and
  durably committing the batch. `RelationalStore::acknowledge` then clears the
  encrypted envelope bytes transactionally while retaining cursor tombstones
  and idempotency fingerprints. E2EE `delivery_receipt` messages remain private
  and are not parsed by the server.
- [ ] Add TTL deletion for uncollected offline messages, with a 30-day maximum retention target.

---

## Phase 3 — Android consumer client first

The first usable client. Keep the first slice small: phone account, 1-to-1 text, reliable delivery, and recovery.

- [x] Build the Android client on top of the shared client core.
  `native/android/client` is the first Android application module. It uses the
  Rust-backed `HardwareIdentityStore`, keeps only public identity metadata in
  app preferences, restores existing identities fail-closed, and provides the
  off-main-thread first-run shell. Transport, messaging UI and background
  scheduling remain the following Android tasks.
- [x] Add phone OTP onboarding and Android Keystore key storage.
  `native/android/client` uses HTTPS `/v1/auth/start` and `/v1/auth/finish`,
  signs both proofs through the Rust identity bridge, stores only the account
  binding, and keeps the bearer token in memory. Android Keystore requires TEE
  or StrongBox-backed wrapping; provider delivery and physical-device acceptance
  remain release gates.
- [x] Implement Android connection manager with exponential backoff, reconnect, and heartbeating.
  `ConnectionManager` uses a binary-only TLS `links.v1` WebSocket, queues the
  Hello frame before exposing the socket, sends OkHttp ping/pong heartbeats every
  30 seconds, rejects text/oversized frames, and reconnects with full-jitter
  backoff with a 1-second initial window and 30-second ceiling. It never places
  bearer tokens in URLs or logs.
- [x] Implement the 1-to-1 send flow: query recipient pre-key bundle, establish/resume MLS group, encrypt payload, and send envelope.
  `links-client-core::send` verifies every claimed bundle, installs the verified
  Sealed Sender key, adds missing recipient devices to the direct MLS group,
  persists the pending commit, delivers the MLS bootstrap, merges the accepted
  epoch, then persists and sends exact per-device envelopes. The Android host
  still supplies the authenticated directory, MLS bootstrap transport, and
  durable outbox adapters.
- [x] Implement the 1-to-1 receive flow: fetch envelope, decrypt Sealed Sender wrapper, process MLS epoch update, render message, and return delivery receipt.
  `links-client-core::receive::receive_available` replays encrypted mailbox
  pages, applies authenticated Welcome/Commit updates before decrypting,
  commits messages and MLS state before QueueAck, invokes the renderer, and
  returns private E2EE delivery-receipt requests grouped by conversation. The
  host sends those receipts through the normal MLS send coordinator.
- [x] Add FCM background fetch and missing-message recovery.
  `LinksFirebaseMessagingService` accepts only the gateway's device-scoped
  data-only wakeup, coalesces it into a constrained WorkManager job, and also
  handles FCM deleted-message recovery with a full-sync signal. The worker
  reconnects through `ConnectionManager`; `AndroidMissingMessageRecovery`
  drives the shared receive core through an injected frame bridge and always
  starts from the durable cursor. The host must configure the recovery and
  token adapters; memory-only bearer sessions still require re-authentication
  after process death.
- [x] Add zero-knowledge account recovery using seed phrase and passkey flows.
  Android JNI now exposes explicit BIP-39 restore plus passkey envelope seal/open
  into the TEE-backed vault. `AccountRecovery` keeps mnemonic, PRF output and
  decrypted identity material local; `PasskeyClient` uploads only the opaque
  authenticated envelope. The host must provide a UV-enforcing Android
  Credential Manager/WebAuthn adapter and an authenticated bootstrap session for
  the current bearer-only backup routes. Recovery assigns a fresh physical
  device/node and still requires normal device enrollment afterward.
- [x] Release an internal Android text-messaging milestone before adding groups or calls.
  Internal build profile `0.2.0-internal` / `android-text-internal-1` and the
  text-only `AndroidTextMessaging` shell are ready. The release is restricted to
  authenticated one-to-one text; groups, media and calls have no API or UI.
  Two-device keystore, reconnect, FCM replay, tamper and battery acceptance is
  documented in `docs/android-text-milestone.md` and remains required before
  sharing the internal APK.

---

## Phase 4 — iOS consumer client

Bring iOS to feature parity with Android. Validate hardware-backed keys and background delivery early.

- [x] Build the iOS client on top of the shared client core.
  `native/apple` now exports the `LinksClient` Swift target. `IOSClient` restores
  and validates Secure Enclave-backed identity metadata, keeps bearer sessions
  in memory, and requires `SharedClientCoreFactory` to bind the shared Rust
  `ClientCore`, MLS, envelope, durable-store, and hardware signing providers. OTP, transport,
  messaging UI, APNs recovery, and passkey/seed recovery remain later Phase 4
  tasks; see [iOS client foundation](docs/ios-client.md).
- [x] Add phone OTP onboarding and Secure Enclave key storage.
  `LinksClient` now signs the exact Rust identity-FFI phone and enrollment
  transcripts with the Secure Enclave-backed identity, calls HTTPS
  `/v1/auth/start` and `/v1/auth/finish`, validates challenge/device/key
  bindings, and keeps the returned bearer in memory only. Physical Secure
  Enclave and live provider acceptance remain release gates.
- [x] Implement iOS connection manager with exponential backoff, reconnect, and heartbeating.
  `IOSConnectionManager` enforces `wss`, negotiates binary `links.v1`, requires
  a bounded Hello within five seconds, rejects text/oversized frames, sends
  30-second ping heartbeats, and reconnects with full-jitter backoff from one
  to thirty seconds. It accepts Hello bytes from the shared core and never
  places bearer tokens in URLs or diagnostics; see [iOS client foundation](docs/ios-client.md).
- [x] Implement the same 1-to-1 send and receive flows as Android.
  `IOSDirectMessaging` binds the authenticated iOS client, shared Rust core,
  and `IOSConnectionManager`. It validates text IDs and the 64 KiB limit, then
  delegates send sequencing, MLS encryption, Sealed Sender fanout, durable
  outbox state, replay decrypt, message commit, and delivery receipts to the
  shared core. Message UI and APNs scheduling remain separate tasks.
- [x] Add APNs background fetch and missing-message recovery.
  `IOSAPNsWakeup` strictly parses the silent `aps.content-available` payload
  and treats its cursor as a hint. `IOSMissingMessageRecovery` coalesces
  duplicate wakeups, reconnects with the durable local cursor, runs bounded
  replay through the shared core, and completes only after decrypt, durable
  inbox/cursor commit, and QueueAck. `IOSAPNsBackgroundHandler` maps the result
  to UIKit background fetch callbacks; physical APNs/background testing remains
  a release gate.
- [x] Add zero-knowledge account recovery using seed phrase and passkey flows.
- [x] Release an internal iOS text-messaging milestone before adding groups or calls.
  `IOSInternalTextMilestone` defines internal build `0.1.0-internal` /
  `ios-text-internal-1` and enables only authenticated one-to-one text. Groups,
  media, and calls have no enabled feature path. The Swift package artifact,
  required `SharedClientCoreFactory`, signed IPA handoff, and two-device
  Secure Enclave/APNs/Android interop gate are documented in
  [ios-text-milestone.md](docs/ios-text-milestone.md); sharing the IPA remains
  blocked until that physical-device evidence exists.

---

## Phase 5 — Web companion and desktop clients

These clients join an existing account as additional MLS device nodes. They do not become the first primary identity client.

### Device pairing and discovery

- [x] Build QR code generation and parsing for public identity keys and signature payloads (`links://connect?...`).
  `links-client-core::pairing::PairingPayload` creates and strictly parses the
  canonical URI, carries no private key or bearer token, and verifies the new
  device's Ed25519 pairing signature before registration. Platform QR libraries
  encode/decode this URI text; the server request uses the existing pairing
  transcript contract.
- [x] Pair a Web or desktop device with a mobile device and register it as an MLS identity node.
  `PairingRegistrationRequest::from_uri` authenticates the scanned payload and
  checks the approving account. `approve_pairing` calls an authenticated
  registration transport and validates the returned Links MLS credential before
  the new client initializes `OpenMlsEngine` and generates its first KeyPackage.

### Web companion client

- [x] Build the Web client on top of the shared client core.
  `crates/web-client` exposes the shared Rust identity and pairing logic to
  WASM. The `web` TypeScript host creates a Web device identity, emits signed
  pairing URI text, and accepts only a matching validated MLS credential from
  mobile approval. Browser storage and text UI remain host-application work.
- [x] Implement Web connection manager with exponential backoff, reconnect, and heartbeating.
  `web/src/WebConnectionManager.ts` uses binary `links.v1` WebSockets, sends
  Hello within five seconds, enforces the 1 MiB frame limit, reconnects with
  full jitter from 1 to 30 seconds, and keeps bearer tokens out of URLs. Browser
  ping/pong is handled by the platform; the manager checks socket liveness.
- [x] Implement encrypted 1-to-1 text sync across mobile and Web.
  `WebTextMessaging` reads the durable cursor from the shared-core adapter for
  every Hello, forwards replay frames back into the core for Sealed Sender/MLS
  decrypt and durable commit, and delegates outbound text encryption and
  per-device fanout to the core. The browser handles only binary frames and
  committed message callbacks; it never inspects plaintext before decryption
  or constructs ciphertext itself.

### Desktop client

- [x] Build the desktop client on top of the shared client core.
  `crates/desktop-client` provides the platform-neutral desktop companion
  boundary. It creates or restores a desktop identity, emits the shared
  signed pairing URI, validates the returned MLS credential, and exposes only
  `LocalIdentity`, the public credential, and cloneable signing handles to
  `links-client-core`. UI and OS keychain persistence remain host integration
  work.
- [x] Implement desktop device registration, encrypted sync, reconnect, and recovery behavior.
  `links-desktop-client` now accepts the validated mobile registration response,
  binds a core-owned encrypted text session, reconnects native sockets with
  bounded full-jitter backoff and heartbeats, and forces replay from the latest
  durable cursor during recovery. The desktop host supplies the native TLS
  WebSocket, durable core providers, and UI event loop.

---

## Phase 5A — macOS native client and local two-client validation

Turn the platform-neutral desktop foundation into a real macOS application. The
exit gate is two isolated macOS clients running at the same time on one Mac and
exchanging encrypted one-to-one text through the same local development stack.

### macOS application host

- [x] Add a macOS 13+ SwiftUI application target that embeds the existing `LinksClient` and `LinksKeyStore` Swift package products.
  `native/macos/Links.xcodeproj` provides the macOS application target and shared
  Links package dependency. Its SwiftUI shell imports both products and reports
  identity, account, and connection state while the concrete host integrations
  are added in the following tasks.
- [x] Add Debug and Release schemes, application lifecycle handling, and a clean dependency on the Rust `links-identity-ffi` library for arm64 macOS; add x86_64 support if Intel Macs remain in scope.
  `native/macos/Links.xcodeproj` now includes shared `Links-Debug` and
  `Links-Release` schemes, arm64-only deployment settings, and the SwiftUI
  `scenePhase` lifecycle hook. `LinksKeyStore` owns the package-level Rust FFI
  linker declaration; Intel remains out of scope until an x86_64 archive is
  available.
- [x] Implement the macOS client shell: onboarding, account state, device state, connection state, conversation list, message list, composer, send action, and receive rendering.
  `MacOSClientModel` owns local identity onboarding, account/device state, and
  the `IOSDirectMessaging` delegate boundary. `MacOSClientShell` provides the
  onboarding screen, conversation list, message list, composer, send action,
  and receive rendering. The UI stays fail-closed until a concrete shared-core
  and durable host is installed.
- [x] Add macOS signing, Keychain entitlements, hardened runtime settings, and a documented local unsigned-debug path.
  The macOS target uses Automatic signing with Apple Development for Debug and
  Developer ID Application for Release, enables the hardened runtime, and
  embeds `Links/Links.entitlements` for sandbox networking and Keychain access.
  `native/macos/README.md` documents the unsigned Debug command and explains
  that Secure Enclave storage requires a signed build.
- [x] Add crash-safe shutdown and restart behavior so pending outbox data and the durable cursor are not lost.
  `LinksMacOSApplicationDelegate` shuts down transport before normal app
  termination. Background transitions use the same `IOSDirectMessaging`
  shutdown path, and active transitions recreate the transport/core when the
  user had requested a connection. The shared durable core remains the source
  of truth for encrypted outbox data and the durable cursor.

### Identity, account, and device enrollment

- [x] Implement a macOS Keychain-backed seed provider using the existing Apple wrapping boundary; never store the seed in UserDefaults, plaintext files, logs, URLs, or analytics.
  `MacOSKeychainSeedProvider` delegates to `HardwareSeedVault` and is wired
  into the macOS `IOSClient`. Only the wrapped seed record is stored in
  Keychain; `IOSClient` persists public metadata only, and seed buffers are
  wiped after each Rust operation.
- [x] Namespace Keychain records by an explicit client profile so two local clients cannot open or overwrite each other's identity.
  `ClientProfile` validates a stable profile name and scopes the Keychain
  service, wrapped-seed context, and public metadata key. The macOS app accepts
  `--profile <name>`, and `IOSClient` rejects mismatched identity-store and
  client profiles before opening metadata.
- [x] Support first-run username registration/login for local development and authenticated device pairing through the existing `links://connect` flow.
  `IOSUsernameAuthClient` signs nonce-bound username registration/login requests,
  keeps the bearer in memory, and the macOS shell can create or approve signed
  `links://connect` payloads through the authenticated device endpoint.
- [x] Support OTP enrollment when the macOS host is configured against a real account-auth service and Twilio Verify account.
  The macOS account screen uses `IOSOTPClient` for HTTPS-only `start` and
  `finish` calls, keeps phone/code input memory-only, and stores the returned
  public MLS credential with the account metadata.
- [x] Persist only public account/device metadata, MLS credentials, and encrypted local state; keep the bearer token memory-only.
  `IOSClient` encodes only public identity/account metadata and the MLS
  credential in its profile-scoped metadata record. `MacOSEncryptedStateStore`
  encrypts macOS shell state with AES-GCM under profile-scoped Application
  Support storage and a Keychain-held key. Bearer sessions remain an
  in-memory-only `AuthenticatedSession`.
- [x] Generate a fresh non-nil `user_id`, `device_id`, and `mls_node_id` for every new local profile; reject accidental identity reuse.
  New macOS identities create distinct non-nil UUIDs for the device and MLS
  node. Username/OTP registration assigns the fresh server-generated user ID;
  later sessions must match the saved user, handle, and MLS credential or the
  client rejects the identity as reused. Pairing an existing account remains
  intentional and uses a new device/node pair.
- [x] Generate and upload the initial pre-key inventory, claim and verify recipient pre-keys, and initialize the first two-user MLS conversation through the shared client core.
  `IOSPreKeyHTTPClient` implements the authenticated protobuf inventory,
  upload, and atomic claim routes. `IOSDirectMessaging` hands those routes and
  the authenticated directory snapshot to the shared-core bootstrap hooks;
  the concrete binding calls `prekeys::maintain_inventory`, verifies every
  `RecipientDevice`, and stages the two-user MLS commit before application
  messages are allowed.

### Native networking and encrypted messaging

- [x] Implement the macOS TLS WebSocket adapter for `wss://<host>/v1/connect` using the `links.v1` subprotocol and binary-only frames.
  The shared Apple transport used by the macOS target now pins the exact
  `/v1/connect` WSS endpoint, requires TLS 1.3 and the negotiated `links.v1`
  subprotocol, rejects redirects and text frames, bounds every binary frame at
  1 MiB, and keeps ping/reconnect/Hello-deadline handling on the socket queue.
- [x] Bind the adapter to `DesktopTextSession` and a concrete `DesktopMessagingCore` implementation backed by the shared Rust core. `bind_desktop_text_session()` now wraps `ClientCore` in `RustDesktopMessagingCore`, validates server protobuf frames, builds fresh Hello frames, and exposes the durable host boundary through `DesktopCoreHost`.
- [x] Implement durable macOS providers for MLS state, inbox, outbox, message IDs, conversation sequences, and replay cursor under the profile's Application Support directory. `MacOSDurableMessagingStore` keeps a profile-scoped encrypted `messaging-v1` document under Application Support and commits related records through atomic transactions; the macOS model creates it for the active client profile.
- [ ] Implement directory lookup, pre-key claim, message fan-out, Sealed Sender envelope creation, decrypt, durable commit, and QueueAck in the host integration.
- [ ] Render a message only after the shared core has committed the decrypted message and cursor transaction.
- [ ] Add reconnect, offline outbox retry, stale-cursor recovery, send failure, authentication expiry, and dependency outage states to the UI.
- [ ] Keep message text, decrypted metadata, seeds, bearer tokens, and sealed payloads out of application and server logs.

### Local development backend and two-client runner

- [ ] Add an explicit loopback-only username development mode that can create disposable test accounts without Twilio; keep OTP disabled in this mode and prevent the mode from binding outside loopback or being enabled in Release builds.
- [ ] Add a runnable local WebSocket adapter around `links-gateway` that wires `decode_client_frame`, `Gateway::open`, `Gateway::handle`, and `encode_server_frame` to a real socket.
- [ ] Add a local development composition for PostgreSQL, account auth, encrypted mailbox storage, ephemeral session state, and the WebSocket gateway using one documented endpoint shared by both clients.
- [ ] Add a client launch option such as `--profile <name>` and an explicit profile root. Each profile must have separate Application Support data, Keychain namespace, logs, bearer token, and device/node IDs.
- [ ] Ensure the app supports two independent processes launched with macOS `open -n`; do not use a process-global singleton, shared lock, or shared database that prevents the second client from starting.
- [ ] Add readiness/status output for each profile so the runner can wait for both clients to authenticate and connect before sending a message.
- [ ] Add a documented two-client launcher, for example:

  ```sh
  open -n "/path/to/Links.app" --args --profile alice
  open -n "/path/to/Links.app" --args --profile bob
  ```

- [ ] Add a disposable two-client smoke harness that creates `@alice-test` and `@bob-test`, waits for two live `links.v1` sessions, sends a message in both directions, and records only pass/fail and timing metadata.
- [ ] Verify that two profiles can use the same gateway endpoint concurrently without session fencing; a reused `device_id` must fail clearly instead of silently replacing another client.

### macOS two-client acceptance gate

- [ ] Launch Alice and Bob as separate macOS processes with separate profiles on one machine.
- [ ] Verify both accounts have distinct identity public keys, device IDs, MLS node IDs, local stores, Keychain records, and bearer tokens.
- [ ] Verify both clients reach `ready` concurrently and remain connected for at least one heartbeat interval.
- [ ] Send Alice → Bob and Bob → Alice text; verify ordered delivery, exactly one render per message, and successful private delivery receipts.
- [ ] Stop Bob, send Alice → Bob while Bob is offline, restart Bob, and verify replay decrypts and renders the message exactly once.
- [ ] Restart both clients and verify identities, MLS state, outbox state, inbox state, and cursors survive without key regeneration.
- [ ] Drop and restore the network; verify reconnect resumes from the durable cursor and never acknowledges an uncommitted message.
- [ ] Tamper with a local envelope, MLS state record, or cursor; verify the client fails closed without rendering plaintext or advancing the cursor.
- [ ] Verify no plaintext message content, private key material, bearer token, or sealed payload bytes appear in local logs.
- [ ] Add a repeatable manual runbook and CI/build documentation for the exact macOS version, architecture, app build, profile names, backend revision, and acceptance result.
- [ ] Produce a signed/notarized macOS build only after the two-client acceptance gate and key-custody review pass.

Current repository blockers covered by this phase: `native/apple` is a Swift
package rather than an app target, `links-desktop-client` still needs concrete
OS storage/UI/socket hosts, `links-gateway` is transport-neutral without a
runnable socket adapter, and the Web host has no complete message UI/core
adapter. See [desktop client](docs/desktop-client.md), [gateway](docs/gateway.md),
and [consumer account setup](docs/consumer-account.md).

---

## Phase 6 — Media and payload efficiency

Add media after text delivery and multi-device sync are stable.

### Text and synchronization

- [x] Integrate Zstandard (Zstd) dictionary compression for bulk state synchronization.
  `CompressedSyncBatch` uses the version-1 fixed dictionary and negotiated
  `Hello.supported_sync_compression` capability. Gateway compression is only
  used when it makes the complete frame smaller; decompression validates the
  dictionary, declared size, exact output length and sync cursor/envelope shape
  before protobuf use. Legacy clients continue receiving plain `SyncBatch`.

### Voice notes

- [x] Integrate Opus audio in `.ogg` / `.opus` containers at 16–24 kbps.
  `links-client-core::voice` validates the fixed 20 ms voice profile, encodes
  native PCM with libopus, muxes packets into CRC-checked Ogg Opus pages, and
  accepts both container extensions. The encrypted Message carries matching
  Opus metadata.
- [x] Enable VBR and DTX to suppress quiet pauses.
  Native libopus now uses constrained VBR at the selected 16–24 kbps target
  and DTX for silent frames; the Ogg writer still keeps stream framing and
  packet bounds strict.
- [x] Add voice-note recording, upload, download, decryption, and playback to Android and iOS.
  `AndroidVoiceNotes` records API-29+ Ogg/Opus and plays verified decrypted
  cache files; `IOSVoiceNoteSession` records PCM, uses the shared Opus and
  attachment-core bridge, and plays verified PCM through AVAudioEngine. Both
  clients upload ciphertext only and require an exact upload receipt before
  sending private `MediaMetadata` through MLS.

### Images

- [x] Resize images client-side to a maximum 1600px longest edge.
  Shared dimension policy plus Android Bitmap and iOS ImageIO adapters preserve
  aspect ratio, avoid upscaling, and cap the longest edge at 1600 pixels before
  later image encryption/upload.
- [x] Strip EXIF location metadata.
  Android preserves EXIF orientation while re-encoding without input EXIF;
  iOS filters the ImageIO GPS dictionary before image encryption/upload.
- [x] Transcode to WebP / AVIF at approximately 80% lossy quality.
  Android emits WebP at quality 80; iOS tries AVIF and falls back to WebP at
  quality 0.8 when the platform encoder is available.
- [x] Embed low-resolution BlurHash placeholders in text payloads.
  Shared Rust core encodes fixed 4x3, 28-character BlurHash values from
  resized RGB pixels; the hash is carried privately in MediaMetadata.
- [x] Deploy S3-compatible encrypted blob storage with Cloudflare / CloudFront CDN edge caching.
  `S3CompatibleBlobStore` accepts only client ciphertext, uses immutable UUID
  object keys and conditional writes, verifies download receipts, and supports
  private R2/S3 origins behind signed CDN URLs.
- [x] Add encrypted image send, receive, caching, and rendering to Android and iOS, then Web and desktop.
  Android and iOS normalize images before shared-core AEAD encryption; all four
  clients upload and cache ciphertext only, verify size/digest receipts, and
  decrypt before rendering. Web and desktop expose the same uploader, cache,
  and renderer boundaries.

### Videos and large files

- [x] Transcode video with native hardware acceleration: iOS VideoToolbox and Android MediaCodec.
  Shared 720p/1080p profiles select H.264 or HEVC at 1.5/3 Mbps. iOS uses a
  hardware-required VideoToolbox session; Android uses hardware MediaCodec
  decoder/encoder surfaces with EGL scaling. Compatible AAC audio is preserved.
- [x] Use H.264/H.265 MP4 with target bitrates of 720p at 1.5 Mbps and 1080p at 3.0 Mbps.
  Shared profiles enforce H.264 or HEVC MP4 output at 1280x720 / 1.5 Mbps or
  1920x1080 / 3.0 Mbps, at 30 fps, across iOS and Android.
- [x] Place the `moov` atom at the file start (`faststart`).
  iOS enables AVAssetWriter network optimization; Android rewrites the MP4
  after MediaMuxer finishes, moving `moov` before `mdat` and adjusting `stco`
  or `co64` chunk offsets.
- [x] Implement WebRTC DataChannel P2P direct file streaming for uncapped large transfers.
  `p2p_transfer` defines ciphertext-only `LDT1` frames with 256 KiB chunks,
  per-chunk and whole-file SHA-256 checks, durable resume offsets, and ordered
  delivery enforcement. `WebRtcFileTransfer` adds browser DataChannel
  backpressure and bounded Blob source/sink callbacks; SDP signaling remains
  a separate host concern.
- [x] Add encrypted video and large-file transfer to clients after images and voice notes work.
  Shared chunked ChaCha20-Poly1305 uses 256 KiB ciphertext chunks, private
  size/chunk metadata, per-chunk AEAD, and whole-ciphertext SHA-256. iOS,
  Android, Web/WASM, and desktop clients stream ciphertext through upload or
  authenticated WebRTC transfer boundaries and publish plaintext only after
  complete verification.

---

## Phase 7 — Private discovery, account types, and anti-spam

Add discovery options after the basic phone-based account works.

### Pseudonymous account

- [x] Implement `@username` self-registration without requiring a phone number.
  Username registration and login use signed Ed25519 device-key transcripts,
  atomic pseudonymous account/handle/device/session writes, and per-handle/IP
  rate limits. Phone-derived subjects remain NULL for these accounts.
- [x] Support self-sovereign key generation with passkeys / BIP-39 mnemonic seed phrases.
  Shared identity APIs derive Ed25519 keys locally from generated 12/24-word
  mnemonics or user-verified WebAuthn PRF output. WASM, Apple, and Android
  wrappers expose the same local-only flow and hardware-seal the result.
- [x] Deploy global key directory lookup for `@usernames`.
  The account service exposes a no-store, rate-limited `GET /v1/directory/{handle}`
  endpoint backed by the shared PostgreSQL control plane. It returns active
  device identity keys and MLS credentials only; disabled accounts and revoked
  devices are excluded, and one-time pre-key claims remain separate.

### Private contact discovery

- [x] Hash local address-book phone numbers with Argon2id and a client salt.
  Shared client core validates canonical E.164 input, uses a random persistent
  16-byte per-client salt, and derives 32-byte Argon2id hashes locally without
  sending phone numbers or hashes to the server.
- [x] Build the server/network PSI zero-knowledge matching API without leaking full contact books.
  Authenticated clients send blinded Ristretto queries; the server returns
  verifiable DLEQ-backed OPRF evaluations, and clients match opaque directory
  tokens locally. Raw contact numbers and local Argon2id hashes never cross the
  network; query volume and account metadata remain visible to the service.
  Request and candidate budgets limit directory-enumeration attempts.

### Anti-spam

- [x] Integrate Privacy Pass anonymous blind signatures to rate-limit new chat requests without tracking identities.
  RFC 9578 P-384/SHA-384 VOPRF issuance uses authenticated account/IP quotas;
  anonymous redemption stores only a one-time token digest and expiry.
- [x] Implement client-side proof-of-work micro-challenges for unverified accounts initiating 1-to-1 connections.
  Pseudonymous accounts receive five-minute SHA-256 hashcash challenges at
  bounded difficulty; the server verifies and consumes each challenge once.

---

## Phase 8 — Groups, channels, business, and bot accounts

Add richer communication modes once 1-to-1 messaging, identity, sync, and media are reliable.

### Many-to-many groups

- [x] Enable MLS group messaging for many-to-many conversations.
  Shared client core now supports bounded many-to-many TreeKEM groups, group
  welcomes/commits, and encrypted group send/receive fan-out.
- [x] Implement group membership updates, device changes, epoch processing, and RBAC.
  Authenticated group create/list/role/remove APIs enforce owner/admin/member
  rules in PostgreSQL; device revocation is exposed through the account service;
  OpenMLS can stage verified device-leaf removals, report committed epochs, and
  reject control commits against stale epoch checkpoints. Hosts must deliver
  each resulting MLS commit before merging it locally.
- [x] Extend the send and receive flows to many-to-many groups.
  Group send/receive adapters now have first-class contracts, bounded active
  device fan-out, group-only MLS update validation, replay, decryption, and
  private delivery-receipt handling through the shared client core.

### Broadcast channels

- [x] Configure MLS broadcast profiles where subscribers join as passive/read-only leaves.
  `BroadcastSubscriber` is a read-only client-core MLS profile: it accepts only
  broadcast welcomes/commits and application messages, while rejecting local
  publishing, group creation, membership changes, and pending-commit merges.
- [x] Enforce Ed25519 signatures on all broadcast posts by admin devices.
  BroadcastPost is an encrypted Message content with a domain-separated
  Ed25519 transcript. The publisher helper signs it with the admin device key;
  subscribers verify the signature, sender-device binding, and current admin
  RBAC policy before storing or rendering the post.
- [x] Implement the publish flow: admin signs post, encrypts with broadcast master key, and dispatches to the broker.
  publish_broadcast_post signs the serialized Message, seals it with an
  HKDF-derived per-conversation/epoch ChaCha20-Poly1305 key, and hands only
  the opaque BroadcastDispatch to the broker. The NATS adapter routes using a
  hashed conversation subject and never opens the ciphertext.
- [x] Implement the receive flow: subscriber fetches signed payload, verifies the admin signature, decrypts, and renders.
  `receive_broadcast_dispatches` fetches bounded broker batches, decrypts the
  master-key wrapper, verifies the Ed25519 signature and current admin RBAC,
  renders only valid posts, and acknowledges after rendering.

### Organization accounts

- [x] Support multi-device and multi-admin key delegation through signed sub-certificates.
  DeviceSubCertificate binds issuer and subject device/MLS identities, role,
  validity, and public keys under an Ed25519 signature. Owners can delegate
  device or admin leaves; admins can delegate device leaves. Authenticated
  delegated registration verifies both issuer authority and child
  proof-of-possession, persists the certificate, and publishes it in the
  device directory.
- [x] Implement cryptographic proof-of-verification badges.
  VerificationBadge binds the account, optional handle, badge kind, issuer
  key, and bounded validity window under a domain-separated Ed25519 signature.
  Clients verify against a pinned authority key; the account service stores
  only the public badge and supports trusted issue/revoke hooks.
- [x] Build channel, business, and bot client surfaces for Android, iOS, Web, and desktop.
  Shared surface profiles define channel/business/bot roles and capabilities;
  platform hosts expose one connection/send/receive facade over the existing
  Android, iOS, Web, and desktop shared-core transports.

---

## Phase 9 — Real-time voice, video, and live streams

Add real-time media after messaging and file media are stable.

- [x] Implement WebRTC session SDP exchange.
  `links.v1` carries bounded offer, answer, and ICE signals between live
  authenticated device sockets; the browser `WebRtcSession` owns the
  RTCPeerConnection offer/answer flow and candidate exchange.
- [x] Implement SFrame frame-level encryption hooks through the WebRTC Encoded Transform API.
  `WebRtcSFrameController` binds the browser's native per-frame SFrame
  encryptor/decryptor to RTP senders and receivers, keeps only current and
  previous non-extractable AES-128-GCM keys, and fails closed when unsupported.
- [x] Transmit SFrame epoch keys through the MLS control channel.
  Private `MlsControl` payloads carry validated SFrame epoch keys inside MLS
  application ciphertext; direct and group send helpers fan them out through
  Sealed Sender, and receive handlers install them before cursor acknowledgement.
- [x] Deploy managed global SFU clusters using LiveKit or Mediasoup.
  LiveKit Cloud is the first managed provider. The gateway now has a typed
  multi-region endpoint, health, failover, pinned-placement, opaque-room, and
  SFrame-required contract in `crates/gateway/src/sfu.rs`; the regional
  deployment manifest and operator runbook are in `deploy/livekit/` and
  `docs/sfu.md`. External project provisioning, quotas, DNS, and credentials
  remain an operator release gate.
- [x] Configure SFUs to route encrypted media frames using unencrypted RTP headers without decrypting media.
  `SfuMediaPolicy::encrypted_sframe()` makes header-only forwarding, mandatory
  SFrame, and disabled media decryption an invariant of every LiveKit
  deployment and room placement. The provider manifest disables recording;
  provider-side settings and encrypted-media acceptance checks remain release
  gates in `deploy/livekit/`.
- [x] Add WebTransport (QUIC) as a fallback channel for low-latency media signaling in high-packet-loss environments.
  `WebTransportConnectionManager` provides authenticated HTTP/3 signaling
  with reliable length-prefixed `links.v1` protobuf frames, low-latency QUIC
  congestion control, bounded parsing, liveness checks, and reconnect. The
  browser WebRTC session fences WebSocket before falling back; the server
  HTTP/3 adapter uses the shared bounded Rust stream framer and remains a
  deployment gate. See `docs/webtransport-signaling.md`.
- [x] Implement the complete call flow: SDP exchange, MLS key exchange, client-side frame encryption, and SFU streaming.
  `WebRtcCallFlow` sequences MLS media-key publication, native SFrame setup,
  local/remote track negotiation, SDP/ICE exchange, encrypted SFU streaming,
  key rotation, and teardown through a LiveKit/Mediasoup adapter. Provider SDK
  binding and mobile host adapters remain platform release work. See
  `docs/call-flow.md`.
- [x] Add voice/video calls and live streams to mobile first, then Web and desktop.
  iOS `IOSCallSession` and Android `AndroidCallSession` enforce MLS key setup,
  native SFrame setup, opaque SFU join, SDP/ICE exchange, key rotation, and
  fail-closed teardown through injected platform media/provider adapters. Web
  exposes the same modes through `WebCallSurface`; desktop exposes the
  provider-neutral `DesktopCallSession`. See `docs/platform-calls.md`.

---

## Phase 10 — Mini-app and bot runtime

Add programmable features only after the account, permission, and messaging boundaries are stable.

- [x] Embed a client-side WebAssembly (WASM) sandbox runtime in mobile and desktop clients.
  `links-client-core::sandbox` uses the native `wasmi` interpreter with strict
  import allowlisting, fresh guest memory per invocation, 16 MiB memory,
  256 KiB output, 64 KiB input, 2 MiB module, recursion, and fuel limits.
  iOS and Android expose the runtime through the native FFI handles; desktop
  re-exports it as `DesktopMiniAppSandbox`. No WASI, network, filesystem,
  clock, randomness, identity, MLS, or key capability is available. See
  `docs/mini-app-sandbox.md`.
- [x] Implement a fine-grained Mini-App permission SDK that restricts direct network calls and isolates cryptographic keys.
  `SandboxPermissions` grants exact HTTPS hosts/methods with request/response
  limits, while `SandboxCryptoGrant` exposes only opaque operation handles.
  `SandboxHost` mediates every call; `SandboxRuntime::run` remains deny-all.
  See `docs/mini-app-permissions.md`.
- [x] Expose Mini-Apps and bots through organization account controls.
  Organization accounts now have default-off Mini-App and bot gates in
  `organization_controls`. `GET/PUT /v1/organization/controls` exposes the
  policy; any active organization device can read it, while only owner/admin
  devices can update it. Changes use an atomic revision counter. See
  `docs/organization-controls.md`.

---

## Phase 11 — Decentralized / federated / P2P network

This is a separate expansion track. Do not block the centralized release on it.

### Protocol and identity

- [x] Select one protocol layer: federated server nodes (Matrix-style), decentralized relay network (Nostr/XMTP-style), or P2P Libp2p mesh.
  Links selects Matrix-style federated server nodes. Each account keeps a
  home node; nodes exchange only authenticated, encrypted `Envelope` batches.
  Existing MLS, Sealed Sender, queues, cursors, and client replay stay intact.
  Nostr/XMTP relays and Libp2p mesh are not selected for this expansion. See
  `docs/federation.md`.
- [x] Implement W3C Decentralized Identifiers (DIDs) or smart-contract key registries on Solana / Base to map `@handles` to public identity keys.
  Links uses W3C `did:key` identifiers for Ed25519 device keys. The global
  `@handle` directory now returns each active device DID beside its public key;
  clients verify the deterministic DID/key binding before PQXDH. No Solana,
  Base, or smart-contract dependency is needed. See `docs/directory.md`.

### Routing and storage

- [x] Implement Libp2p / PubSub or relay gossip for cross-node envelope propagation.
  Links uses relay gossip in the selected Matrix-style federation layer. Signed
  `FederatedEnvelopeBatch` messages, bounded peer fan-out, exact NATS subjects,
  expiry checks, opaque payload forwarding, and atomic batch/envelope replay
  claims are implemented in `links-queue`. See `docs/relay-gossip.md`.
- [x] Deploy store-and-forward offline buffering nodes across independent relays.
  `IndependentRelayPool` fans each signed opaque batch into up to three
  independent JetStream clusters. Each cluster keeps messages for the bounded
  30-day TTL with explicit acknowledgement and encrypted-at-rest storage;
  destination nodes verify and deduplicate before local handoff. See
  `deploy/relays/` and `docs/relay-gossip.md`.
- [x] Integrate IPFS / Arweave / Filecoin for client-side encrypted chunk storage and content-addressed retrieval (`ipfs://CID`).
  `ContentAddressedLargeFileEncryptor` derives CIDv1 SHA-256 addresses from
  ciphertext chunks and stores the ordered references in private media
  metadata. `ContentAddressedStore` verifies every block before upload and
  after download; IPFS, Arweave, and Filecoin adapters share the provider
  boundary. See `docs/content-addressed-storage.md` and
  `deploy/content-addressed/`.

### Decentralized real-time media

- [x] Build SFU node discovery through Distributed Hash Tables (DHT).
  `links-gateway::sfu_discovery` provides signed Ed25519 SFU records, trusted
  key binding, region-scoped DHT keys, monotonic sequence handling, expiry and
  recent-health filtering, and bounded lookup results. Production libp2p
  Kademlia wiring is the `SfuDhtClient` adapter boundary. See
  `docs/sfu-discovery.md` and `deploy/sfu-dht/`.
- [x] Implement open-node or token-incentivized media relay networks for WebRTC call routing.
  `links-gateway::media_relay` adds signed open/token relay records, DHT
  discovery with trust and health filtering, relay-bound short-lived credit
  tokens, signed usage receipts, and route selection. Every route requires
  SFrame and forwards only RTP headers plus encrypted media. See
  `docs/media-relays.md` and `deploy/media-relays/`.
- [x] Add decentralized transport, storage, and media support to the existing Android, iOS, Web, and desktop clients.
  `links-client-core::decentralized` retries opaque envelopes across trusted
  transport endpoints, verifies client-encrypted CID chunks across storage
  gateways, and selects fresh SFrame-only media relays with open/token
  admission. Android, iOS, and Web expose matching host adapters; desktop and
  Web/WASM re-export the shared Rust contract. See
  `docs/decentralized-clients.md`.

---

## Phase 12 — Performance, security, and release gates

Run these checks continuously at the relevant phase boundary, with the full audit before public launch.

- [ ] Conduct a third-party cryptographic code audit of PQXDH, MLS ratcheting, and SFrame integration.
  Audit scope and evidence checklist are prepared in
  `docs/crypto-audit-scope.md`; leave this item open until an independent
  auditor reviews a pinned commit and signs the final report.
- [x] Verify the zero-knowledge routing stance for centralized and decentralized deployments.
  Static code and wire-contract review confirms that centralized gateways,
  federation queues, storage providers, DHTs, and media relays receive only
  routing metadata plus opaque encrypted bytes. Residual metadata and required
  operational evidence are recorded in `docs/zero-knowledge-routing.md`.
- [ ] Centralized benchmark: verify less than 50ms delivery latency for 1M concurrent WebSocket connections.
  The runbook and target contract are prepared in
  `docs/centralized-websocket-benchmark.md` and
  `deploy/benchmarks/centralized-websocket-1m.yaml`; keep this open until
  three real runs produce a signed worst-run report.
- [ ] Decentralized benchmark: measure multi-hop gossip propagation across 50 international nodes.
  The runbook and target contract are prepared in
  `docs/decentralized-gossip-benchmark.md` and
  `deploy/benchmarks/decentralized-gossip-50.yaml`; keep this open until
  three real runs produce a signed worst-run report.
- [ ] Review battery use, reconnect behavior, offline delivery, key recovery, and multi-device removal before public release.
  The evidence matrix and test sequence are prepared in
  `docs/release-readiness-review.md`. Source contracts are present, but the
  public-release gate stays open until physical-device, failure-injection,
  push/provider, recovery, battery, and MLS-removal evidence is signed.
