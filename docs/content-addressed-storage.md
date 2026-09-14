# Content-addressed encrypted storage

Links encrypts attachment chunks on the client before calculating a CID. The
CID is CIDv1, raw codec, SHA-256 multihash, lowercase base32. Its canonical
retrieval form is `ipfs://CID`.

## Client flow

`ContentAddressedLargeFileEncryptor` wraps the existing streaming
ChaCha20-Poly1305 large-file encryptor. For every ciphertext chunk it returns
the bytes, chunk index, CID, and `ipfs://` URI. The caller uploads the bytes
immediately and keeps only the ordered CID references after upload. The final
private `MediaMetadata` carries those references; content keys, nonce, and the
whole-ciphertext digest remain inside the authenticated MLS message.

The receiver downloads every CID block, verifies each block against its CID,
checks expected chunk sizes and the complete ciphertext SHA-256, then decrypts
in order. No plaintext is released before all blocks pass verification.

## Provider boundary

`links-server-store::ContentAddressedStore` validates the CID and ciphertext
before upload or after download. `ContentAddressedObjectClient` is the adapter
boundary for:

- IPFS pinning and gateway retrieval;
- Arweave immutable transaction storage with a CID-to-transaction index;
- Filecoin-backed block storage or IPFS/Filecoin retrieval gateways.

Each adapter must make `put_if_absent` idempotent, map its native identifier to
the supplied CID, use TLS and bounded timeouts, and never log ciphertext,
content keys, or private media metadata. Arweave transaction IDs and Filecoin
deal IDs are provider metadata, not replacements for the CID in the encrypted
Links message.

## Retention and failure

Blocks are immutable and addressed by ciphertext. Providers may pin or replicate
them across independent regions, but must apply the attachment retention policy
and remove expired pins/deals. Missing or corrupted blocks fail the download;
callers retry from another provider or relay and never render partial plaintext.

The implementation and deployment boundary are in
`crates/client-core/src/content_addressed.rs`,
`crates/server-store/src/content_addressed.rs`, and
`deploy/content-addressed/`.
