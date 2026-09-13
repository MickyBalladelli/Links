# Links Web client

This is the Web host for the shared Rust/WASM client core. Build the WASM
package first, import the generated module, then create `LinksWebClient` with
the existing account/user ID and fresh Web device/node IDs.

```sh
npm run build:wasm
```

`pairingURI()` returns the signed `links://connect?...` text for a QR renderer.
After the mobile device approves `POST /v1/devices`, pass the returned public
identity key and MLS credential to `completePairing()`. The WASM core checks
that the response matches the Web device identity and expected Links credential.

The Web host must keep the identity seed inside the WASM/provider boundary. Do
not put it in `localStorage`, URLs, analytics, or logs. IndexedDB persistence,
WebSocket reconnect, replay, and encrypted text UI are separate follow-up
features in the roadmap.
