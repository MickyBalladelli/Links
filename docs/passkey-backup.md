# WebAuthn / Passkey encrypted key backup

The backup flow keeps the identity seed on the device. A passkey is registered
with the account service, then the client asks the platform WebAuthn API for a
PRF (`hmac-secret`) result using a fresh 32-byte salt. The PRF result never goes
to Links. It can also be used once, locally, as the domain-separated root for a
new self-sovereign Ed25519 identity before username registration; the platform
wrapper immediately seals that identity in its hardware vault.

`links-client-core::HardwareIdentityStore::backup_with_passkey` loads the seed
from the native hardware vault for one call, derives an encryption key from the
32-byte PRF result, and seals the seed with ChaCha20-Poly1305. The authenticated
envelope binds:

- protocol version and identity-seed kind;
- backup ID and source device ID;
- passkey credential ID;
- the public PRF salt.

The client uploads only this opaque envelope. The server also receives the
credential ID and IDs needed for lookup, but never receives the PRF output,
encryption key, seed, mnemonic or plaintext key state. Replacing any envelope
metadata, ciphertext or passkey-derived output makes local decryption fail.

## Server ceremony

Configure `PasskeyConfig` with the exact WebAuthn relying-party ID and origin,
then construct `AccountAuth::new_with_passkey`. The HTTP routes are:

| Route | Purpose |
| --- | --- |
| `POST /v1/passkeys/register/start` | Create a short-lived registration challenge. |
| `POST /v1/passkeys/register/finish` | Verify `webauthn.create`, UV, RP hash and an ES256 credential from `fmt=none` attestation. |
| `POST /v1/passkeys/assert/start` | Create a short-lived assertion challenge. |
| `POST /v1/passkeys/assert/finish` | Verify `webauthn.get`, UV, RP hash, ES256 signature and the authenticator counter. |
| `PUT /v1/passkey-backups` | Store an authenticated opaque envelope for the current device. |
| `GET /v1/passkey-backups/{backup_id}` | Return the opaque envelope to an authenticated account session. |

All routes use the existing bearer session and `no-store` response policy. The
backup upload is bound to the authenticated device and to a registered passkey
credential. Reusing a device slot with different backup metadata or ciphertext
is rejected. Expired challenge rows are purged only after their replay-retention
window. The database migration is `0005_passkey_backup.sql`.

The current verifier intentionally accepts only ES256 (`-7`) P-256 credentials
and `fmt=none` attestation. This is a clear interoperability/security boundary:
platforms must request no attestation, and adding packed, Android Key or Apple
attestation requires implementing and reviewing those attestation chains first.

Passkey restore is explicit. The client fetches the envelope, evaluates the
same credential's PRF with the stored salt, calls
`HardwareIdentityStore::restore_from_passkey`, then performs the separate
authenticated device enrollment and regenerated PQXDH pre-key upload. No login
retry or missing-key path may call restore automatically.

## Android client flow

`native/android/client` exposes the same boundary through `AccountRecovery`:

1. `ClientSession.restoreFromRecovery` accepts an English BIP-39 phrase and
   optional passphrase. JNI derives the identity in Rust and immediately seals
   it with `HardwareSeedVault`; the phrase is not sent to Links.
2. `AccountRecovery.registerPasskey` and `backupWithPasskey` run the authenticated
   WebAuthn ceremony, request user verification plus a 32-byte PRF evaluation,
   then upload only the encrypted envelope through `PasskeyClient`.
3. `restoreFromPasskey` downloads the opaque envelope, reads its public PRF salt,
   asks the platform passkey provider for the matching PRF result, and sends the
   result only to the local JNI restore call. A fresh device ID and MLS node ID
   are assigned after restore.

For first-device self-sovereign creation, Android also exposes
`generateRecoveryMnemonic` and `createFromPasskeyPrf`. Apple and WASM expose
matching local constructors. These return only a public key plus an opaque
hardware reference; the phrase and PRF output never enter the server API.

The `PasskeyProvider` interface is the Android Credential Manager integration
boundary. Its implementation must return the raw WebAuthn response fields and
the 32-byte PRF result without logging or persisting them. The current account
service requires a bearer session for passkey endpoints, so a separate
authenticated bootstrap session is required before cloud restore; seed phrase
restore remains fully local.
