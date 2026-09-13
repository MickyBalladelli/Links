# Android internal text milestone

Milestone ID: `android-text-internal-1`  
Package version: `0.2.0-internal`  
Scope: authenticated one-to-one text on Android only.

This is an internal device build. It is not a public messenger release and it
does not enable groups, media, calls, or a public security claim.

## Included

- Hardware-backed Android identity and phone OTP enrollment.
- Authenticated TLS WebSocket with reconnect, ping heartbeats and binary-only
  `links.v1` frames.
- Shared Rust one-to-one send/receive coordinators using direct two-user MLS,
  Sealed Sender envelopes, durable cursors and delivery receipts.
- FCM data-only wakeup with constrained WorkManager replay and deleted-message
  full-sync recovery.
- Explicit BIP-39 and passkey identity recovery boundaries.
- `AndroidTextMessaging`, a text-only connection shell. The injected
  `CoreBridge` must bind the shared Rust send/receive providers and durable
  Android storage; it has no group or call API.

## Build

Set `AUTH_BASE_URL` to the internal HTTPS account/gateway endpoint, then build:

```sh
gradle -p native/android :client:assembleInternal
```

The internal application ID is `ai.links.app.internal`. Do not distribute this
variant through a public store. Release artifacts must come from a signed,
reviewed build environment; local debug signing is for the internal device gate
only.

## Two-device acceptance gate

Before calling this milestone accepted, two physical Android devices must pass
the following manual flow with message contents never visible in server logs:

1. Enroll both devices and verify their hardware identities survive app restart.
2. Complete authenticated device/pre-key enrollment for both accounts.
3. Send text in both directions while connected; verify one render per message,
   ordered sender-local sequences and a delivery receipt.
4. Force-stop each app, send while the peer is offline, then reopen and verify
   FCM/WorkManager replay renders the missing message once.
5. Drop and restore the network; verify reconnect resumes from the durable cursor
   and does not acknowledge an uncommitted message.
6. Tamper with one envelope or cursor and verify the client fails closed without
   advancing the durable checkpoint.
7. Record Android versions, device models, TEE/StrongBox result, battery impact,
   and the exact internal build ID.

The Rust core FFI check passes in the workspace. The physical-device gate and
the host `CoreBridge` provider acceptance remain release evidence, not claims
made by this source tree alone.
