# Independent store-and-forward relays

Links fans each signed opaque federation batch into up to three independent
NATS JetStream clusters. A relay keeps the exact bytes until the destination
node acknowledges successful local handoff or the batch expires. The
destination deduplicates copies from other clusters by batch and envelope ID.

## Configure

1. Provision three separate NATS/JetStream clusters in independent regions or
   failure domains. Do not make them one stretched consensus cluster.
2. Apply the settings in [`regions.example.yaml`](regions.example.yaml):
   explicit acknowledgements, three storage replicas per cluster, 30-day
   maximum age, encrypted-at-rest storage, and the 512 KiB message ceiling.
3. Put the endpoint, CA, client certificate, and client key values in the
   deployment secret manager. Never commit private keys or NATS credentials.
4. Create one `NatsPublisher` per cluster and pass them to
   `links_queue::IndependentRelayPool`. Use the same source node signer and
   batch ID across all clusters.
5. Run a consumer for each destination-node subject. Verify the source node
   key, call `decode_and_claim_relay_for_node`, route the returned envelopes to
   the local gateway, then acknowledge the JetStream message only after local
   handoff or durable mailbox acceptance.

## Failure and retention rules

- Retry publish with the returned batch ID. Copies already accepted are safe
  to replay.
- Use bounded exponential retry and stop at batch expiry. Never extend expiry
  during retry or relay handoff.
- Persist relay claims when consumers can move between workers. The in-memory
  `RelayDeduplicator` is only a reference adapter; the durable mailbox's
  envelope-id idempotency remains the final duplicate guard.
- Purge expired batches and tombstones on a schedule. Do not log envelope
  bytes, `sealed_payload`, sender metadata, conversation IDs, or keys.
- Alert on relay lag, redelivery, claim conflicts, expiry drops, and cluster
  quorum loss without including raw payloads.

The repository supplies the signed batch protocol, pool, and deployment
contract. Operators still provision the independent clusters, storage disks,
TLS trust, secrets, backup policy, and regional health checks.
