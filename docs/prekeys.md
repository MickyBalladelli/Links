# Pre-key provisioning

`links-client-core::prekeys` builds the public material needed to start PQXDH
while the recipient is offline. Private X25519 and ML-KEM-768 seeds stay behind
the `PreKeySecretStore` boundary and must be wrapped by the native hardware
vault before generation succeeds.

Each profile contains the enrolled Ed25519 public key, a separately generated
X25519 identity key, a signed X25519 prekey, and a signed last-resort ML-KEM key.
A refill upload adds one-time X25519 and signed one-time ML-KEM keys. Profile
revision numbers only move forward; changing public profile material without a
higher revision is rejected.

## Automatic refill

Call `maintain_inventory` after authenticated login and when the server reports
low inventory. The default low watermark is 20 keys and the target is 100 keys
for each one-time pool. A native host implements:

- `PreKeySigner` with the enrolled hardware-backed Ed25519 identity
- `PreKeySecretStore` with Secure Enclave/Keychain or Android Keystore wrapping
- `PendingPreKeyUploadStore` with durable, device-private storage
- `PreKeyApi` with the authenticated protobuf HTTP endpoints below

The worker saves the exact upload and random `upload_id` before network
transmission. The server keeps its digest for the active profile revision. On
restart the client retries that upload first, making a lost HTTP response safe
without recreating a key that another device already claimed. Run one refill
worker per device. A failed generation can leave hardware-wrapped orphan keys if
the process dies before the pending record is saved; native storage maintenance
should remove records not referenced by the active profile or pending upload.

## HTTP API

All routes require `Authorization: Bearer <device session>`, return
`Cache-Control: no-store`, and use protobuf response bodies.

| Method and route | Body | Result |
| --- | --- | --- |
| `GET /v1/prekeys/status` | none | `PreKeyInventory` for the authenticated device |
| `PUT /v1/prekeys` | `PreKeyUpload`, `application/x-protobuf` | Idempotent upload and resulting inventory |
| `POST /v1/prekeys/{device_id}/claim` | none | `PreKeyBundle` for offline initiation |

Uploads are limited to 256 KiB and 100 keys per pool. The service verifies every
identity, signed X25519, and ML-KEM signature and binds the signing key to the
authenticated enrolled device. PostgreSQL stores only public key material.

A claim atomically removes at most one X25519 and one ML-KEM one-time key. When
the ML-KEM pool is empty, the bundle carries the signed last-resort ML-KEM key.
When the X25519 pool is empty, that optional key is absent. Claimers must verify
the bundle with the expected enrolled Ed25519 key through `claimed_bundle`
before using it. Authentication limits anonymous draining; broader abuse limits
remain part of the anti-spam phase.

No endpoint uploads or returns private key bytes. Revoked devices and disabled
accounts cannot upload, inspect inventory, or be claimed.
