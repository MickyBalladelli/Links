# Channel, business, and bot client surfaces

The three surfaces reuse the existing encrypted transport, MLS state, cursor
replay, and delivery-receipt paths. A surface profile only selects the account
mode, route, and UI capabilities.

## Profiles

| Surface | Allowed roles | Send behavior | Publish behavior |
| --- | --- | --- | --- |
| Channel | owner, admin, subscriber | Subscribers are read-only | Owner/admin posts use the broadcast MLS publisher |
| Business | owner, admin, member | Members can reply to business conversations | Owner/admin manage business posts and inbox policy |
| Bot | bot | Bot identity can send and receive | Bot automation uses the bot policy, not admin broadcast publishing |

The shared profile validates a canonical non-nil UUID, a display name of at
most 80 UTF-8 bytes, a valid role for the surface, and text of at most 64 KiB.
It contains no private key or bearer token.

## Platform adapters

- Android: `AndroidChannelBusinessBot` composes `ConnectionManager`,
  `ClientSession`, and `AndroidTextMessaging.CoreBridge`.
- iOS: `IOSChannelBusinessBotClient` composes `IOSConnectionManager`,
  `IOSClient`, and `SharedClientCore`.
- Web: `WebChannelBusinessBot` composes `WebConnectionManager` and
  `WebMessagingCore`.
- Desktop: `DesktopSurfaceClient` composes `DesktopTextSession` and the
  `DesktopMessagingCore` surface hook.

The default surface send hook routes through the existing encrypted text
coordinator. A channel-capable core overrides that hook to call the existing
signed broadcast publish flow. Receive callbacks remain post-commit, so the
host renders only messages whose MLS state and cursor are durable.
