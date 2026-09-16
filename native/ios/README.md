# Links iOS app

This folder contains the runnable internal iOS Debug target. It embeds the
local Swift package products `LinksClient` and `LinksKeyStore`.

The current mobile shell supports:

- Secure Enclave-backed identity enrollment;
- phone OTP enrollment against a real HTTPS account-auth service;
- signed `links://connect` URL intake; and
- authenticated device approval through `POST /v1/devices`.

The bearer token stays in memory. The app does not log phone numbers, OTP
codes, pairing payloads, seeds, or message text.

## Physical iPhone Debug build

Select a real Apple Development Team in Xcode, or pass it on the command line:

```sh
DEVELOPMENT_TEAM=YOUR_TEAM_ID bash scripts/build-ios-debug.sh
```

The script builds both Rust static libraries for `aarch64-apple-ios`, then
builds and signs `Links.app` with the `Links-iOS-Debug` scheme. Open the
resulting app in Xcode, select a connected iPhone, and run it. Automatic
signing must have a registered App ID for `ai.links.Links.iOS` and the
Keychain access capability enabled.

For terminal installation, trust the iPhone and enable Developer Mode first:

```sh
xcrun devicectl device list
xcrun devicectl device install app \
  --device YOUR_DEVICE_UDID \
  native/ios/DerivedData/Build/Products/Debug-iphoneos/Links.app
```

If Xcode reports that the iOS platform is missing, install the matching iOS
platform from Xcode > Settings > Components before running the script.

Set `LINKS_AUTH_URL` in the Xcode scheme or build settings to a reachable
HTTPS account-auth endpoint. A phone cannot reach the Mac through
`127.0.0.1`; use a LAN hostname or IP with a trusted development certificate.
The local HTTP Debug endpoint is intentionally not accepted by the iOS host.

The iOS target is an internal onboarding and pairing build. Full mobile
WebSocket messaging, QR scanning, and conversation UI remain separate mobile
validation tasks in `TODO.md`.
