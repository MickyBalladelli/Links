# Federated relay gossip

Links uses relay gossip between the selected Matrix-style home nodes. This is
server-to-server propagation, not a client Libp2p mesh. A source node sends a
separate signed `FederatedEnvelopeBatch` to each destination peer through its
authenticated NATS/JetStream adapter.

## Batch contract

`links.v1.FederatedEnvelopeBatch` contains:

- source and destination node IDs;
- a UUID batch ID and expiry timestamp;
- serialized `links.v1.Envelope` values;
- a SHA-256 digest over the exact ordered envelope bytes;
- an Ed25519 signature over a domain-separated transcript containing routing,
  batch, expiry, and body digest fields.

The batch is bounded to 100 envelopes and 512 KiB, and each envelope must be
valid, unexpired, unique within the batch, and expire no later than the batch.
Relay workers do not decrypt or inspect `sealed_payload`.

## Propagation and replay

`NatsFederationRelay` publishes to the exact
`links.v1.federation.<destination>.relay` subject. Fan-out is capped at eight
peers. If a publish partially succeeds, the caller retries with the same batch
ID; the receiver's `RelayDeduplicator` claims the batch and its envelope IDs
atomically, so an accepted envelope is never routed twice.

Receivers call `decode_and_claim_relay_for_node` with the authenticated source
node public key, expected source and destination IDs, and current time. The
adapter verifies size, expiry, digest, signature, and destination before the
local gateway routes each envelope. Production deployments must persist the
claim set when more than one relay consumer can receive the same stream.

Node public keys and mTLS identities remain separate from user/device DIDs.
Peer allowlisting, TLS, rate limits, retry backoff, and durable stream retention
belong to the deployment adapter.
