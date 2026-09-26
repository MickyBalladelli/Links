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
not put it in `localStorage`, URLs, analytics, or logs. The browser messenger
adapts `WebMessagingCore` to `WebTextMessaging`: it owns MLS, Sealed Sender,
durable cursors, pre-key publication, recipient directory loading, and local
inbox/outbox frames while the host owns the binary WebSocket. Browser hosts
should use `logoutAccountSession`; it attempts remote
revocation first and invokes the supplied local-clear callback even when the
network request fails.
