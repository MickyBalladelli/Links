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
| `crates/protocol` | Generated common types, descriptors and boundary validation. |
| `crates/client-core` | Portable identity/PQXDH/MLS interfaces, encrypted conversation sequencing, PRF-encrypted passkey backup, per-device envelope fanout, background replay/decrypt and durable sync validation. |
| `crates/server-store` | PostgreSQL repository/migrations, append-only encrypted payload store, Redis Lua state adapter and memory reference adapter. |
| `crates/identity` | Random Ed25519 keys, signed phone/enrollment transcripts and MLS basic credentials. |
| `crates/account-auth` | SMS/WhatsApp Verify adapter, durable account enrollment/login, authenticated additional-device registration, WebAuthn passkeys, opaque key-backup HTTP API and sessions. |
| `crates/gateway` | Multi-region WebSocket session fencing, durable encrypted routing and configured APNs/FCM silent wakeup contracts. |
| `crates/queue` | Opaque NATS JetStream delivery wire contract and durable publish adapter for cross-region gateway routing. |
| `native/apple`, `native/android` | Hardware-backed seed-wrapping adapters, acceptance tests, iOS/Android client foundations and identity/OTP onboarding, plus the internal Android text-messaging shell. |
| `docs/consumer-account.md` | Account setup, API, security boundaries and remaining hardware gates. |
| `docs/phase-0.md` | Product scope, threat model, metadata budget, platforms and release gates. |
| `docs/contracts.md` | WebSocket choice, wire compatibility, transport and sync semantics. |
| `docs/storage.md` | PostgreSQL, payload-store and ephemeral-state guarantees and limits. |
| `docs/pqxdh.md` | Links PQXDH profile, key schedule, custody requirements and security limits. |
| `docs/prekeys.md` | Pre-key generation, automatic refill, authenticated upload and atomic claim contracts. |
| `docs/mls.md` | OpenMLS RFC 9420 TreeKEM core, hybrid suite, credential checks and durable commit flow. |
| `docs/passkey-backup.md` | WebAuthn ceremonies, PRF-encrypted identity backup and server storage boundary. |
| `docs/gateway.md` | Multi-region WebSocket gateway flow, routing, push fallback and deployment gates. |
| `docs/message-queue.md` | NATS JetStream subjects, opaque delivery rules and regional deployment contract. |
| `docs/android-text-milestone.md` | Internal Android one-to-one text build scope and two-device acceptance gate. |
| `docs/ios-client.md` | iOS client foundation, shared-core boundary and release limits. |

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
