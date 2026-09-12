# Standard consumer account

OTP authentication and Ed25519/MLS credential enrollment are implemented and tested.
Native seed vaults are implemented and compile-checked; hardware acceptance is
still open. This is not a finished mobile onboarding UI or an audited E2EE system.

## What is implemented

`links-account-auth` is a loopback HTTP service backed by PostgreSQL. Its provider
adapter uses [Twilio Verify](https://www.twilio.com/docs/verify/api) for SMS or
[WhatsApp](https://www.twilio.com/docs/verify/whatsapp). Code generation and delivery
are delegated to Verify, not a custom OTP generator. Production code has no fixed
OTP, console-code output, in-memory auth database, or bypass provider. Mock
providers are confined to tests. Live sends incur provider charges and were not
performed during implementation.

`links-identity` generates 32-byte random Ed25519 seeds, derives public keys, signs
request transcripts, verifies signatures strictly, and produces an RFC 9420 basic
credential using TLS codec serialization. The core exposes `HardwareIdentityStore`
and `HardwareSeedVault`; Swift and Java supply native wrap/load/delete operations.
Mobile applications still need to wire these operations through their FFI layer
in the Android/iOS client phases.

## Security correction to the roadmap

Phone numbers and OTPs are not key seeds. The seed comes from the OS CSPRNG; phone
possession is verified separately. A keyed HMAC maps the phone to an opaque
account UUID in the server database. The device proves possession of its Ed25519
key before requesting a code, and again over a fresh challenge before enrollment.
The resulting account/device/node/public-key binding is stored atomically only
after verification. Public credentials contain no phone number or phone digest.

An [MLS basic credential](https://www.rfc-editor.org/info/rfc9420) is an application
identity assertion, not a certificate or proof of phone verification by itself.
Peers still require an authenticated directory/verification policy and signed MLS
leaf/key-package processing. Those MLS engine and peer-verification features are
not implemented here. The Ed25519 implementation uses ed25519-dalek; its behavior
is checked against [RFC 8032, section 7.1](https://www.rfc-editor.org/rfc/rfc8032.html#section-7.1).
No custom signature or encryption algorithm is introduced.

## API contract

All routes are under `/v1/auth`. JSON bodies are limited to 4 KiB. Binary fields
are base64url without padding. UUIDs are UUID strings. Responses, including errors,
have `Cache-Control: no-store`. Do not enable request/response-body logging at the
application, proxy, provider SDK, or analytics layer.

| Endpoint | Input | Result |
| --- | --- | --- |
| `POST /start` | `phone`, `channel`, `device_id`, `mls_node_id`, `public_key`, `signature` | Provisional account/device binding, `challenge_id`, nonce, expiry and MLS credential. |
| `POST /finish` | `challenge_id`, `code`, `signature` | A device-scoped bearer access token, expiry, user ID and device ID. |
| `GET /me` | `Authorization: Bearer <access_token>` | Authenticated user/device after current revocation and account-status checks. |

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
An existing account can only log in with its already registered, unrevoked device
and the same public key/node/credential. A new key or device requires the future
pairing/recovery flow; **OTP alone cannot replace existing identity keys**.
Ineligible requests receive provisional challenges without disclosing the real
account UUID. They never receive a session even with a correct OTP. This is not
a formal guarantee of enumeration resistance or of constant-time behavior.

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
erasure, SIM-swap recovery, or key-replacement endpoints in this slice. Account
creation, device registration, credential storage, token insertion and challenge
consumption commit together. Existing devices are never overwritten.

Phone lookup uses a domain-separated HMAC-SHA-256 with a required, stable server
secret. Raw phones, codes, seeds and bearer tokens are not database fields. The
provider necessarily receives the phone and code; its retention policy is separate.
Do not claim this is anonymous or zero-knowledge account authentication. The
minute cleanup job deletes expired sessions, challenges ten minutes after their
expiry (including replay markers), and rate-limit rows after the longest window.
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

```sh
cargo run -p links-server-store --example migrate --locked
cargo run -p links-account-auth --locked
```

The binary is not a TLS server: terminate TLS in a trusted local proxy before
external access. It deliberately ignores `X-Forwarded-For`; the IP limit is based
on the socket peer. A shared reverse proxy therefore shares this conservative
limit. Production ingress needs its own trusted client-IP rate limits, global
spend caps, bot protection, geographical delivery policy, connection limits and
provider fraud controls. Do not relax the socket limit by trusting arbitrary
forwarded headers. No production ingress or provider account was deployed here.

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
requires API 28+; its build targets SDK 35.

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
identity is hardcoded. The Android library uses AGP 8.9.2, Gradle 8.11.1, JDK 17,
and SDK 35. The repository does not bundle a Gradle distribution/wrapper.

## Verification and remaining gates

```sh
cargo test --workspace --all-targets --locked
# Separate, disposable PostgreSQL database; every test creates its own schema.
cargo test --workspace --all-targets --locked -- --ignored
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo check -p links-client-core --target wasm32-unknown-unknown --locked
swift test --package-path native/apple
# In an entitled hardware-capable Apple test environment:
LINKS_TEST_SECURE_ENCLAVE=1 swift test --package-path native/apple
# With the Android SDK and an unlocked physical TEE/StrongBox device:
gradle -p native/android connectedDebugAndroidTest
```

PostgreSQL tests cover enrollment, returning-device login, proof rejection,
rate limits, five-attempt lockout, replay, concurrent approval, provider failure,
expiry during provider I/O, supersession, restarts, revocation, account disablement,
and HTTP request/response behavior. Local HTTP mock tests exercise both delivery
channels and provider response validation; they do not prove live delivery.

The Apple package's non-hardware tests and iOS typecheck passed. The opt-in macOS
Secure Enclave test reached key creation but returned OS status `-34018` (missing
entitlement), so no hardware round-trip is claimed. Android production source
compiled against Android API classes from Robolectric; a full Gradle build and
physical-device tests were not run because the Android SDK/device harness is not
available here. Hosted CI jobs were added, not executed in this session.

Before closing the hardware TODO, run signed physical iOS and Android round-trip,
restart, deletion, locked-device, tamper and key-invalidation tests, then connect
the native vault to each client's Rust identity worker. Live SMS and WhatsApp
acceptance also require configured provider credentials and explicit test sends.
No public release should proceed without the broader Phase 1 security review.
