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
3. The host's own `ClientCore` crypto, MLS provider, durable store, and network
   adapters.

`DesktopTextSession` adds the desktop lifecycle around that binding. Its
`DesktopSocketFactory` must return a TLS binary `links.v1` socket; the session
creates a fresh core Hello from the in-memory bearer and durable cursor on each
connection attempt. `poll()` drains binary frames into the shared core and
emits only committed text events. The core decodes plain or negotiated Zstd
dictionary `CompressedSyncBatch` frames before committing inbox state. Outbound
text delegates to the core, which owns MLS encryption, Sealed Sender envelopes,
durable outbox state, and device fanout.

The connection manager sends native ping heartbeats every 30 seconds and uses
full-jitter reconnect delays from 1 to 30 seconds. `recover()` deliberately
reconnects so the next Hello starts at the latest durable cursor, then drains
replay until the core reports recovery complete. A failed core frame stops the
session and requires an explicit restart.

The seed is held in process memory by the foundation. Production desktop
applications must supply an audited OS keychain/provider and durable local
metadata boundary before relying on restart persistence. The native socket,
durable `ClientCore` provider, and UI event loop remain host integration work.
