# DHT SFU discovery

Links uses an authenticated DHT control plane for decentralized SFU endpoint
discovery. The DHT stores public `SfuDiscoveryRecord` values under a
domain-separated hash of the region. It stores no room names, account IDs,
participants, tokens, MLS keys, SFrame keys, or media frames.

## Record and trust

Each record contains a node ID, region, `wss://` endpoint, optional TURN
endpoint, Ed25519 public key, monotonic sequence, short expiry, and an
`supports_sframe` flag. The node signs all fields. The caller accepts a record
only when:

- the node key matches the authenticated SFU trust directory;
- the signature and protobuf bounds pass;
- the record is newer than the last record for that node;
- the record has not expired and its TTL is at most ten minutes;
- SFrame capability is advertised; and
- an external health probe marked the node healthy within two minutes.

`DhtSfuDiscovery::discover_healthy` returns at most 16 verified records for one
region, ordered by sequence. The region key is explicit, so residency policy
does not silently fall back to another region.

## Adapter boundary

`SfuDhtClient` is the transport boundary. A production deployment can use
libp2p Kademlia or another authenticated DHT implementation. The repository
also includes `MemorySfuDht` as a bounded local reference adapter. DHT writes
must retain the highest sequence per node, and reads must enforce the result
limit.

The gateway still uses SFrame and opaque room names. DHT discovery only chooses
the public SFU endpoint; the trusted call service issues the short-lived room
token and MLS remains on the Links control channel.

Deployment settings and secret names are in
`deploy/sfu-dht/regions.example.yaml`. Node signing keys, DHT identity keys,
TLS credentials, and health-probe credentials stay in the secret manager.
