# Links PQXDH profile

`links-client-core::pqxdh` implements the shared key-agreement primitive used
before MLS session setup. It follows the Signal PQXDH construction while using
the Links suite:

- X25519 identity, signed, ephemeral and optional one-time curve keys
- ML-KEM-768 from final FIPS 203
- HKDF-SHA-512 producing a 32-byte session secret
- separate hardware-backed Ed25519 account keys for prekey authentication

This is a Links profile, not byte-compatible Signal PQXDH. Signal's published
profile uses a Curve25519 identity key with XEdDSA. Links deliberately keeps the
Ed25519 signing key and X25519 DH identity key separate. The Ed25519 key signs an
identity-binding transcript, then signs the X25519 signed prekey and ML-KEM
prekey transcripts. This avoids reusing one private key for signing and DH.

## Key calculation

The initiator validates the authenticated account signing key and all prekey
signatures before doing key agreement. It calculates:

```text
DH1 = X25519(IK_A, SPK_B)
DH2 = X25519(EK_A, IK_B)
DH3 = X25519(EK_A, SPK_B)
DH4 = X25519(EK_A, OPK_B)     # only when an EC one-time prekey exists
SS  = ML-KEM-768-ENC(PQPK_B)
SK  = HKDF-SHA-512(0xff^32 || DH1 || DH2 || DH3 [|| DH4] || SS)
```

HKDF uses a 64-byte zero salt and the fixed info string
`LinksV1_X25519_SHA-512_ML-KEM-768`. EC and KEM keys use distinct encoding tags.
The associated data binds both X25519 identity keys and both authenticated
Ed25519 account keys.

The responder checks every selected prekey ID and repeats the same calculation.
All-zero/non-contributory X25519 exchanges fail closed. ML-KEM ciphertexts and
public keys require their exact FIPS 203 sizes and canonical decoding.

## API and secret custody

Private key objects do not implement `Clone` or `Debug`. Temporary DH, KEM and
HKDF material is zeroized where the dependencies and language runtime permit.
Seed generators return `Zeroizing` buffers so a native client can immediately
seal them in the hardware-backed vault. Persist only opaque vault handles and
public keys in application state.

Use `identity_binding_transcript`, `signed_prekey_transcript` and
`kem_prekey_transcript` as the exact inputs to `HardwareIdentityStore.sign`.
Never accept a self-consistent bundle alone: pass the expected Ed25519 account
key from authenticated directory/device metadata to `initiate` or `respond`.

`initiate` returns the public initial-message header plus the session secret and
associated data. `respond` returns the same secret and the IDs of selected
one-time keys. ML-KEM has implicit rejection, so the responder must first
authenticate the initial AEAD/MLS payload. Only then may it atomically consume
the reported one-time curve and ML-KEM keys. Failed payload authentication must
discard the derived secret without accepting a session.

Prekey persistence, protobuf wire encoding, atomic server claiming/upload and
rotation belong to the next prekey-bundle roadmap item. The application must not
reuse initiator ephemeral keys or consumed one-time prekeys.

## Security status

PQXDH protects the initial secret against passive harvest-now/decrypt-later
attacks when ML-KEM remains secure. Authentication is still classical and does
not resist an active quantum attacker. Replay handling and rapid replacement of
the initial secret remain responsibilities of the MLS integration.

The implementation uses RustCrypto `x25519-dalek` and `ml-kem`. The latter states
that it has not received an independent audit. Keep the roadmap's third-party
cryptographic audit open and do not advertise production post-quantum security
before interoperability vectors, adversarial review and that audit are complete.

Primary specifications:

- https://signal.org/docs/specifications/pqxdh/
- https://csrc.nist.gov/pubs/fips/203/final
- https://www.rfc-editor.org/rfc/rfc7748
- https://www.rfc-editor.org/rfc/rfc5869
