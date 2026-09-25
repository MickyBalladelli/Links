# Standard consumer account

OTP authentication and Ed25519/MLS credential enrollment are implemented and tested.
Native seed vaults are wired to Rust through Swift/C and Android JNI wrappers;
physical hardware acceptance is still open. See [hardware identity integration
and acceptance](hardware-identity.md). This is not a finished consumer messaging
client or an audited E2EE system.

## What is implemented

`links-account-auth` is a loopback HTTP service backed by PostgreSQL. Its provider
adapter uses [Twilio Verify](https://www.twilio.com/docs/verify/api) for SMS or
[WhatsApp](https://www.twilio.com/docs/verify/whatsapp). Code generation and delivery
are delegated to Verify, not a custom OTP generator. Production code has no fixed
OTP, console-code output, in-memory auth database, or bypass provider. Mock
providers are confined to tests. Live sends incur provider charges and were not
performed during implementation.

Username-only accounts use the same Ed25519 device identity but do not require a
phone or store a phone-derived subject. The canonical handle is lowercase ASCII
`[a-z][a-z0-9_]{2,31}`; the `@` is display-only. Registration and returning login
start with a durable, short-lived, one-time server challenge. The versioned
transcript binds the operation, challenge ID and value, expiry, handle, device,
MLS node, and public key. Challenge consumption commits atomically with account or
session creation. Unauthenticated traffic is limited by source and global buckets;
account/device limits apply only after a valid returning-device proof.

For local two-client work, the Debug-only `AUTH_DEV_USERNAME_MODE=1` service
accepts signed canonical lowercase handles (for example `alice`) and uses no
Twilio provider. Its phone OTP routes are absent and the service still
requires `AUTH_BIND` to be loopback. Release builds reject this mode. Use a
separate disposable PostgreSQL database or volume for these accounts.

`links-identity` generates 32-byte random Ed25519 seeds, derives public keys, signs
phone, username, and device-enrollment transcripts, verifies signatures strictly,
and produces an RFC 9420 basic credential using TLS codec serialization. The core
exposes `HardwareIdentityStore`
and `HardwareSeedVault`; Swift and Java supply native wrap/load/delete operations.
`links-identity-ffi` connects those operations to the native `HardwareIdentityStore`
APIs. The Android client now provides the first-run hardware identity and phone OTP
flow; iOS onboarding and the rest of the consumer messaging UX remain separate
client tasks.

`RecoveryMnemonic` accepts only English BIP-39 12- or 24-word phrases. It uses
BIP-39 PBKDF2-HMAC-SHA512 with an optional passphrase, keeps the 512-bit root in
zeroizing memory, and derives the Links Ed25519 identity seed through a separate
HKDF domain. The phrase and derived keys stay on the device; this does not send
recovery material to the server. Account recovery enrollment, device metadata,
and regenerated PQXDH pre-key inventory still need their own authenticated flow.
`HardwareIdentityStore::restore_from_recovery` can seal the derived identity
directly into the native vault; it must not run during ordinary login or retry.

Passkey backup uses the WebAuthn PRF (`hmac-secret`) extension, not a passkey
private-key export. `HardwareIdentityStore::backup_with_passkey` loads the
hardware-wrapped Ed25519 seed briefly, encrypts it in `links-client-core` with a
fresh-salt PRF-derived ChaCha20-Poly1305 key, and uploads only the authenticated
opaque envelope. `crates/account-auth` provides explicit registration/assertion
ceremonies for ES256 `fmt=none` passkeys and monotonic counter checks. The server
stores credential public keys, challenges and ciphertext only; it never sees
the PRF result or identity seed. Configure `PasskeyConfig` and use
`AccountAuth::new_with_passkey` before enabling these routes.

## Security correction to the roadmap

Phone numbers and OTPs are not key seeds. The seed comes from the OS CSPRNG; phone
possession is verified separately. A keyed HMAC maps the phone to an opaque
account UUID in the server database. The device proves possession of its Ed25519
key before requesting a code, and again over a fresh challenge before enrollment.
The resulting account/device/node/public-key binding is stored atomically only
after verification. Public credentials contain no phone number or phone digest.

An [MLS basic credential](https://www.rfc-editor.org/info/rfc9420) is an application
identity assertion, not a certificate or proof of phone verification by itself.
The shared client core now verifies signed MLS leaves and key packages through an
authenticated directory hook. The Ed25519 implementation uses ed25519-dalek; its
behavior is checked against [RFC 8032, section 7.1](https://www.rfc-editor.org/rfc/rfc8032.html#section-7.1).
No custom signature or encryption algorithm is introduced.

## API contract

Auth routes are under `/v1/auth`. JSON bodies are limited to 4 KiB. Binary fields
are base64url without padding. UUIDs are UUID strings. Responses, including errors,
have `Cache-Control: no-store`. Do not enable request/response-body logging at the
application, proxy, provider SDK, or analytics layer.

| Endpoint | Input | Result |
| --- | --- | --- |
| `POST /start` | `phone`, `channel`, `device_id`, `mls_node_id`, `public_key`, `signature` | Provisional account/device binding, `challenge_id`, nonce, expiry and MLS credential. |
| `POST /finish` | `challenge_id`, `code`, `signature` | A device-scoped bearer access token, expiry, user ID and device ID. |
| `POST /v1/auth/username/challenge` | `purpose` (`registration` or `login`), `handle`, `device_id`, `mls_node_id`, `public_key` | Returns the server challenge ID/value, exact bound identity fields, and expiry. |
| `POST /v1/auth/username/register` | `challenge_id`, `signature` | Atomically consumes a registration challenge, creates a pseudonymous account, and returns its session, handle, and MLS credential. |
| `POST /v1/auth/username/login` | `challenge_id`, `signature` | Atomically consumes a login challenge and returns one session after the registered device key proves possession. |
| `GET /v1/account/username` | Current bearer | Returns the account's public username, or `null` if none is set. |
| `PUT /v1/account/username` | Current bearer and canonical `handle` | Changes the account's public username and returns it. The previous username is released. |
| `GET /v1/account/display-name` | Current bearer | Returns the shared display name and whether the account has set one. |
| `PUT /v1/account/display-name` | Current bearer and `display_name` (up to 80 UTF-8 bytes) | Sets the public display name. An empty string clears it. |
| `POST /v1/auth/logout` | Current bearer | Deletes only the current session and returns no body. Repetition is safe. |
| `DELETE /v1/auth/sessions/others` | Current bearer | Revokes every other session for the account while preserving the current session. |
| `GET /v1/directory/{handle}` | Canonical handle, optionally prefixed with display-only `@` | Current username, shared display name, active user ID, and every active device's W3C `did:key`, public identity key, MLS node ID and MLS credential. |
| `GET /v1/directory/users/{user_id}` | Bearer session and sender user ID | Resolves the current username and display name for saved contacts and conversations without changing the stable user ID. |
| `GET /v1/contact-discovery/parameters` | Bearer session | OPRF public key and opaque active phone-directory membership filter. |
| `POST /v1/contact-discovery/query` | Bearer session plus bounded blinded Ristretto points | One verifiable OPRF evaluation per blinded input. |
| `GET /v1/privacy-pass/parameters` | None | Privacy Pass VOPRF public key and key identifier. |
| `GET /v1/privacy-pass/challenge` | None | Short-lived challenge for a new-chat admission token. |
| `POST /v1/privacy-pass/issue` | Bearer session plus one blinded P-384 point | Blind VOPRF evaluation; the issuer does not see the challenge or nonce. |
| `POST /v1/privacy-pass/redeem` | Challenge, token | One-time anonymous admission check; no account identifier is accepted or stored. |
| `GET /v1/chat-requests/proof-of-work/challenge` | Bearer session for a pseudonymous account | Five-minute, bounded SHA-256 hashcash challenge. |
| `POST /v1/chat-requests/proof-of-work/verify` | Bearer session plus challenge and nonce | Verifies and consumes one client proof before new-chat admission. |
| `GET /me` | `Authorization: Bearer <access_token>` | Authenticated user/device after current revocation and account-status checks. |
| `POST /v1/devices` | Bearer session plus target `device_id`, `mls_node_id`, public key, pairing nonce and target signature | Registers an additional physical client as a distinct device/node and returns its MLS credential. |
| `DELETE /v1/devices/{device_id}` | Bearer session; device ID in the path | Revokes an owned physical device and removes it from active directory/session queries. Remaining MLS members must remove its leaf. |
| `POST /v1/groups` | Bearer session plus `group_id` and optional `kind` | Creates an account-level group and assigns the authenticated account as owner. |
| `GET /v1/groups/{group_id}/members` | Bearer session | Returns the current member user IDs and RBAC roles. |
| `PUT /v1/groups/{group_id}/members/{user_id}/role` | Bearer session plus `role` | Applies owner/admin RBAC to add or change a member role. |
| `DELETE /v1/groups/{group_id}/members/{user_id}` | Bearer session | Removes a permitted member, or lets the authenticated member leave. |

Username challenge starts use separate registration and login buckets: 20 per
source IP per minute and 200 per source IP per hour for each purpose, plus 10,000
starts per purpose per hour across the service. Finish requests use separate
per-purpose source buckets of 40 per minute and 400 per hour, plus a 10,000-per-hour
global ceiling. After a valid login signature, one account/device pair may
complete 120 logins per hour. OTP starts allow one per phone per minute, five per
phone per hour, 20 per source IP per hour, and 10,000 globally per hour. These
service ceilings are conservative defense in depth. Operations may impose tighter
limits at ingress immediately; raising a service ceiling requires a reviewed code
and deployment change so an emergency override cannot silently disable protection.

The username directory is globally backed by the authoritative PostgreSQL
control plane. It returns only active accounts and non-revoked device public
material; disabled accounts and revoked devices disappear from the result. All
responses use `Cache-Control: no-store`, and the endpoint applies independent
per-handle and per-source rate limits. A missing handle returns `404` without
revealing account-authentication state. The lookup does not consume one-time
pre-keys: an authenticated sender claims each returned device's bundle through
`POST /v1/prekeys/{device_id}/claim`, then verifies the signed bundle before
starting PQXDH.

Contact discovery uses an authenticated one-sided PSI flow. The client creates
fresh blindings for local canonical E.164 contacts, posts only the blinded
Ristretto points, verifies each server DLEQ proof against the configured OPRF
public key, unblinds locally, and checks the resulting tokens against the opaque
membership filter. The server sees the authenticated account, source address,
query count, and timing, but not the phone values. The client sees opaque OPRF
tokens, not the server's phone numbers. Queries are limited to 256 contacts per
request and rate-limited per account and source address.

New-chat admission uses a separate Privacy Pass flow. The client obtains a
short-lived challenge, blinds a fresh token locally, and gets its blind
signature through the authenticated issuance route. It then redeems the
unblinded token without bearer authentication. Issuance is quota-limited per
account and source address; redemption is limited per source address and uses
only a one-time token digest plus expiry for replay prevention. No account,
device, conversation, or chat-request identifier is stored with redemption.
The gateway or new-chat host must require a successful redemption before
accepting a new-chat request. Keep issuance and redemption logs and operational
contexts separate, because correlating them would weaken the anonymity goal.

Unverified username-only accounts are `pseudonymous` accounts. Before a
one-to-one connection start, they request a proof-of-work challenge and solve
it locally on a background worker. The challenge binds to the account and
device, expires after five minutes, and uses 18 leading zero bits by default
with a server-enforced range of 12–24 bits. The server stores only a keyed
challenge digest, account/device binding, difficulty, expiry, and consumed
state in the challenge table; a separate keyed source-address bucket protects
the rate limit. Phone-verified accounts do not need this proof.

The phone must already be canonical E.164: `+` followed by 8–15 ASCII digits, with
a nonzero country-code prefix. This is format validation, not proof that a number
is assigned; the provider handles delivery. Channels are `sms` and `whatsapp`.
Configure the Verify service for 6–10 digit codes. Phone normalization belongs in
the client and must not invent a default country silently.

For `/start`, the client signs `links_identity::phone_auth_transcript` with its
random identity key. This binds the phone, channel, device ID, MLS node ID and
public key. For `/finish`, it validates that the returned device/node/public key
match the local request, checks the credential against `DeviceBinding::mls_credential`,
and signs `DeviceBinding::enrollment_transcript` over the account binding, challenge
ID, random nonce and expiry. An existing device must also check the account UUID
against its saved identity. Persist the vault handle and binding before proceeding;
do not regenerate keys on an ordinary login or network retry.

A new account can enroll its initial device after OTP plus key-possession proof.
An existing account can log in with its already registered, unrevoked device and
the same public key/node/credential. An additional physical client uses the
authenticated `POST /v1/devices` flow: the approving device supplies its bearer
session, and the new device signs a fresh nonce-bound transcript with its own
identity key. **OTP alone cannot replace existing identity keys**.
`links-client-core::pairing::PairingPayload` now carries these public fields and
the signature in a strict `links://connect?...` URI. The approving device must
parse and verify it, confirm the displayed account identity, then submit the
decoded fields to `POST /v1/devices`.

For username-only registration and login, the client first submits the canonical
handle and public device binding to `/v1/auth/username/challenge`. It validates the
returned binding and expiry, signs the corresponding v2 transcript, then submits
only the challenge ID and signature to the finish route. A username is not a
password and does not authenticate a copied device ID; the registered Ed25519 key
is required.
For self-sovereign creation, clients generate an English 12- or 24-word BIP-39
phrase locally or derive the identity from a user-verified passkey PRF. The
derived key is sealed in the platform vault before username registration. The
phrase and PRF output never enter the server API. Passkey or seed-phrase recovery
remains the path for a lost device.
Ineligible requests receive provisional challenges without disclosing the real
account UUID. They never receive a session even with a correct OTP. This is not
a formal guarantee of enumeration resistance or of constant-time behavior.

Passkey routes are disabled unless both `PASSKEY_RP_ID` and `PASSKEY_ORIGIN` are
configured. They use the existing bearer session for account/device context:

| Endpoint | Input | Result |
| --- | --- | --- |
| `POST /v1/passkeys/register/start` | Bearer session | Short-lived challenge, RP ID and account user ID. |
| `POST /v1/passkeys/register/finish` | Challenge ID, credential ID, client data JSON and attestation object, base64url encoded | Registered ES256 public credential after `fmt=none`, UV and RP checks. |
| `POST /v1/passkeys/assert/start` | Bearer session | Short-lived assertion challenge. |
| `POST /v1/passkeys/assert/finish` | Challenge ID, credential ID, client data JSON, authenticator data and signature, base64url encoded | Verified passkey assertion and updated authenticator counter. |
| `PUT /v1/passkey-backups` | Backup ID, current device ID, credential ID and encrypted envelope, base64url encoded | Stores opaque ciphertext; the PRF output is never sent. |
| `GET /v1/passkey-backups/{backup_id}` | Bearer session | Returns the opaque envelope for local PRF decryption. |

The verifier accepts ES256 P-256 credentials (`alg=-7`) and `fmt=none`
attestation only. The client must request the WebAuthn PRF/`hmac-secret`
extension separately; its 32-byte result is passed only to
`HardwareIdentityStore::backup_with_passkey` or
`HardwareIdentityStore::restore_from_passkey`. Passkey assertion is not yet a
replacement for the separate new-device enrollment flow.

The Android client now wires these primitives through `AccountRecovery` and
`PasskeyClient`. BIP-39 restore derives inside the Rust JNI call and seals the
result directly into `HardwareSeedVault`. Passkey backup/restore uses a host
`PasskeyProvider` that must request user verification and the WebAuthn PRF
extension; the server receives only the opaque authenticated envelope. Restored
Android clients receive a fresh physical device/node ID and must complete the
existing authenticated device enrollment and PQXDH pre-key upload. The current
bearer-only passkey routes require an authenticated bootstrap session for cloud
restore; no server-side mnemonic, PRF output, seed, or unencrypted key state is
introduced.

## Durable authentication rules

Challenges last 10 minutes, permit five provider checks, and allow one check in
flight. Starting a replacement invalidates older pending/checking challenges for
the phone. Requests are limited to one per phone per 60-second window, five per
phone per hour, and twenty per socket-peer IP per hour. The database arbitrates
these limits before delivery, so multiple service instances cannot bypass them.
Provider errors still consume the request quota; no fail-open retry path exists.

The states are `reserved`, `pending`, `checking`, `consumed`, and `failed`.
Transactions do not remain open during provider HTTP. A timeout, process crash,
or ambiguous provider approval during `checking` requires a new challenge rather
than guessing that authentication succeeded. The database prevents one provider
verification SID from consuming multiple local challenges. An already-consumed
challenge cannot issue another session, even after a restart. If a successful
response is lost, the client requests a new OTP after the cooldown; finish is not
a token-recovery endpoint.

Tokens contain 32 random bytes, expire after 15 minutes, and are stored only as
SHA-256 digests. Every authenticated request checks expiry, account disablement
and device revocation. There are no refresh tokens, phone-number change, account
erasure, SIM-swap recovery, or key-replacement endpoints in this slice. A username
change keeps the same account and devices but releases the previous username.
Account
creation, device registration, credential storage, token insertion and challenge
consumption commit together. Existing devices are never overwritten.

Phone lookup uses a domain-separated HMAC-SHA-256 with a required, stable server
secret. Raw phones, codes, seeds and bearer tokens are not database fields. The
provider necessarily receives the phone and code; its retention policy is separate.
Do not claim this is anonymous or zero-knowledge account authentication. The
minute cleanup job deletes expired sessions, OTP and username challenges ten
minutes after expiry (including replay markers), and rate-limit rows after the
longest window.
Deployments must separately apply retention controls to backups and infrastructure
logs. The secret lookup key must be backed up securely; replacing it without a
planned migration breaks phone-to-account lookup.

## Run the service

Apply the repository migrations using the explicit migration example, then supply
these variables through a secret manager or a private local environment:

| Variable | Meaning |
| --- | --- |
| `DATABASE_URL` | Intended PostgreSQL database; use verified TLS outside local development. |
| `AUTH_LOOKUP_KEY` | Stable random 32-byte secret, base64url without padding. No default. |
| `TWILIO_ACCOUNT_SID` | Account identifier for the configured provider account. |
| `TWILIO_AUTH_TOKEN` | Provider credential; never commit or log it. |
| `TWILIO_VERIFY_SERVICE_SID` | Verify service with SMS and, when needed, WhatsApp sender enabled. |
| `AUTH_BIND` | Defaults to `127.0.0.1:8080`; non-loopback HTTP binding is rejected. |
| `AUTH_DEV_USERNAME_MODE` | Debug-only `1` enables loopback username-only accounts with canonical lowercase handles; disables OTP and does not read Twilio credentials. |
| `AUTH_TRUSTED_PROXY_IPS` | Comma-separated exact socket IPs allowed to supply one replacement `X-Forwarded-For` client address. Required in Release builds. |
| `PASSKEY_RP_ID` | WebAuthn relying-party ID; must be paired with `PASSKEY_ORIGIN`. |
| `PASSKEY_ORIGIN` | Exact web origin used by WebAuthn client data; must be paired with `PASSKEY_RP_ID`. |

```sh
cargo run -p links-server-store --example migrate --locked
cargo run -p links-account-auth --locked
```

For disposable local username accounts, omit the `TWILIO_*` variables and use
the explicit Debug-only mode:

```sh
AUTH_DEV_USERNAME_MODE=1 AUTH_BIND=127.0.0.1:8080 \
  cargo run -p links-account-auth --locked
```

The username registration and login routes remain available for signed
canonical lowercase handles. Phone `/v1/auth/start` and `/v1/auth/finish` are
not mounted.
Do not expose this service beyond loopback or use the mode in a Release build.

The binary is not a TLS server: terminate TLS in a trusted local proxy before
external access. Release builds require `AUTH_TRUSTED_PROXY_IPS`. Only an exact
socket-peer match in that allowlist may supply `X-Forwarded-For`, and the value
must contain exactly one valid IP address; missing, malformed, repeated, or chained
values fail closed. Untrusted peers cannot override their socket address. The
proxy must replace inbound forwarding headers rather than append to them.
Production ingress remains responsible for source-IP, connection, bot,
geographical, provider-spend, and global controls. Service-side source and global
ceilings remain defense in depth; repeated `429` responses emit an operational
signal without logging identifiers or credentials.

## Native key custody

[Apple Secure Enclave](https://developer.apple.com/documentation/security/protecting-keys-with-the-secure-enclave)
does not provide direct Ed25519 signing. The Apple vault creates a non-exportable
Secure Enclave P-256 wrapping key and uses Apple's ECIES authenticated encryption
API. It stores only the wrapped seed in a device-only, non-synchronizing Keychain
item. Access is allowed while unlocked; missing keys, unavailable hardware or
invalid ciphertext fail rather than silently create a replacement identity.

The [Android vault](https://developer.android.com/privacy-and-security/keystore)
uses a non-exportable AndroidKeyStore AES-256-GCM wrapping key. It checks KeyInfo
for TEE/StrongBox enforcement and rejects software-only keys. It requires an
unlocked device, uses provider-generated nonces and authenticates the handle as
associated data. Encrypted records use AtomicFile in the app's no-backup directory.
No raw seed is written to files/preferences or cloud backup. The Java vault
requires API 28+, a configured secure screen lock and an unlocked user profile;
its build targets SDK 35. Older Android unlocked-device-required availability
issues are documented in the [hardware integration guide](hardware-identity.md).

**Only the wrapping key stays inside secure hardware.** The Ed25519 seed briefly
returns to app memory for signing. Rust buffers zeroize on drop and Dalek zeroizes
its owned signing keys; callers must minimize and wipe copies across FFI. Swift,
Java, OS buffers and memory snapshots prevent any blanket guarantee that every
copy is erased. Loss/invalidation of the wrapping key requires authenticated
re-pairing/recovery, not regeneration under the old identity. Serialize vault
operations through the client's identity worker. Crash-created orphan wrapping
keys require eventual cleanup; they contain no recoverable plaintext seed alone.

The Apple library is a Swift package for iOS 16+/macOS 13+. Use a signed app/test
host with the correct Keychain entitlements; no developer team or provisioning
identity is hardcoded. The Android identity library uses AGP 8.9.2, Gradle 8.11.1,
JDK 17, and SDK 35. `native/android/client` is the first application module and
depends on that library. Its first-run shell creates/restores the Rust-backed
hardware identity off the main thread, then signs the phone OTP enrollment proof
through the native transcript bridge. It stores only public identity/account
metadata; the bearer token is memory-only. Set `AUTH_BASE_URL` in the client build
configuration to the HTTPS account-auth endpoint. The repository does not bundle
a Gradle distribution/wrapper.

## Verification and remaining gates

```sh
cargo test --workspace --all-targets --locked
# Separate, disposable PostgreSQL database; every test creates its own schema.
cargo test --workspace --all-targets --locked -- --ignored
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo check -p links-client-core --target wasm32-unknown-unknown --locked
cargo build -p links-identity-ffi --locked
swift test --package-path native/apple
bash native/android/tests/run-host-tests.sh
# In an entitled hardware-capable Apple test environment:
LINKS_TEST_SECURE_ENCLAVE=1 swift test --package-path native/apple
# With the Android SDK and an unlocked physical TEE/StrongBox device:
gradle -p native/android connectedDebugAndroidTest :client:connectedDebugAndroidTest
```

PostgreSQL tests cover enrollment, returning-device login, proof rejection,
rate limits, five-attempt lockout, replay, concurrent approval, provider failure,
expiry during provider I/O, supersession, restarts, revocation, account disablement,
and HTTP request/response behavior. Local HTTP mock tests exercise both delivery
channels and provider response validation; they do not prove live delivery.

The Apple package's six non-hardware tests and arm64 iOS link check passed.
Three opt-in hardware tests remain unexecuted in an entitled physical test host.
The earlier macOS Secure Enclave attempt returned OS status `-34018` (missing
entitlement), so no hardware round-trip is claimed. The real JNI/Rust bridge passed
host JVM tests; Android production Java compiled against Android API classes.
Rust archives cross-built for both Android ABIs. A full Gradle/NDK build and
physical-device tests still require the Android SDK/device harness. Hosted CI
jobs were updated, not executed in this session.

Before closing the hardware TODO, record signed physical iOS and Android
round-trip, process restart, deletion, locked-device, tamper and key-invalidation
results using the [acceptance guide](hardware-identity.md). Native-to-Rust FFI
wiring is implemented. Live SMS and WhatsApp acceptance separately requires
configured provider credentials and explicit test sends. No public release
should proceed without the broader Phase 1 security review.
