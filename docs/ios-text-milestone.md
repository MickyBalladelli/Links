# iOS internal text milestone

Milestone ID: `ios-text-internal-1`  
Version: `0.1.0-internal`  
Scope: authenticated one-to-one text on iOS 16+.

This is an internal device milestone. It is not a public App Store release or
a public security claim. `IOSInternalTextMilestone` enables only one-to-one
text. Groups, media and calls have no enabled feature path.

## Included

- Secure Enclave-backed identity and signed phone OTP onboarding.
- Shared Rust core factory for identity signing, MLS, Sealed Sender and durable
  inbox/outbox providers.
- TLS `wss` connection using binary `links.v1`, Hello deadline, reconnect and
  heartbeat handling.
- One-to-one text send and receive with 64 KiB UTF-8 limit.
- Silent APNs wakeup parsing and bounded missing-message replay.
- Durable cursor, decrypt, local commit and QueueAck ordering through the
  shared core.

The Swift package is the iOS client artifact today. An Xcode app target must
embed `LinksClient`, provide the production `SharedClientCoreFactory`, set the
internal account/gateway endpoint, and sign the IPA before device distribution.

## Physical-device acceptance gate

Before sharing the internal IPA, two physical iOS devices must pass:

1. Enroll identities and verify Secure Enclave identity persistence after app
   restart and device lock/unlock.
2. Complete OTP onboarding; verify device and public-key challenge bindings.
3. Exchange text in both directions with Android and iOS; verify one render,
   ordered sender-local sequences and delivery receipts.
4. Force-quit one app, send while offline, deliver a silent APNs wakeup, and
   verify replay renders the message once.
5. Drop and restore network; verify reconnect resumes from the durable cursor
   without acknowledging uncommitted messages.
6. Tamper with an envelope or cursor; verify fail-closed behavior and no cursor
   advance.
7. Record iOS versions, device models, Secure Enclave result, APNs delivery,
   battery impact and the exact milestone identifier.

No group, media or call surface may be present in the internal build. The
physical-device gate and the host core provider are release evidence, not claims
made by this Swift package alone.
