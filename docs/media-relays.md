# Open media relays

Links supports two relay admission modes for WebRTC calls:

- `open`: a healthy node accepts a call without payment or account metadata;
- `token-incentivized`: the call service issues a short-lived credit token bound
  to one relay node and one opaque call session.

Both modes use the same signed `MediaRelayRecord`. The record contains only the
relay endpoint, region, SFrame capability, expiry, sequence, and pricing. It is
published under a domain-separated region key through `SfuDhtClient`. The
trusted node directory pins the Ed25519 key, and the health cache rejects stale
or unhealthy nodes. The bounded lookup returns at most 16 current records.

## Routing and admission

`MediaRelayRouter::select` first prefers the highest-sequence healthy open node.
If only token relays are available, it checks the authority signature, relay
node binding, session binding, expiry, byte limit, duration limit, and rounded
minute credit before returning a route. A token is never published into the
DHT. The selected route always carries
`SfuMediaPolicy::encrypted_sframe()`.

The relay receives ordinary RTP routing headers and encrypted SFrame frames. It
does not receive MLS state, SFrame keys, conversation IDs, account IDs, or
plaintext media. A relay signs a `MediaRelayUsageReceipt` after forwarding
work; the receipt proves opaque token/session usage for settlement without
adding user identity to the relay protocol.

The repository provides `MemoryMediaRelayDht` for bounded local development.
Production nodes must implement the existing `SfuDhtClient` boundary with an
authenticated Kademlia or equivalent directory adapter, and must run the
forwarding/QUIC/WebRTC adapter separately from this routing contract.

Deployment settings and secret names are in
`deploy/media-relays/regions.example.yaml`. Node signing keys, token-authority
keys, DHT identity keys, TLS credentials, and settlement credentials stay in
the secret manager. Provisioning relay infrastructure or a token economy is
an operator action; this repository supplies the signed protocol and routing
boundary.
