# MLS broadcast profile

Broadcast subscribers use `links_client_core::broadcast::BroadcastSubscriber`
around their normal MLS engine. This is a passive/read-only profile:

- `BroadcastWelcome` joins the subscriber to the broadcast group.
- `BroadcastCommit` advances the subscriber through publisher-supplied MLS
  epochs.
- Application messages are decrypted locally after MLS authentication.
- Every post is a `BroadcastPost` content carrying a domain-separated Ed25519
  signature from the admin device. The signature binds the conversation, MLS
  sender device, post ID, epoch, admin public key, and payload.
- Group creation, publishing, direct-chat control updates, membership changes,
  and local pending-commit merges fail closed.

`receive_broadcast_available()` requires a transport implementing
`BroadcastReceiveTransport`. It rejects ordinary direct or group MLS updates,
applies broadcast updates before opening encrypted envelopes, durably commits
the mailbox cursor before QueueAck, and returns no outbound delivery receipts;
the subscriber remains receive-only.

Publishers call `publish_broadcast_post()` with their hardware-backed
`MlsIdentitySigner`, `BroadcastAdminVerifier`, `BroadcastMasterKey`, and
`BroadcastBroker`. The send side checks the current owner/admin role, signs the
serialized Message, encrypts it with an HKDF-derived per-conversation/epoch key,
and dispatches only a `BroadcastDispatch` ciphertext. `NatsBroadcastPublisher`
publishes that wrapper to a subject derived from a hash of the conversation ID;
the broker can route it without learning the post or admin identity.

The receive side first opens the master-key ciphertext, then checks that the
MLS-authenticated sender device equals the signed admin device, verifies the
Ed25519 signature, and asks `BroadcastAdminVerifier` for the current role.
Invalid, unsigned, revoked, or non-admin posts fail before storage or rendering.

Read-only is an application policy around MLS, not a wire-level MLS capability.
Master keys must be provisioned to authorized publisher/subscriber devices
through the existing secure device setup; they are never stored in the broker.

Subscribers implement `BroadcastDispatchTransport` and call
`receive_broadcast_dispatches()`. The client fetches a bounded batch for one
conversation, validates and decrypts each wrapper, verifies the Ed25519
signature plus current admin role, renders the message, and only then sends
the broker acknowledgement. Failed verification, decryption, rendering, or
acknowledgement leaves the dispatch unacknowledged for safe retry.
