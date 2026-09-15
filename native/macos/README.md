# Links macOS app

This folder contains the macOS 13+ SwiftUI application target. The app uses
the local `native/apple` Swift package and links both of its products:

- `LinksClient` for the shared client host boundary;
- `LinksKeyStore` for Apple hardware-backed identity custody.

The app currently targets arm64 macOS. The package's `LinksKeyStore` target
declares the `links_identity_ffi` linker dependency and reads
`LINKS_IDENTITY_LIB_DIR` when a non-default Rust output directory is needed.
Intel support stays out of scope until an x86_64 Rust archive is built and
validated.

The shell creates the local identity on first run, shows account/device/
connection state, and only sends through an installed `IOSDirectMessaging`
host. Without concrete Rust-core and durable providers it remains in a safe
"Core not configured" state.

The macOS messaging path uses `IOSConnectionManager` as its native socket
adapter. Release builds accept only `wss://<host>/v1/connect`; Debug builds
also accept the loopback-only `ws://127.0.0.1/v1/connect` endpoint from the
local composition. It requests and verifies the `links.v1` subprotocol, uses
TLS 1.3 for WSS, sends and receives binary protobuf frames only, rejects
redirects and oversized frames, and handles ping heartbeats plus bounded
full-jitter reconnects. Hello is produced by the shared core and must arrive
within five seconds of the handshake.

Build the arm64 Rust library first with the same macOS deployment target, then
build a signed Debug app after choosing an Apple Development team in Xcode:

```sh
MACOSX_DEPLOYMENT_TARGET=13.0 cargo build -p links-identity-ffi --locked
xcodebuild \
  -project native/macos/Links.xcodeproj \
  -scheme Links-Debug \
  -configuration Debug \
  -sdk macosx \
  build
```

Use the `Links-Release` scheme and `-configuration Release` with the matching
Rust release archive:

```sh
MACOSX_DEPLOYMENT_TARGET=13.0 \
  cargo build -p links-identity-ffi --release --locked
LINKS_IDENTITY_LIB_DIR="$PWD/target/release" xcodebuild \
  -project native/macos/Links.xcodeproj \
  -scheme Links-Release \
  -configuration Release \
  -sdk macosx \
  build
```

The target enables Automatic signing, the hardened runtime, and
`Links/Links.entitlements`. Choose an Apple Development team in Xcode for
Debug builds. Use a Developer ID Application identity and the same team for
release distribution.

## Local unsigned debug

Use this path when no Apple signing team or certificate is available:

```sh
MACOSX_DEPLOYMENT_TARGET=13.0 cargo build -p links-identity-ffi --locked
xcodebuild \
  -project native/macos/Links.xcodeproj \
  -scheme Links-Debug \
  -configuration Debug \
  -sdk macosx \
  -derivedDataPath /private/tmp/links-debug-derived-data \
  CODE_SIGNING_ALLOWED=NO \
  CODE_SIGNING_REQUIRED=NO \
  build
```

The unsigned app is useful for UI and package-link checks. It has no signed
Keychain entitlements, so hardware identity creation can fail with a missing
Keychain entitlement. Use a signed Debug build to exercise Secure Enclave and
Keychain storage.

The SwiftUI app observes `scenePhase` so future transport and durable store
hosts have explicit active, inactive, and background lifecycle hooks.

### Delivery states

The client shell exposes transport and durable-delivery state in the sidebar
and conversation header. It shows reconnecting and encrypted outbox retry
counts, keeps send failures visible, routes expired authentication back to
sign-in, and reports dependency outages without discarding local state. A
stale replay cursor shows a `Recover` action; recovery resets the cursor only
through the shared core boundary and fails closed when that host operation is
not available. Messages are rendered only from the post-commit callback.

## Local username auth and pairing

After the hardware identity is enrolled, the first-run screen can register or
log in a username against the local account-auth service. The default endpoint
is `http://127.0.0.1:8080`; override it with `LINKS_AUTH_URL` or
`--auth-url <url>`. Plain HTTP is accepted only for loopback development. A
non-loopback endpoint must use HTTPS.

Phone enrollment appears on the same screen when `--auth-url` points to an
HTTPS account-auth service configured with a real Twilio Verify account. The
macOS host sends the signed device proof, accepts SMS or WhatsApp codes, and
keeps the phone number and verification code in memory only. Loopback HTTP
deliberately disables this OTP path.

The account screen can create a signed `links://connect` link for an existing
account. Scan it on that account's authenticated device, then log in on this
Mac with the account username. An authenticated macOS client can also approve
a link from another device with the **Pair device** action, or by opening a
registered `links://connect?...` URL. The app verifies the URI signature and
account before calling `POST /v1/devices`; bearer tokens never enter the URL or
the pairing payload.

## Pre-keys and first secure conversation

After a shared-core host is installed, `connect` asks the core to maintain the
initial pre-key inventory. `IOSPreKeyHTTPClient` sends only protobuf public
material to `GET /v1/prekeys/status` and `PUT /v1/prekeys`; the core generates
the keys, wraps private seeds through its native provider, and durably retries
the exact upload bytes.

`initializeSelectedConversation()` asks an authenticated directory adapter for
the recipient's active devices and MLS KeyPackages, claims one bundle per
device through `POST /v1/prekeys/{device_id}/claim`, and passes the opaque
claims to the shared core. The core verifies each claim against the directory
identity key, establishes the two-user MLS group, and delivers the pending
commit before sending application text. Swift does not parse or trust claimed
key material. The platform-neutral Rust binding includes
`DesktopCoreHostAdapter` for this host integration: its
`DesktopCoreServices` implementation supplies directory/pre-key I/O and the
encrypted durable store, while the adapter performs claim verification,
device fan-out, envelope sealing/decryption, cursor commit, and QueueAck. A
host without the concrete Rust core or authenticated directory stays
fail-closed and reports the missing integration.

On background and application termination, the macOS delegate calls
`IOSDirectMessaging.shutdown()`. This closes transport and releases the core;
it does not clear the shared core's encrypted outbox or durable cursor. If the
user had requested a connection, the shell creates a fresh transport/core on
the next active phase and resumes from the persisted cursor. Abrupt process
crashes still depend on the shared durable store committing each outbox and
inbox operation before it returns.

The package's `LINKS_IDENTITY_LIB_DIR` environment variable can point at a
matching Rust target/profile directory when the default `target/debug` path is
not appropriate.

## Local persistence

`IOSClient` stores only the hardware handle, public key, account/device IDs,
account handle, and MLS credential in the profile-scoped UserDefaults record.
The bearer session is held only in memory and is never encoded in that record.
macOS shell state is serialized as ciphertext under the profile's Application
Support directory. `MacOSEncryptedStateStore` uses AES-GCM and keeps its
profile-scoped 256-bit key in Keychain; invalid or tampered state is ignored.
The shared core remains responsible for its own encrypted MLS, inbox, outbox,
and cursor state when a concrete durable provider is installed.

`MacOSDurableMessagingStore` is the profile-scoped provider for that host
boundary. It stores opaque encrypted MLS state, encrypted inbox messages,
exact encrypted outbox frames, message-ID reservations, conversation sequence
counters, and the replay cursor in `messaging-v1.bin` under the profile's
Application Support directory. Each mutation atomically replaces the
encrypted document; `withTransaction` commits MLS state, inbox, outbox,
sequence, and cursor changes together. The store is constructed by
`LinksMacOSAppModel` and can be passed to the concrete Rust-core host factory.

## Identity seed custody

The macOS client passes `MacOSKeychainSeedProvider` into
`HardwareIdentityStore`. That provider delegates to `HardwareSeedVault`, which
creates a Secure Enclave wrapping key and stores only the wrapped seed record
in Keychain. Seed bytes are exposed only for the synchronous Rust operation and
are wiped immediately after use. `IOSClient` persists public identity metadata
only; the seed is never written to UserDefaults, files, logs, URLs, or
analytics.

## Contacts

After account authentication, use the person-plus button in the messaging
sidebar to look up a known username through the account directory. The client
saves only the public handle, account ID, and active-device count in the
profile's encrypted local state. Saved contacts appear in the Contacts list;
click one to create or open its conversation. The directory does not expose a
public list of every account.

Pass an explicit profile to run more than one local identity:

```sh
open -n "/path/to/Links.app" --args \
  --profile alice --profile-root "$HOME/Library/Application Support/Links/profiles"
open -n "/path/to/Links.app" --args \
  --profile bob --profile-root "$HOME/Library/Application Support/Links/profiles"
```

Each `open -n` invocation creates its own app process, model, transport, and
ephemeral bearer session. The macOS client has no process-wide app singleton,
shared database, or cross-profile lock.

Use the repository launcher to start both local clients:

```sh
bash scripts/launch-macos-two-client.sh \
  "/path/to/Links.app" \
  "$HOME/Library/Application Support/Links/profiles"
```

It runs the equivalent of two separate `open -n` commands for `alice` and
`bob`. Pass different profile names as the third and fourth arguments when
needed. `LINKS_AUTH_URL` selects the account-auth endpoint; it defaults to the
loopback development service.

## Runner readiness

Each profile writes `<profile-root>/<profile>/status.json`. The JSON contains
only the profile name, lifecycle state, authentication and connection booleans,
process ID, and an ISO-8601 update time. A runner can wait for both clients
without reading logs or client data:

```sh
jq -e '(.state == "ready") and .authenticated and .connected' \
  "$PROFILE_ROOT/alice/status.json"
jq -e '(.state == "ready") and .authenticated and .connected' \
  "$PROFILE_ROOT/bob/status.json"
```

The file is atomically replaced on every lifecycle transition. `ready` means
the account is authenticated and the `links.v1` WebSocket is live; startup,
retry, recovery, and failure states remain non-ready.

## Two-client smoke harness

With the local composition already running, execute:

```sh
bash scripts/smoke-two-client.sh
```

The harness creates disposable `@alice-test` and `@bob-test` accounts using
the loopback service's canonical `alice_test` and `bob_test` handles, opens two
concurrent `links.v1` sessions, and routes one opaque envelope in each
direction. It then tries to reuse Alice's active `device_id`; that connection
must receive a session-conflict error while the original Alice and Bob
connections remain usable. It prints one JSON record containing only `result`
and timing fields.
Set `SMOKE_RESULT_PATH` to save that same record, or override the local
endpoints with `SMOKE_AUTH_URL` and `SMOKE_GATEWAY_URL`. Use a fresh disposable
database when the fixed test handles have already been registered.

`--profile-root` (or `LINKS_PROFILE_ROOT`) selects the directory containing
profile directories. The active profile is stored below
`<profile-root>/<profile>/`; the default is
`~/Library/Application Support/Links/profiles/<profile>`. Each profile has
its own encrypted Application Support state, `logs/client.log`, Keychain
namespace, public metadata suite, bearer session, device ID, and MLS node ID.
Profile names are canonical lower-case ASCII names. A mismatched provider and
client profile is rejected before metadata is opened, so one local client
cannot accidentally validate or overwrite another client's identity.

The app creates one ephemeral URL session and one in-memory bearer session per
process/profile. Bearer tokens are never written to the profile root, logs,
Keychain, URLs, or UserDefaults. The profile logger accepts fixed status event
names only and never records message text, IDs, or payloads.

New profiles generate distinct non-nil device and MLS-node UUIDs. The account
service generates the user UUID during username or phone registration. After a
profile is bound, a later session must return the same user, handle, and MLS
credential; a mismatch fails closed instead of rebinding the local identity.
