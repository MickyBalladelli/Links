# Platform call surfaces

Links ships calls in this order: iOS and Android mobile, then Web, then
desktop. All platforms use the managed SFU placement contract from
`docs/sfu.md` and the end-to-end ordering from `docs/call-flow.md`.

## Shared security ordering

Every call surface follows this sequence:

1. The authenticated call service returns an opaque room name, regional `wss`
   endpoint, short-lived room token, and `requireSFrame: true`.
2. The client obtains the initial media epoch key from its MLS host and
   installs it in the platform media engine.
3. The client publishes that key through encrypted MLS application control.
   The SFU signaling adapter never receives it.
4. The media engine configures voice, video, or live-stream mode, attaches
   SFrame to senders and receivers, joins the room, and exchanges SDP/ICE.
5. Authenticated MLS epoch updates install a new SFrame key before media use;
   the previous key is retained only for the bounded transition window.
6. Provider failure or malformed signaling closes the media engine and leaves
   the room without exposing key material to UI callbacks.

## Mobile first

`native/apple/Sources/LinksClient/IOSCallSession.swift` provides the iOS
state machine. The host injects an `IOSCallMediaEngine` backed by its audited
WebRTC/LiveKit SDK, an `IOSCallSignaling` adapter, and an
`IOSCallMLSKeyProvider`. `IOSCallMode.liveStream` supports publisher,
subscriber, and platform-defined participant UIs without changing the
encryption boundary.

`native/android/client/src/main/java/ai/links/app/AndroidCallSession.java`
provides the equivalent Android state machine. The host injects a native
WebRTC/LiveKit media engine implementing SFrame, plus signaling and MLS
adapters. `CompletableFuture` keeps provider callbacks off the UI thread and
all temporary key copies are wiped after installation or publication.

The repository intentionally keeps vendor SDK bindings behind these interfaces.
This lets the release build select the audited iOS and Android WebRTC/SFU
binding while keeping MLS control and key custody in the shared client core.

## Web and desktop

`web/src/WebCallSurface.ts` adds `voice`, `video`, and `live-stream` modes and
publisher/subscriber policy over `WebRtcCallFlow`. The existing browser flow
still owns native SFrame transforms, MLS key callbacks, and WebRTC SDP/ICE.

`crates/desktop-client/src/call.rs` exposes `DesktopCallSession` with the same
state machine and typed `DesktopCallSignaling`, `DesktopCallMlsKeyProvider`,
and `DesktopCallMediaEngine` boundaries. A desktop event loop feeds provider
signals into `handle_signal()` and calls `mark_connected()` after the native
peer reports success.

## Release gates

Before enabling a client surface, the platform adapter must prove real native
SFrame encryption/decryption, MLS control-message authentication, provider
recording disabled, room-token expiry, reconnect behavior, and two-device
voice/video acceptance. Live-stream release additionally needs publisher and
subscriber authorization checks, bounded fan-out, and a review of stream
retention and moderation behavior.
