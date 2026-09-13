# Device-pairing QR payload

`links-client-core::pairing::PairingPayload` defines the shared text carried by
device-pairing QR codes. It is a transport format, not a new key or trust
system. The new device creates the payload; an already authenticated mobile
device scans it and approves registration.

## URI contract

The only accepted form is the canonical, versioned URI below:

```text
links://connect?v=1&user_id=<uuid>&device_id=<uuid>&mls_node_id=<uuid>&public_key=<base64url>&nonce=<base64url>&signature=<base64url>
```

The fields are fixed and ordered:

| Field | Size | Meaning |
| --- | ---: | --- |
| `v` | `1` | Pairing URI format version. |
| `user_id` | UUID | Account the new device intends to join. |
| `device_id` | UUID | New physical client identity. |
| `mls_node_id` | UUID | New MLS identity node. |
| `public_key` | 32 bytes | New device Ed25519 identity public key. |
| `nonce` | 32 bytes | Fresh pairing challenge from this attempt. |
| `signature` | 64 bytes | Ed25519 signature by `public_key`. |

Binary fields use base64url without padding. UUIDs must be lowercase canonical
hyphenated strings. The parser rejects unknown or duplicate fields, fragments,
percent-encoding, alternate UUID spellings, padded base64, and oversized URIs.

The signature covers the existing `links/device-pairing/v1` transcript:
`user_id`, `device_id`, `mls_node_id`, `public_key`, and `nonce`. This keeps the
QR flow compatible with `POST /v1/devices`; the server still authenticates the
approving device through its bearer session and checks the account/device
binding. A QR string is not trusted merely because it parsed.

## Client flow

1. The new client creates random `device_id`, `mls_node_id`, and a 32-byte nonce.
2. It signs `PairingPayload::signing_transcript()` with its hardware-backed
   identity key and builds `PairingPayload::new(...).to_uri()`.
3. A platform QR encoder renders that URI as a QR image. No secret or bearer
   token is placed in the code.
4. The authenticated mobile client decodes the QR image to URI text, calls
   `PairingPayload::from_uri()`, calls `verify()`, and confirms `user_id` before
   showing the approval UI.
5. The mobile client submits the decoded public fields to `POST /v1/devices`.
   The new device stores the returned MLS credential and becomes a distinct
   device node under the account.

`PairingPayload::signed_with_identity` is available for software-held test or
recovery seeds. Production mobile clients should sign the transcript through
the iOS Secure Enclave or Android Keystore adapter, then construct the payload
with the returned signature. QR image encoding/decoding stays in the platform
UI layer; all security-sensitive URI parsing and signature checks stay in the
shared core.

Discard a QR code after one approval attempt. A fresh nonce is required for
each retry. The current registration endpoint is idempotent for the exact
already-registered device binding, but does not make a copied QR code expire;
short-lived QR presentation and user confirmation remain client policy.
