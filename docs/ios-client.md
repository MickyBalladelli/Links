# iOS client foundation

`native/apple` now exports a `LinksClient` Swift target on top of the existing
`LinksKeyStore` hardware adapter. `IOSClient` owns the first iOS client boundary:

- creates and validates the Secure Enclave-backed identity;
- stores only the hardware handle, public identity key, device ID, MLS node ID,
  and optional account ID in `UserDefaults`;
- keeps the bearer token in memory only;
- rejects partial or substituted metadata after restart; and
- requires a `SharedClientCoreFactory` to construct the shared Rust
  `links-client-core` providers, passing signing through `HardwareIdentityStore`
  instead of exporting a seed.

The factory boundary prevents a Swift protocol or crypto fork. Its production
implementation must bind Rust `ClientCore`, MLS state, Sealed Sender key
resolution, and durable inbox/outbox storage. Missing providers fail closed.

This target is the iOS foundation only. OTP transport, TLS WebSocket lifecycle,
send/receive UI, APNs recovery, and account recovery remain the following Phase
4 tasks. The iOS release gate still requires Android interop, restart/replay,
tamper, Secure Enclave, background, and battery evidence on physical devices.
