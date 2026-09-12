# Links

Centralized-first encrypted messaging. The foundation now includes phone OTP
account authentication and Ed25519 device enrollment. **This is not yet a working
messenger or a claim of post-quantum security.** MLS/envelope encryption, delivery
gateways, and client applications remain in later phases. Native hardware vaults
are implemented but physical-device acceptance remains open.

## Workspace

| Path | Responsibility |
| --- | --- |
| `proto/links/v1` | Versioned protobuf message, user, media, receipt, envelope, sync and transport contracts. |
| `crates/protocol` | Generated common types, descriptors and boundary validation. |
| `crates/client-core` | Portable identity/crypto/MLS interfaces, envelope orchestration and durable sync validation. |
| `crates/server-store` | PostgreSQL repository/migrations, encrypted payload-store contract, Redis-shaped state contract and memory reference adapter. |
| `crates/identity` | Random Ed25519 keys, signed phone/enrollment transcripts and MLS basic credentials. |
| `crates/account-auth` | SMS/WhatsApp Verify adapter, durable account enrollment/login, HTTP API and sessions. |
| `native/apple`, `native/android` | Hardware-backed seed-wrapping adapters and acceptance tests. |
| `docs/consumer-account.md` | Account setup, API, security boundaries and remaining hardware gates. |
| `docs/phase-0.md` | Product scope, threat model, metadata budget, platforms and release gates. |
| `docs/contracts.md` | WebSocket choice, wire compatibility, transport and sync semantics. |
| `docs/storage.md` | PostgreSQL, payload-store and ephemeral-state guarantees and limits. |

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
future work. Message-encryption providers still fail closed rather than sending
plaintext. See [account setup](docs/consumer-account.md) and [the roadmap](TODO.md).
