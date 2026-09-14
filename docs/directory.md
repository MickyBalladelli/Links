# Global username key directory

The account service exposes the public device directory for `@username`
discovery:

```text
GET /v1/directory/@alice
```

The `@` is display syntax. The server canonicalizes the remaining path segment
with the shared lowercase ASCII handle validator. The JSON response is:

```json
{
  "handle": "alice",
  "user_id": "uuid",
  "devices": [
    {
      "device_id": "uuid",
      "mls_node_id": "uuid",
      "did": "did:key:z…",
      "identity_public_key": "base64url",
      "mls_credential": "base64url"
    }
  ]
}
```

The query uses the shared PostgreSQL control plane, so every region sees the
same handle claim, account disablement, and device revocation state. It does
not use an eventually consistent local cache. The service validates each
credential against the stored account/device/node/public-key binding before
serializing it. Empty or inconsistent records fail closed.

The `did` is the W3C `did:key` derived from the device's Ed25519 public key
using the Ed25519 multicodec and base58btc encoding. The handle maps to the
active device DIDs through this directory response; clients must verify that
each DID resolves to the accompanying `identity_public_key` before trusting
it. A DID does not replace the authenticated device enrollment or MLS
credential checks.

The endpoint is public because a sender must discover a recipient before an
authenticated session exists. It reveals only stable public directory data:
handle, user ID, device IDs, MLS node IDs, DIDs, identity public keys, and MLS
credentials. It never returns phone subjects, sessions, routing locators,
pre-key private material, or one-time pre-key inventory. Lookup responses are
`no-store` and rate-limited by both canonical handle and source address.

Directory lookup and pre-key claiming are separate operations. After finding a
handle, an authenticated sender claims one `PreKeyBundle` per active device
with `POST /v1/prekeys/{device_id}/claim`. The sender validates those bundles
against the directory credentials before PQXDH/MLS setup. Claiming is atomic
and consumes at most one one-time curve and one one-time ML-KEM pre-key; a
directory read never consumes keys.
