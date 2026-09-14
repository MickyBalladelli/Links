# Mini-App permission SDK

`links-client-core::sandbox` gives Mini-Apps a default-deny capability boundary.
The WASM module cannot open sockets, use WASI, read files, access clocks or
randomness, inspect identity state, or receive private key bytes.

Hosts opt into capabilities with `SandboxPermissions` and execute through
`SandboxRuntime::run_with_host`:

```rust
let rule = SandboxNetworkRule::new(
    "api.example.com",
    ["GET", "POST"],
    16 * 1024,
    64 * 1024,
)?;
let grant = SandboxCryptoGrant::new(
    opaque_key_handle,
    [SandboxCryptoOperation::Sign],
    8 * 1024,
    128,
)?;

let mut permissions = SandboxPermissions::deny_all();
permissions.grant_network(rule)?;
permissions.grant_crypto(grant)?;
let output = runtime.run_with_host(input, &permissions, &mut host)?;
```

Network access is only available through the `links.network_request` import.
Requests must use HTTPS, match one exact granted host, use a granted method, and
stay below the request and response byte limits. The host mediator must also
reject redirects, proxying, cookies, and DNS/IP destinations outside the same
approved host.

Crypto access is only available through `links.crypto_operation`. The module
passes a non-zero 32-byte opaque capability identifier plus an operation code:
`1` hash, `2` HMAC, or `3` sign. The host resolves that identifier internally;
the runtime never receives or returns key material. Grants independently limit
the operation and input/output sizes.

`SandboxRuntime::run` uses `DenyAllHost`, so existing native FFI wrappers remain
safe by default. Desktop hosts can implement `SandboxHost` for their audited
network and hardware-key adapters. Mobile hosts should keep the same mediator
shape when adding platform callbacks: URL requests go through the app’s
network policy, and signing/HMAC goes through Secure Enclave or Android
Keystore-backed handles.
