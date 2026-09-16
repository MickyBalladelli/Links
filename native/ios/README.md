# Links iOS app

This folder contains the runnable internal iOS Debug target. It embeds the
local Swift package products `LinksClient` and `LinksKeyStore`.

The current mobile shell supports:

- Secure Enclave-backed identity enrollment;
- local-development username registration and login;
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

For a connected phone, target its UDID so Xcode registers that device in the
development profile:

```sh
DEVELOPMENT_TEAM=YOUR_TEAM_ID \
LINKS_IOS_DESTINATION="id=YOUR_DEVICE_UDID" \
bash scripts/build-ios-debug.sh
```

Build and install in one step:

```sh
DEVELOPMENT_TEAM=YOUR_TEAM_ID \
LINKS_AUTH_URL=https://your-phone-reachable-auth-host \
LINKS_IOS_DEVICE_ID=YOUR_DEVICE_UDID \
bash scripts/launch-iphone.sh
```

If `LINKS_AUTH_URL` is omitted, the launcher detects the Mac's default LAN
IPv4 address, starts the built-in Node HTTPS bridge, and uses
`https://<mac-ip>:8443`. The bridge forwards account-auth HTTP traffic to
`127.0.0.1:8080` and WebSocket upgrades at `/v1/connect` to the local gateway
at `127.0.0.1:8081`. Its generated certificate and log live under
`native/ios/LocalHTTPS/`.

Start the local Rust services before launching the iPhone build. The launcher
checks that account-auth is reachable on port 8080 and stops with a clear
error if it is not. Do not use `https://127.0.0.1:8443` for a physical
iPhone: that points to the iPhone itself. If a stale loopback URL is exported,
the launcher replaces it with the Mac LAN endpoint automatically.

You can run the bridge by itself with:

```sh
LINKS_LAN_IP="$(ipconfig getifaddr en0)" node scripts/local-https-proxy.mjs
```

The iPhone must trust the generated certificate before making requests:

1. Use the exact `root-cert.cer` path printed by
   `scripts/launch-iphone.sh`. Copy that file to the iPhone with AirDrop or
   Files and install the certificate profile. Do not install an old
   `cert-<mac-ip>.cer` server certificate.
2. Open Settings > General > About > Certificate Trust Settings.
3. Enable full trust for the Links local certificate.

If the certificate was already installed, remove the old Links profile first
under Settings > General > VPN & Device Management, then install and trust the
new root certificate. The root certificate is stable and does not contain the
Mac IP. The proxy creates a new server certificate with the current IP when
needed, so changing IP does not require reinstalling the root certificate.
Rebuild with `scripts/launch-iphone.sh` if the Mac IP changed.

The certificate is for local development only. For a real account, use a
trusted HTTPS host with `LINKS_AUTH_URL`. The local Rust username mode does
not implement Twilio OTP; the bridge provides HTTPS reachability but cannot
send a verification code without a real account-auth service configured for
Twilio Verify.

For local iPhone testing, create the identity first, then use **Local
development account** in the app. Choose **Register**, enter a lowercase
username such as `alice`, and tap **Register username**. On a later install or
another profile, choose **Log in**. Each username is one account, and the
hardware identity signs the request; no password or SMS is used.

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
