# Desktop client foundation

`crates/desktop-client` is the platform-neutral Rust foundation for the
desktop companion. It depends on `links-client-core` and does not create a
second crypto or MLS implementation.

`DesktopClient` creates a fresh device and MLS node identity for an existing
account, or accepts a seed from an explicit desktop key provider. It emits the
same signed `links://connect?...` pairing URI used by Web, validates the mobile
registration response against the exact device identity, and exposes only
public metadata plus the server-created MLS credential.

After pairing, the desktop host binds the shared core with:

1. `DesktopClient::core_identity()` for `LocalIdentity` and the MLS credential.
2. `DesktopClient::signer()` for OpenMLS signing and PQXDH pre-key signatures.
3. A shared `ClientCore` with the host's crypto and MLS providers.
4. `bind_desktop_text_session()`, which wraps that core in the concrete
   `RustDesktopMessagingCore` and attaches the native `DesktopSocketFactory`.

`DesktopCoreHostAdapter` is the concrete host orchestrator at this layer. Its
`DesktopCoreServices` implementation performs the public directory lookup,
claims one pre-key per active recipient device, and supplies the matching MLS
KeyPackage. The adapter verifies every claim through the shared core, stages
and delivers pending MLS commits, encrypts one MLS message, creates exact
per-device Sealed Sender envelopes, persists those exact Send frames, and then
writes them to the socket.

The same adapter validates replay batches, decrypts each envelope through
`ClientCore`, commits inbox data and the cursor through one durable service
call, sends QueueAck only after that commit, and invokes the UI callback last.
`DesktopCoreHost` remains the small protocol seam; `DesktopCoreHostAdapter`
is the reference implementation. Its service must encrypt message data before
local persistence and must never persist the bearer token. The Apple wrapper
also buffers callbacks until `handleServerFrame` returns, so a UI queue cannot
render while the shared-core transaction is still running.
The adapter validates every binary protobuf server frame, creates a fresh
protocol-v1 Hello, and keeps the bearer token memory-only through the supplied
token closure.

`DesktopTextSession` adds the desktop lifecycle around that binding. Its
`DesktopSocketFactory` must return a TLS binary `links.v1` socket; the session
creates a fresh core Hello from the in-memory bearer and durable cursor on each
connection attempt. `poll()` drains binary frames into the shared core and
emits only committed text events. The core decodes plain or negotiated Zstd
dictionary `CompressedSyncBatch` frames before committing inbox state. Outbound
text delegates to the core, which owns MLS encryption, Sealed Sender envelopes,
durable outbox state, and device fanout.

`DesktopTextSession` exposes the matching image boundary. The desktop host
provides already normalized/transcoded WebP or AVIF bytes and RGB pixels;
`prepare_and_encrypt_image()` asks the shared core for the private BlurHash and
image ciphertext. `DesktopImageUploader` uploads opaque ciphertext and the
receipt must match before `send_image()` sends private `MediaMetadata`. Download
uses `DesktopImageCache` (with `DesktopImageFileCache` as the ciphertext-only
reference), verifies the blob, decrypts through the core, and passes plaintext
only to `DesktopImageRenderer` for immediate rendering.

The connection manager sends native ping heartbeats every 30 seconds and uses
full-jitter reconnect delays from 1 to 30 seconds. `recover()` deliberately
reconnects so the next Hello starts at the latest durable cursor, then drains
replay until the core reports recovery complete. A failed core frame stops the
session and requires an explicit restart.

The seed is held in process memory by the foundation. Production desktop
applications must supply an audited OS keychain/provider and durable local
metadata boundary before relying on restart persistence. The native socket,
durable `ClientCore` provider, and UI event loop remain host integration work.

## Logging boundary

The desktop and gateway APIs do not log request bodies, message events, MLS
state, seeds, bearer tokens, or opaque sealed payloads. Secret-bearing Rust
values have redacted `Debug` implementations: received text, call access
tokens and SDP, SFrame epoch keys, image keys/ciphertext, media relay tokens,
and push bodies render only as `REDACTED`. The server's only built-in process
messages are startup and generic cleanup-failure messages. Production socket,
HTTP, queue, proxy, crash, trace, and analytics integrations must keep body
logging disabled and record only allowlisted status, size, latency, and
failure-class fields.
