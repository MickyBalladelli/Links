# Multi-region connection gateway

`links-gateway` is the transport-neutral core for the public WebSocket edge. A
small adapter owns the actual `wss://.../v1/connect` socket and calls
`decode_client_frame`, `Gateway::open`, `Gateway::handle` and
`encode_server_frame`. `Gateway::open` returns `Welcome` followed by the
initial `SyncBatch` when `Hello.last_seen_cursor` has missing items. The core
may send the same batch as a smaller `CompressedSyncBatch` when the client
advertises `SYNC_COMPRESSION_ZSTD_DICTIONARY_V1`; the shared protocol/client
core owns dictionary validation and bounded decompression. The core never logs
or decrypts sealed message bytes.

## Regional flow

Each gateway instance has an explicit `GatewayConfig { gateway_id, region }`.
The gateway authenticates the first `Hello`, binds a random session lease in
the shared `EphemeralState`, and replaces any previous lease for that device.
The lease contains only device ID, session ID, gateway locator and a 120-second
expiry. Renew and close are compare-and-swap operations; an old socket cannot
clear or use a newer connection after a cross-region reconnect.

Sending follows this order for each envelope in a client-side fanout:

1. Validate the opaque envelope and append it to the durable encrypted mailbox.
2. Return `Accepted` only after the append/idempotency commit succeeds.
3. Deliver to the active local socket, forward to the gateway in the lease, or
   send a silent APNs/FCM wakeup when no socket is active.

Forward or push failure leaves the mailbox row available for replay. Retrying
the same envelope ID is idempotent. A client sends `Replay` after `Welcome` and
sends `QueueAck` only after its local message/MLS transaction is durable.

The sender obtains the target user's active device list from an authenticated
directory. `links-client-core` encrypts the MLS message once and wraps that
ciphertext separately for every device, with a distinct recipient-bound
envelope ID. The gateway receives those envelopes independently; it does not
need target-user metadata and never clones one device's sealed envelope into
another queue.

`RegionBus` is implemented for the first queue target by
`links-queue::NatsRegionBus`. Its `GatewayDelivery` wrapper preserves the
destination gateway ID, exact envelope bytes and mailbox cursor, uses a
durable NATS JetStream publish, and applies bounded backpressure. Do not
publish bearer tokens, phone numbers, conversation IDs or plaintext messages.
The destination consumer checks the subject and destination ID, then passes
the authenticated delivery to `Gateway::handle_forwarded`; it does not append
a second mailbox row. Kafka or RabbitMQ can implement the same `RegionBus`
contract if deployment needs change.

`PushNotifier` is the APNs/FCM boundary. `PushWakeup::apns_request` configures
the APNs `background` push type with priority `5` and an `aps.content-available`
body. `PushWakeup::fcm_request` configures a high-priority data-only FCM body;
it has no notification payload. Both carry only the recipient device ID and
mailbox cursor. They must not contain ciphertext, sender identity, conversation
metadata or access tokens. `conversation_id` and `sequence_id` are E2EE-private,
so the client wakes, reconnects over TLS and replays from its durable cursor.

## Production deployment

Run at least two gateway instances per region behind a TLS 1.3 load balancer.
Use a shared `links-server-store::redis::RedisEphemeralState` with server-time
Lua/CAS operations and the PostgreSQL `RelationalStore` encrypted payload store
shared by all regions.
Route
records must use the gateway's stable deployment ID, not a pod IP. Health checks
must remove a gateway from new connections before termination; existing sockets
receive a reconnect/close signal and their leases expire or are explicitly
unbound.

Required edge policy:

- WebSocket subprotocol is `links.v1`; reject text frames and WebSocket
  compression extensions. Application-level `CompressedSyncBatch` is allowed
  only after Hello negotiation.
- Require `Hello` within 5 seconds, heartbeat every 30 seconds, and close after
  90 seconds without liveness.
- Cap complete frames at 1 MiB before protobuf decode or allocation.
- Validate browser `Origin` against the configured allowlist.
- Apply per-device/account/IP connection and send limits in shared state.
- Keep gateway, bus, push-provider and queue credentials in a secret manager.
- Aggregate metrics without device IDs, account IDs, payload labels or raw IPs.
- Drain before deploy; never delete a queue row because a socket disconnected.

The repository provides the gateway core and provider interfaces. Concrete
Redis, bus, APNs and FCM adapters plus cloud load-balancer/IaC rollout are
deployment work and must pass regional failover, duplicate delivery, stale
lease, queue outage, push outage and reconnect acceptance checks before release.
