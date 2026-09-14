# Links MLS and TreeKEM core

`crates/client-core/src/mls.rs` wraps OpenMLS 0.9 for the RFC 9420 group
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
Additional physical clients register through the authenticated device endpoint;
the existing device authorizes the account scope and the new device signs a
nonce-bound pairing transcript. The resulting credential is a distinct MLS node
under the same user account. `client-core::pairing::approve_pairing` validates
this handoff before the new client initializes OpenMLS and publishes its first
KeyPackage. An existing MLS group still needs an authenticated
member-add commit before that node receives the group's traffic.

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

`add_group_members()` stages the same TreeKEM operation for many-to-many
groups. The core permits at most 100 distinct user identities and 100 physical
device leaves per group. Every new leaf must have a verified Links credential;
the core rejects duplicate devices, self-adds, untrusted credentials, and
over-limit commits. Group welcomes and commits use the `GroupWelcome` and
`GroupCommit` receive variants, so direct-chat updates retain their stricter
two-user validation.

## Direct-chat invariant

The client core treats one-to-one conversations as direct MLS groups. A direct
group is ready for application messages only when it has exactly two distinct
`user_id` values. A user may have several device leaves, so multi-device
pairing can produce more than two leaves without becoming a group chat.

The direct control path rejects a third user in add and commit processing,
rejects leaf updates that transfer a leaf to another user, and fails closed for
encryption or decryption while the group has fewer than two users. A direct
group may be temporarily inactive after a member leaves; it cannot send or
receive messages until it again has the two-user shape.

Many-to-many groups use the generic MLS control path and are ready when at
least two verified users remain, the local user is present, and the group stays
within the 100-user/100-device bounds. `send_group_message()` adds missing
verified device leaves, stages and delivers any pending TreeKEM commit, then
encrypts the application message once and fans out independently sealed
envelopes to every active group device. `receive_available()` applies group
welcomes/commits before decrypting those envelopes.

The host storage implementation must make OpenMLS state writes durable and
transaction-compatible with the application outbox. A process crash must not
forget a pending commit or advance the application state without its accepted
delivery record. `RustCryptoProvider<Storage>` supplies RustCrypto and leaves
this durable storage boundary explicit.

## Complexity and remaining gates

OpenMLS TreeKEM updates derive and publish only the changed path, giving
`O(log N)` member-update path work for a balanced tree. Message encryption and
decryption use the current MLS epoch and sender ratchets.

Still required before production use: a durable host storage adapter,
adversarial/interoperability vectors, signed physical-device acceptance, and an
independent cryptographic audit. The draft ciphersuite is an implementation
choice, not a production security claim.
