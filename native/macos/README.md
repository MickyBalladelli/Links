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

Build the Rust library first, then build the app from the repository root:

```sh
cargo build -p links-identity-ffi --locked
xcodebuild \
  -project native/macos/Links.xcodeproj \
  -scheme Links-Debug \
  -configuration Debug \
  -sdk macosx \
  CODE_SIGNING_ALLOWED=NO
```

Use the `Links-Release` scheme and `-configuration Release` for a release
build. The SwiftUI app observes `scenePhase` so future transport and durable
store hosts have explicit active, inactive, and background lifecycle hooks.

The package's `LINKS_IDENTITY_LIB_DIR` environment variable can point at a
matching Rust target/profile directory when the default `target/debug` path is
not appropriate.
