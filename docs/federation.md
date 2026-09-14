# Links federation protocol choice

Links selects federated server nodes with a Matrix-style home-server model.
This is the Phase 11 protocol layer. Links does not select a Nostr/XMTP-style
relay network or a Libp2p mesh for the first decentralized expansion.

## Why this layer

Federation reuses the current authenticated account, directory, WebSocket,
queue, cursor, and store-and-forward boundaries. It adds independent home
servers without forcing every mobile, Web, or desktop client to maintain a
peer-routing table or participate in gossip. The centralized deployment stays
the default and is not blocked by this expansion.

## Node contract

Each account has one home node. The home node remains the authority for that
account's active devices, pre-key bundles, handles, and account policy. A
federated gateway forwards only encrypted delivery material to another node:

- transport uses mutually authenticated TLS between allowlisted federation
  nodes, plus a signed request transcript bound to source node, destination
  node, batch ID, expiry, and body digest;
- the body contains serialized `links.v1.Envelope` values, exactly as the
  existing gateway queue does;
- destination nodes route by `recipient_device_id`, preserve envelope expiry,
  and apply idempotency by envelope ID;
- nodes may retain delivery cursors, batch IDs, routing metadata, and ciphertext
  envelopes, but never sender identity, conversation IDs, plaintext messages,
  MLS state, private keys, or Sealed Sender contents;
- failed delivery retries are bounded by expiry and backoff. A node never
  retries an expired envelope or accepts a duplicate batch as new delivery.

Federation is a server-to-server transport boundary, not a new encryption
protocol. Existing MLS, Sealed Sender, device queues, receipts, and client
replay rules remain unchanged. Cross-node directory and node-key discovery
must be authenticated before federation is enabled. The global directory now
provides deterministic W3C `did:key` bindings for device keys, but those user
DIDs do not authenticate a server node or replace node-level federation trust.

## Relay gossip implementation

Cross-node propagation is implemented by `NatsFederationRelay` and
`IndependentRelayPool` in `links-queue`. They sign one bounded
`FederatedEnvelopeBatch` per destination peer and can store the same batch in
multiple independent JetStream clusters through exact relay subjects. Consumers use
`decode_and_claim_relay_for_node` and `RelayDeduplicator` before passing the
opaque envelopes to the local gateway. See [relay gossip](relay-gossip.md) for
the wire contract, limits, retry behavior, and deployment boundary.

## Rollout boundary

The relay implementation is an adapter boundary. It must be deployed only
after the centralized gateway passes its existing encrypted-routing checks and
with durable claim storage when relay consumers are scaled horizontally. The
independent cluster deployment contract is in `deploy/relays/`. Client P2P
media and decentralized storage are not prerequisites for this federation
choice.
