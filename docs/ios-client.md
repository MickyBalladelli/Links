# iOS client foundation

`native/apple` now exports a `LinksClient` Swift target on top of the existing
`LinksKeyStore` hardware adapter. `IOSClient` owns the first iOS client boundary:

- creates and validates the Secure Enclave-backed identity;
- signs the exact phone and enrollment transcripts through the Rust identity
  FFI and completes HTTPS `/v1/auth/start` and `/v1/auth/finish`;
- stores only the hardware handle, public identity key, device ID, MLS node ID,
  and optional account ID in `UserDefaults`;
- keeps the bearer token in memory only;
- rejects partial or substituted metadata after restart; and
- requires a `SharedClientCoreFactory` to construct the shared Rust
  `links-client-core` providers, passing signing through `HardwareIdentityStore`
  instead of exporting a seed.

`IOSConnectionManager` supplies the native transport boundary. It accepts a
core-produced Hello, negotiates `links.v1` over `wss`, rejects text and frames
over 1 MiB, sends ping heartbeats every 30 seconds, and reconnects with
full-jitter backoff from 1 to 30 seconds. It reports frames to the host; the
host passes them to `SharedClientCore.handleServerFrame`.

The factory boundary prevents a Swift protocol or crypto fork. Its production
implementation must bind Rust `ClientCore`, MLS state, Sealed Sender key
resolution, and durable inbox/outbox storage. Missing providers fail closed.

This target is the iOS foundation, OTP, and connection layer. Send/receive UI,
APNs recovery, and account recovery remain the following Phase 4 tasks. The iOS
release gate still requires live OTP, Android interop,
restart/replay, tamper, Secure Enclave, background, and battery evidence on
physical devices.
