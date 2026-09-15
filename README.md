# Links

Links is an end-to-end encrypted messaging platform in active development. It
is built around a shared Rust protocol and client core, native hardware-backed
identity custody, and encrypted routing that does not need to read message
content.

> **Status:** Links is an implementation baseline, not a released messenger.
> The cryptographic designs and service boundaries are being built and checked,
> but a complete end-to-end client, production gateway, physical-device
> acceptance, and independent cryptographic audit are still open.

## What is here

- Versioned protobuf contracts for identity, pre-keys, messages, sync,
  transport, WebRTC, media, queues, and relays.
- Ed25519 identities and device certificates, BIP-39 local recovery, and
  hardware-backed seed wrapping for Apple Secure Enclave and Android Keystore.
- A Links PQXDH profile using X25519 and ML-KEM-768, plus OpenMLS RFC 9420
  TreeKEM for one-to-one and group sessions.
- Sealed Sender envelopes, per-device mailbox cursors, replay, acknowledgements,
  encrypted payload storage, and background recovery contracts.
- Account authentication with SMS/WhatsApp Verify, username-only accounts,
  passkey PRF backup, contact PSI, Privacy Pass, and chat-request proof of work.
- Client foundations for Android, iOS, Web/WASM, and desktop, with media,
  WebRTC, channels, business, bot, mini-app, and decentralized transport
  boundaries.

## What is not ready

The repository does not yet provide a runnable production chat app. In
particular:

- `native/apple` is the shared Apple Swift package; `native/macos` now contains
  the macOS SwiftUI application target that embeds its `LinksClient` and
  `LinksKeyStore` products.
- `web` contains a WASM host library, not a complete browser chat UI.
- `links-gateway` includes a runnable loopback WebSocket adapter and a
  PostgreSQL/account-auth composition for local two-client messaging.
- Durable host providers, UI integration, message TTL cleanup, physical-device
  acceptance, deployment, and the external crypto audit remain release work.

## Repository map

| Path | Purpose |
| --- | --- |
| [`proto/links/v1`](proto/links/v1) | Versioned wire contracts. |
| [`crates/protocol`](crates/protocol) | Generated protobuf types and boundary validation. |
| [`crates/identity`](crates/identity) | Self-sovereign identity, recovery, device certificates, and MLS credentials. |
| [`crates/client-core`](crates/client-core) | Shared crypto, PQXDH, MLS, messaging, sync, media, and sandbox logic. |
| [`crates/account-auth`](crates/account-auth) | Account, device, directory, passkey, and anti-spam HTTP services. |
| [`crates/server-store`](crates/server-store) | PostgreSQL migrations, encrypted mailbox, and storage adapters. |
| [`crates/gateway`](crates/gateway) and [`crates/queue`](crates/queue) | Gateway, push, routing, queue, relay, and delivery contracts. |
| [`native/android`](native/android) and [`native/apple`](native/apple) | Native identity custody and mobile client foundations. |
| [`crates/web-client`](crates/web-client), [`crates/desktop-client`](crates/desktop-client), and [`web`](web) | Web/WASM and desktop client foundations. |
| [`docs`](docs) and [`deploy`](deploy) | Design contracts, runbooks, and deployment manifests. |

## Quickstart

### Prerequisites

- Stable Rust with Cargo
- PostgreSQL 15+ for local development; Docker is optional
- Node.js and npm for the Web/WASM package
- Xcode and Swift 5.9+ for Apple targets
- JDK, Android SDK, and Gradle for Android builds
- Twilio Verify credentials when running phone OTP authentication

### Start the local development composition

The local composition uses host PostgreSQL or Docker PostgreSQL and one Debug
Rust process. That process starts account auth on `127.0.0.1:8080` and the
encrypted-mailbox gateway on `127.0.0.1:8081`. Both clients use the same
WebSocket endpoint: `ws://127.0.0.1:8081/v1/connect`.

```sh
cp .env.example .env
# Edit .env. Set AUTH_LOOKUP_KEY to a random 32-byte base64url secret.
bash scripts/local-dev.sh
```

To build and launch two separate macOS clients with `alice` and `bob`
profiles, run:

```sh
bash launch-links.sh
```

The launcher builds the signed Debug app, starts the local backend without
Docker, and uses two `open -n` processes. Set `LINKS_BUILD_APP=0` with
`LINKS_APP_PATH=/path/to/Links.app` to use an existing build. If Xcode has no
team selected, pass your Apple Development Team ID:

```sh
# Replace YOUR_TEAM_ID with your real Apple Team ID.
LINKS_DEVELOPMENT_TEAM=YOUR_TEAM_ID bash launch-links.sh
```

The launcher allows Xcode to create or download the matching development
profile. Set `LINKS_ALLOW_PROVISIONING_UPDATES=0` to disable that behavior.

Set `LINKS_GATEWAY_ENDPOINT` to that value in both the macOS and Web client
hosts. The auth URL is `http://127.0.0.1:8080`. The process applies the
PostgreSQL migrations before serving and uses the Debug-only `_test` username
flow; no Twilio credentials are needed. Stop the Rust process with Ctrl-C;
PostgreSQL remains in its Docker volume for the next run.

For the real phone OTP flow, run the migration example and account-auth binary
separately with the same `DATABASE_URL` and `AUTH_LOOKUP_KEY`, then supply all
`TWILIO_*` values. Never use the example credentials outside local development.

### Disposable username development mode

For a throwaway local database, run the account service in a Debug build with
the explicit loopback-only mode:

```sh
AUTH_DEV_USERNAME_MODE=1 AUTH_BIND=127.0.0.1:8080 \
  cargo run -p links-account-auth --locked
```

Register signed usernames ending in `_test`, such as `alice_test` and
`bob_test`. This mode does not read Twilio credentials, removes the phone OTP
routes, and rejects non-test handles. `AUTH_LOOKUP_KEY` is still required.
`AUTH_DEV_USERNAME_MODE=1` is rejected by `cargo run --release`, and every
account-auth mode rejects a non-loopback `AUTH_BIND`.

### Build the libraries

```sh
cargo build --workspace --locked

# The Apple Swift package links this Rust library.
cargo build -p links-identity-ffi --locked
swift build --package-path native/apple

# Web/WASM client
rustup target add wasm32-unknown-unknown
cd web
npm run build:wasm
```

Build the Android modules with an installed Android SDK and Gradle:

```sh
gradle -p native/android assembleDebug
```

These commands build libraries and native modules. They do not launch a
complete chat client.

## Security model

Links keeps plaintext and long-term private key material on the client. The
server is intended to handle account control, opaque ciphertext, routing,
mailbox cursors, and delivery state.

- Identity keys use Ed25519. Device enrollment is bound by signed,
  nonce-specific transcripts and device sub-certificates.
- Session setup uses X25519 plus ML-KEM-768 in the Links PQXDH profile. OpenMLS
  provides the TreeKEM ratchet and encrypted application messages.
- Each recipient device receives its own sealed envelope. Routing services see
  only the metadata required to deliver it; conversation and message content
  stay inside the encrypted payload.
- Passkey backup encrypts the identity seed locally with a WebAuthn PRF output.
  The server stores only the opaque backup envelope and ceremony data.
- Defaults fail closed where a platform-backed key or provider is required.
  Implemented interfaces and passing compile checks are not a production
  security claim; interoperability testing and independent review are still
  required.

Read the detailed boundaries in [`docs/pqxdh.md`](docs/pqxdh.md),
[`docs/mls.md`](docs/mls.md),
[`docs/consumer-account.md`](docs/consumer-account.md), and
[`docs/hardware-identity.md`](docs/hardware-identity.md).

## Validation

The CI workflow runs formatting, warning-free Clippy, Rust unit and doc tests,
PostgreSQL integration tests, WASM checks, and Apple/Android native builds.
The PostgreSQL integration tests use `LINKS_TEST_DATABASE_URL` and a disposable
database. Hardware-backed round trips and the two-client macOS flow are separate
release gates.

Useful local checks:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --all-targets --locked
cargo test --workspace --doc --locked
cargo check -p links-client-core --target wasm32-unknown-unknown --locked
cargo check -p links-web-client --target wasm32-unknown-unknown --locked
```

For PostgreSQL integration tests, point `LINKS_TEST_DATABASE_URL` at a
disposable database and run:

```sh
cargo test --workspace --all-targets --locked -- --ignored
```

## Roadmap

The intended delivery order is:

1. Shared protocol and cryptographic core
2. Reliable centralized one-to-one text
3. Android and iOS consumer clients
4. Web and desktop companions
5. Groups, media, calls, channels, business, and bot surfaces
6. Mini-app runtime and decentralized/federated transport
7. Performance, security review, hardware acceptance, and release gates

See [`TODO.md`](TODO.md) for the checked implementation plan. Start with
[`docs/phase-0.md`](docs/phase-0.md) for product scope and threat model,
[`docs/contracts.md`](docs/contracts.md) for protocol and transport semantics,
and [`docs/release-readiness-review.md`](docs/release-readiness-review.md) for
the public-release checklist.
