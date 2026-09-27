# Links test map

Links has three test layers. Keep them separate. Small tests find bad code.
Real-client tests find bad client talk.

| Layer | What it proves | Run |
| --- | --- | --- |
| Shared core | Crypto, message format, queue rules, MLS rules | `cargo test -p links-client-core --locked` |
| Client pair | Native desktop core and browser core can make encrypted chat, reply, send at same time, and survive browser reload | `cargo test -p links-desktop-client-ffi --locked` |
| Local stack | Username account make, login again, gateway connect, duplicate-device block, Alice to Bob, Bob to Alice | `./scripts/smoke-two-client.sh` |

Run all three with:

```sh
./scripts/test-client-matrix.sh all
```

The local-stack test needs the normal local account service and gateway already
running. It makes fresh test usernames every time. It does not print keys,
tokens, account IDs, or message bytes.

## Must-pass client stories

Every client needs these stories before release:

1. Make account. Close app. Login same device.
2. Add a recipient from directory data.
3. Send encrypted text to another client surface.
4. Receive text, ack it, reconnect, and do not show it twice.
5. Send both ways before either side receives.
6. Reject a revoked device and duplicate live session.

The Rust desktop FFI is the native client boundary used by Apple hosts. The
browser test uses the real WASM-facing `WebMessagingCore` on the other side.
This is the current native-to-web compatibility test.

## Client matrix

| Sender | Receiver | Current proof | Next proof |
| --- | --- | --- | --- |
| macOS/iOS native core | Web | Shared encrypted-chat test | Run same story through macOS and iOS app UI |
| Web | macOS/iOS native core | Shared encrypted-chat test | Run same story through browser and macOS app UI |
| Android | Web/native | Android identity storage tests | Add Android app test adapter to this same story |
| iOS | Android | Shared native core behavior | Add two-device simulator test adapter |

Do not copy message rules into each app test. Put message rules in
`client-core` or the desktop FFI/native-to-web compatibility tests. Platform
tests should only prove storage, network, lifecycle, and UI wiring.
