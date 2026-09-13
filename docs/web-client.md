# Web client foundation

The Web client is split into a Rust/WASM identity facade and a small TypeScript
host:

- `crates/web-client` owns the Web identity seed in the WASM instance and calls
  `links-client-core::pairing::PairingPayload` and
  `PairingRegistrationResponse`.
- `web/src/LinksWebClient.ts` validates browser-facing strings, asks WASM for a
  signed pairing URI, and installs the mobile approval response.

## Bootstrap flow

```text
Web WASM identity -> signed links://connect URI -> mobile approval
mobile POST /v1/devices -> public MLS credential -> Web WASM identity
```

`LinksWebClient.pairingURI()` returns URI text for a browser QR renderer. The
mobile side submits only `device_id`, `mls_node_id`, `public_key`, `nonce`, and
`signature` with its bearer session. The Web host passes the returned public key
and MLS credential to `completePairing()`. WASM verifies that the credential is
the expected Links BasicCredential and matches this exact Web device identity.

`WebConnectionManager` owns the browser `wss://<host>/v1/connect` lifecycle.
It requests the `links.v1` subprotocol, sends one binary Hello frame within five
seconds, rejects text or oversized frames, and reports binary ServerFrame bytes
to the shared-core host callback. It reconnects with full jitter and exponential
backoff, capped at 30 seconds, resetting after a stable 30-second connection.
Browser WebSocket APIs automatically answer server ping frames and do not expose
ping/pong callbacks; the manager therefore uses the browser's close/error events
and periodic open-state checks for liveness. It never puts a bearer token in the
endpoint URL.

`WebTextMessaging` binds that connection to a `WebMessagingCore` adapter. The
adapter is the boundary for the Rust client core and durable browser stores: it
owns MLS epochs, Sealed Sender encryption/decryption, outbox state, inbox
commit, cursor replay and delivery acknowledgements. Each Hello reads the
current in-memory bearer and durable cursor, so reconnects resume from the last
committed message. Incoming binary frames go directly to the core; the core
must commit a decrypted message before the callback is invoked. The current
WASM build does not advertise Zstd and therefore receives plain `SyncBatch`
frames; a future browser-native decoder may opt into `CompressedSyncBatch`.
Outbound text
is validated by the host and then passed to the core, which creates and queues
the encrypted envelope and device fanout. The browser never builds or inspects
the encrypted payload.

The seed never crosses the WASM/TypeScript boundary. Do not put it in
`localStorage`, URLs, analytics, or logs. The current facade keeps it in memory;
refresh or process loss requires a fresh explicit pairing until a durable,
reviewed Web provider is added. Persist only public device metadata and the MLS
credential through the host's storage boundary.

Build the Web-facing WASM package from the `web` directory:

```sh
npm run build:wasm
```

Browser storage policy and message UI remain host-application work. The shared
Web sync shell covers encrypted one-to-one text transport and replay, but it
does not choose a storage provider or render a particular interface.
