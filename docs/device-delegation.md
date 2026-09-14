# Signed device sub-certificates

An account root device can authorize another physical device with a public
`DeviceSubCertificate`. The certificate binds the account, issuer device and
MLS node, subject device and MLS node, subject Ed25519 key, role, and a short
validity window. The issuer signs a domain-separated transcript.

The shared client core builds certificates through
`delegation::issue_device_subcertificate()` using the hardware-backed
`MlsIdentitySigner`. The existing identity crate also provides the canonical
transcript and verification helpers.

Registration uses `POST /v1/devices/delegated` with:

- the base64url protobuf certificate
- a fresh nonce
- a signature by the new device over its pairing transcript

The bearer session must belong to the certificate issuer. Server checks enforce
that the issuer is active, the certificate is current and correctly bound, and
that the issuer key matches the registered device. Owner devices may delegate
device or admin leaves. Admin devices may delegate device leaves only. The
server stores the public certificate with the device and returns it through the
username directory.

Revoking the issuer or letting its certificate expire blocks further
delegations. Existing child devices still need the normal MLS remove flow when
an account policy revokes them.
