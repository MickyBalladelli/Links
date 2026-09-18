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
WebSocket reconnect and browser message UI remain host-application work. Use
`WebTextMessaging` with a shared-core adapter for encrypted one-to-one sync;
the adapter owns MLS, Sealed Sender, durable cursors and local inbox/outbox
state. Browser hosts should use `logoutAccountSession`; it attempts remote
revocation first and invokes the supplied local-clear callback even when the
network request fails.
