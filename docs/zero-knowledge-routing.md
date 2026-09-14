# Zero-knowledge routing stance

Status: verified at the implementation-contract level. Links uses “zero-
knowledge routing” as an engineering term for routing nodes not receiving
message/media plaintext or decryption keys. It is not a claim of formal
zero-knowledge proofs, traffic-analysis resistance, or an anonymous network.

## Verification matrix

| Deployment | Routing input | What the router can see | What it cannot see |
| --- | --- | --- | --- |
| Centralized WebSocket gateway | `links.v1.Envelope` | Recipient device ID, envelope ID, expiry, ciphertext size/timing, connection/session metadata | Sender identity in the outer envelope, conversation ID, message text, media keys, MLS state, Sealed Sender contents |
| Centralized regional queue | Serialized envelope / `FederatedEnvelopeBatch` | Destination node/gateway, batch ID, expiry, envelope IDs, ciphertext size/timing | Plaintext, sender identity, conversation ID, MLS/SFrame keys, media |
| APNs / FCM wakeup | Device ID and cursor | Device target and wakeup timing | Ciphertext, sender, conversation, message, access token |
| Content-addressed storage | CID and encrypted chunk | CID, ciphertext size/timing, provider/IP metadata | Plaintext, attachment key, private media metadata |
| Decentralized relay DHT | Signed relay capability record | Public endpoint, region, health/sequence, SFrame capability, pricing | Accounts, rooms, calls, participants, tokens, MLS/SFrame keys, media |
| Decentralized media relay | RTP headers and SFrame frames; optional node-bound access token | Routing headers, opaque session/token identifiers, traffic size/timing | Encoded media plaintext, SFrame keys, MLS state, account/conversation identity |

## Code evidence

- `proto/links/v1/envelope.proto` restricts the routing-visible envelope to
  protocol version, envelope ID, recipient device ID, expiry, and
  `sealed_payload`.
- `crates/client-core/src/envelopes.rs` creates the MLS ciphertext and Sealed
  Sender wrapper before the envelope reaches a transport. `crypto.rs` has the
  only normal open path and it requires the local recipient key.
- `crates/gateway/src/lib.rs` validates, stores, routes, and replays envelopes;
  it has no message/MLS decrypt operation. Push payloads contain only device
  ID and cursor.
- `crates/queue/src/lib.rs` verifies envelope bounds, expiry, digest, and node
  signatures, then forwards the exact serialized bytes. Federation relay code
  does not decrypt `sealed_payload`.
- `crates/server-store/src/payload.rs` and `blob.rs` expose ciphertext storage
  boundaries only. Content-addressed storage verifies the CID over ciphertext.
- `crates/gateway/src/sfu.rs` requires RTP-header routing, rejects media
  decryption, and requires SFrame. `media_relay.rs` applies that policy to
  open and token-incentivized routes.
- `crates/client-core/src/decentralized.rs` retries exact opaque envelopes,
  verifies CID chunks before upload/use, and accepts only fresh trusted
  SFrame relay records.

## Limits and residual metadata

This stance does not hide the recipient device from the delivery service. It
also does not hide connection IPs, endpoint choice, packet sizes, timing,
availability, account-directory lookups, or the existence of a relay session.
An authenticated gateway can correlate traffic and a compromised endpoint can
read content after local decryption. Logs, metrics, provider backups, and
failure diagnostics must follow the same no-plaintext rule.

The centralized deployment remains the default. The federated deployment adds
authenticated node trust and opaque envelope propagation. The decentralized
media/storage clients add provider failover but do not bypass MLS, Sealed
Sender, AEAD, CID, or SFrame checks. No route may fall back to plaintext media
or server-side decryption.

## Release evidence

Before release, operators must supplement this static verification with:

1. red-team tests that inspect gateway, queue, DHT, relay, provider, and push
   logs for message/media/key leakage;
2. packet captures proving transport encryption and SFrame payload opacity;
3. provider-access review for backups, traces, crash dumps, and metrics; and
4. the independent cryptographic audit in `docs/crypto-audit-scope.md`.

This document verifies the current code boundary. It does not replace those
operational checks or the third-party audit.
