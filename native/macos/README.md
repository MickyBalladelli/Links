# Links macOS app

This folder contains the macOS 13+ SwiftUI application target. The app uses
the local `native/apple` Swift package and links both of its products:

- `LinksClient` for the shared client host boundary;
- `LinksKeyStore` for Apple hardware-backed identity custody.

Build the Rust library first, then build the app from the repository root:

```sh
cargo build -p links-identity-ffi --locked
xcodebuild \
  -project native/macos/Links.xcodeproj \
  -scheme Links \
  -configuration Debug \
  -sdk macosx \
  CODE_SIGNING_ALLOWED=NO
```

The package's `LINKS_IDENTITY_LIB_DIR` environment variable can point at a
matching Rust target/profile directory when the default `target/debug` path is
not appropriate.
