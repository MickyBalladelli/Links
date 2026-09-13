# Distributed message queue

Links uses NATS JetStream as the first cross-region queue target. The queue
routes an opaque `GatewayDelivery` wrapper to the gateway that currently owns a
recipient socket. It never decrypts or interprets the Sealed Sender payload.

## Wire contract

`links-queue` serializes these fields:

- protocol version
- source gateway ID
- destination gateway ID
- durable mailbox cursor
- exact serialized `Envelope` bytes

The queue boundary validates protobuf size, gateway locators, cursor bounds and
the outer envelope shape. It does not inspect sender identity, conversation
metadata or sealed payload contents. Queue messages are at-least-once; the
mailbox envelope ID remains the idempotency key.

## NATS layout

Publish to one exact subject per gateway. Dots and colons in the deployment ID
are escaped as `~2E` and `~3A` so NATS subject tokens cannot be confused:

```text
links.v1.gateway.<subject_gateway_id>.deliver
```

Use a JetStream stream such as `LINKS_GATEWAY_DELIVERIES` with subjects
`links.v1.gateway.*.deliver`, encrypted storage, replicas across failure
domains, and a durable consumer per gateway deployment. The consumer checks its
destination ID, calls `Gateway::handle_forwarded`, then acknowledges only after
local socket handoff or a durable replay path is ready. A failed handoff gets a
negative acknowledgement with bounded retry and backoff.

The origin gateway appends to the encrypted mailbox first and publishes only
after that commit. A publish outage therefore leaves the message replayable;
the client can still retrieve it after reconnect. The destination does not
append a second mailbox row.

## Security and operations

Use TLS 1.3 with gateway-to-NATS mTLS, short-lived credentials from a secret
manager, and account permissions scoped to the gateway's own delivery subject.
Do not grant application workers broad publish/subscribe wildcards. Do not put
bearer tokens, phone numbers, conversation IDs, plaintext, or ciphertext in
subjects, logs, metrics, push payloads, or dead-letter diagnostics.

Set bounded message size and consumer limits below the WebSocket frame ceiling,
retain messages long enough for regional recovery, and alert on redelivery,
consumer lag, publish failures, and malformed envelopes without logging raw
payloads. Drain consumers before gateway deployment and preserve JetStream
messages during socket churn.

`links-queue::NatsPublisher` is the provider boundary. The repository does not
contain cloud credentials or a live NATS client, so cluster creation, mTLS
provisioning, JetStream policy, and the concrete async consumer are release
work rather than local build steps.
