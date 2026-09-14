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
must be authenticated before federation is enabled; the later DID/key-registry
task remains separate.

## Rollout boundary

The first implementation gate is the node-to-node delivery adapter and its
replay/idempotency storage. It must be deployed only after the centralized
gateway passes its existing encrypted-routing checks. Client P2P media and
decentralized storage are not prerequisites for this federation choice.
