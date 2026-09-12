# Links

Centralized-first encrypted messaging. **Phase 0 is a tested foundation, not a
working messenger or a claim of post-quantum security.** Production cryptography,
authentication, gateways, and client applications remain in later phases.

## Workspace

| Path | Responsibility |
| --- | --- |
| `proto/links/v1` | Versioned protobuf message, user, media, receipt, envelope, sync and transport contracts. |
| `crates/protocol` | Generated common types, descriptors and boundary validation. |
| `crates/client-core` | Portable identity/crypto/MLS interfaces, envelope orchestration and durable sync validation. |
| `crates/server-store` | PostgreSQL repository/migrations, encrypted payload-store contract, Redis-shaped state contract and memory reference adapter. |
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
cargo test -p links-server-store --test postgres --locked -- --ignored
```

The migration command targets `DATABASE_URL`. Integration tests use
`LINKS_TEST_DATABASE_URL`, create a random isolated schema for each test, and drop
only those schemas afterward. The test role needs CREATE SCHEMA permission. Never
point these development commands at production. `docker compose down` stops the
local service while retaining its named data volume.

CI runs formatting, warning-free lint, unit/doc tests, PostgreSQL integration tests
and a WASM compile check. Mobile FFI bindings, a Redis network adapter, and
ScyllaDB/DynamoDB adapters are not part of Phase 0. Crypto providers are explicit
interfaces; the included unavailable provider always fails rather than sending
plaintext. See [the roadmap](TODO.md) for subsequent implementation phases.
