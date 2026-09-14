# Mini-app WASM sandbox

Mobile and desktop clients execute mini-app modules with the native
`wasmi` interpreter in `crates/client-core/src/sandbox.rs`. The Web client does
not use this native module; browser mini-app execution needs a separately
audited browser runtime and CSP/worker policy.

## Guest ABI

The module must export:

```text
links_run: () -> i32
memory: linear memory
```

The only allowed imports are functions in the `links` namespace:

```text
links.input_len() -> i32
links.input_read(destination: i32, maximum: i32) -> i32
links.output_write(source: i32, length: i32) -> i32
```

The host gives each invocation fresh linear memory and input. The guest reads
the request with `input_read`, writes its response with `output_write`, and
returns zero from `links_run` on success. Non-zero status traps the invocation.

## Limits and capabilities

The default hard limits are:

- module: 2 MiB
- linear memory: 16 MiB
- input: 64 KiB
- output: 256 KiB
- execution: 5,000,000 Wasmi fuel units
- recursion depth: 256

There is no WASI linker. Modules cannot import sockets, files, clocks,
randomness, threads, identity handles, MLS state, or private keys. Unknown
imports, missing entrypoint, missing exported memory, malformed pointers, fuel
exhaustion, and resource growth fail closed.

## Native hosts

`IOSMiniAppSandbox` and `AndroidMiniAppSandbox` own opaque native handles and
call the FFI create/run/destroy functions. The FFI copies no identity or key
material into the guest. `DesktopMiniAppSandbox` re-exports the same
`SandboxRuntime` from the desktop Rust crate.

Mini-app permissions, network mediation, and organization-level enablement are
separate release gates. Until those are implemented, mini-app code has only
the bounded input/output ABI above.
