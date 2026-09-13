# Links MLS and TreeKEM core

`crates/client-core/src/mls.rs` now wraps OpenMLS 0.9 for the RFC 9420 group
ratchet. OpenMLS owns the ratchet tree, epoch secrets, message protection,
commit validation, and TreeKEM update path. Every physical device is one MLS
leaf, so a user with several devices has several leaves.

## Ciphersuite

The core pins:

`MLS_128_MLKEM768X25519_AES128GCM_SHA256_Ed25519`

This is OpenMLS's draft hybrid ciphersuite: ML-KEM-768 plus X25519 for the
TreeKEM HPKE path, AES-128-GCM and SHA-256 for message protection, and Ed25519
for authentication. It is not a final interoperable standard. Keep the
version pinned, negotiate it explicitly, and do not advertise post-quantum
security until the protocol composition has had independent review.

## Credential boundary

`DeviceBinding::mls_credential()` produces an RFC 9420 BasicCredential carrying
the user ID, device ID, MLS node ID, and Ed25519 public key. `OpenMlsEngine::new`
checks that the credential key equals the native hardware-backed signer.

Incoming credentials are accepted only after all of these checks:

- OpenMLS verifies the MLS signature and leaf structure.
- The credential is a Links BasicCredential with a valid device identity.
- The credential's public key equals the MLS leaf signature key.
- The caller's `MlsCredentialVerifier` confirms enrollment, account membership,
  and current revocation state.

Plaintext sender fields are never used as authentication.

## Durable commit flow

`generate_key_package()` creates offline-initiation material. OpenMLS stores the
private init and leaf-encryption keys through the configured storage provider.

`add_members()`, `remove_members()`, and `self_update()` stage a commit and
return `PendingCommit`. The host must durably retain the exact commit bytes and
deliver them before merging the pending state. Once the delivery service
accepts the commit, call `process_commit()` with the accepted/fanned-back bytes.
OpenMLS recognizes an own pending commit and merges it once; remote commits are
verified, checked for newly introduced credentials, and then merged.

## Direct-chat invariant

The client core treats one-to-one conversations as direct MLS groups. A direct
group is ready for application messages only when it has exactly two distinct
`user_id` values. A user may have several device leaves, so multi-device
pairing can produce more than two leaves without becoming a group chat.

The core rejects a third user in add and commit processing, rejects leaf updates
that transfer a leaf to another user, and fails closed for encryption or
decryption while the group has fewer than two users. A direct group may be
temporarily inactive after a member leaves; it cannot send or receive messages
until it again has the two-user shape.

The host storage implementation must make OpenMLS state writes durable and
transaction-compatible with the application outbox. A process crash must not
forget a pending commit or advance the application state without its accepted
delivery record. `RustCryptoProvider<Storage>` supplies RustCrypto and leaves
this durable storage boundary explicit.

## Complexity and remaining gates

OpenMLS TreeKEM updates derive and publish only the changed path, giving
`O(log N)` member-update path work for a balanced tree. Message encryption and
decryption use the current MLS epoch and sender ratchets.

Still required before production use: a durable host storage adapter, Sealed
Sender integration, adversarial/interoperability vectors, signed
physical-device acceptance, and an independent cryptographic audit. The draft
ciphersuite is an implementation choice, not a production security claim.
