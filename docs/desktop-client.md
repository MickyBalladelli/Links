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

The seed is held in process memory by the foundation. Production desktop
applications must supply an audited OS keychain/provider and durable local
metadata boundary before relying on restart persistence. Device registration,
encrypted sync, reconnect, and recovery are the next desktop work item.
