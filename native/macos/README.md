# Links macOS app

This folder contains the macOS 13+ SwiftUI application target. The app uses
the local `native/apple` Swift package and links both of its products:

- `LinksClient` for the shared client host boundary;
- `LinksKeyStore` for Apple hardware-backed identity custody.

The app currently targets arm64 macOS. The package's `LinksKeyStore` target
declares the `links_identity_ffi` linker dependency and reads
`LINKS_IDENTITY_LIB_DIR` when a non-default Rust output directory is needed.
Intel support stays out of scope until an x86_64 Rust archive is built and
validated.

The shell creates the local identity on first run, shows account/device/
connection state, and only sends through an installed `IOSDirectMessaging`
host. Without concrete Rust-core and durable providers it remains in a safe
"Core not configured" state.

Build the Rust library first, then build a signed Debug app after choosing an
Apple Development team in Xcode:

```sh
cargo build -p links-identity-ffi --locked
xcodebuild \
  -project native/macos/Links.xcodeproj \
  -scheme Links-Debug \
  -configuration Debug \
  -sdk macosx \
  build
```

Use the `Links-Release` scheme and `-configuration Release` for a release
build. The target enables Automatic signing, the hardened runtime, and
`Links/Links.entitlements`. Choose an Apple Development team in Xcode for
Debug builds. Use a Developer ID Application identity and the same team for
release distribution.

## Local unsigned debug

Use this path when no Apple signing team or certificate is available:

```sh
cargo build -p links-identity-ffi --locked
xcodebuild \
  -project native/macos/Links.xcodeproj \
  -scheme Links-Debug \
  -configuration Debug \
  -sdk macosx \
  -derivedDataPath /private/tmp/links-debug-derived-data \
  CODE_SIGNING_ALLOWED=NO \
  CODE_SIGNING_REQUIRED=NO \
  build
```

The unsigned app is useful for UI and package-link checks. It has no signed
Keychain entitlements, so hardware identity creation can fail with a missing
Keychain entitlement. Use a signed Debug build to exercise Secure Enclave and
Keychain storage.

The SwiftUI app observes `scenePhase` so future transport and durable store
hosts have explicit active, inactive, and background lifecycle hooks.

On background and application termination, the macOS delegate calls
`IOSDirectMessaging.shutdown()`. This closes transport and releases the core;
it does not clear the shared core's encrypted outbox or durable cursor. If the
user had requested a connection, the shell creates a fresh transport/core on
the next active phase and resumes from the persisted cursor. Abrupt process
crashes still depend on the shared durable store committing each outbox and
inbox operation before it returns.

The package's `LINKS_IDENTITY_LIB_DIR` environment variable can point at a
matching Rust target/profile directory when the default `target/debug` path is
not appropriate.

## Identity seed custody

The macOS client passes `MacOSKeychainSeedProvider` into
`HardwareIdentityStore`. That provider delegates to `HardwareSeedVault`, which
creates a Secure Enclave wrapping key and stores only the wrapped seed record
in Keychain. Seed bytes are exposed only for the synchronous Rust operation and
are wiped immediately after use. `IOSClient` persists public identity metadata
only; the seed is never written to UserDefaults, files, logs, URLs, or
analytics.

Pass an explicit profile to run more than one local identity:

```sh
open -n "/path/to/Links.app" --args --profile alice
open -n "/path/to/Links.app" --args --profile bob
```

Profile names are canonical lower-case ASCII names. Each non-default profile
uses its own Keychain service and public metadata key. A mismatched provider
and client profile is rejected before metadata is opened, so one local client
cannot accidentally validate or overwrite another client's identity.
