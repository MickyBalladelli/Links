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

The seed never crosses the WASM/TypeScript boundary. Do not put it in
`localStorage`, URLs, analytics, or logs. The current facade keeps it in memory;
refresh or process loss requires a fresh explicit pairing until a durable,
reviewed Web provider is added. Persist only public device metadata and the MLS
credential through the host's storage boundary.

Build the Web-facing WASM package from the `web` directory:

```sh
npm run build:wasm
```

This is the Web identity/pairing foundation. WebSocket reconnect, cursor replay,
encrypted text orchestration, and browser UI are separate roadmap tasks.
