# MLS broadcast profile

Broadcast subscribers use `links_client_core::broadcast::BroadcastSubscriber`
around their normal MLS engine. This is a passive/read-only profile:

- `BroadcastWelcome` joins the subscriber to the broadcast group.
- `BroadcastCommit` advances the subscriber through publisher-supplied MLS
  epochs.
- Application messages are decrypted locally after MLS authentication.
- Group creation, publishing, direct-chat control updates, membership changes,
  and local pending-commit merges fail closed.

`receive_broadcast_available()` requires a transport implementing
`BroadcastReceiveTransport`. It rejects ordinary direct or group MLS updates,
applies broadcast updates before opening encrypted envelopes, durably commits
the mailbox cursor before QueueAck, and returns no outbound delivery receipts;
the subscriber remains receive-only.

Read-only is an application policy around MLS, not a wire-level MLS capability.
The publisher path must still authenticate admin devices and sign posts before
the later broadcast publishing milestone is released.
