# Encrypted video and large files

Video is normalized on the sending device before encryption. iOS uses the
hardware VideoToolbox path and Android uses hardware MediaCodec, producing the
shared H.264/HEVC MP4 profiles with `faststart`. Arbitrary files skip video
normalization but use the same encrypted transfer path.

## Chunked attachment format

The shared Rust core uses ChaCha20-Poly1305 with a fresh 32-byte content key
and 12-byte base nonce per attachment. Plaintext is read in chunks of at most
262,128 bytes. Each ciphertext chunk adds one 16-byte authentication tag, so a
wire chunk is at most 256 KiB.

For chunk index `i`, encoded as an unsigned big-endian `u64`:

```text
chunk_nonce = base_nonce
chunk_nonce[4..12] ^= i.to_be_bytes()
AAD = "links/large-file/attachment/v1\0"
      || attachment_id.UTF8
      || i.to_be_bytes()
```

`MediaMetadata.original_size_bytes` and
`MediaMetadata.encryption_chunk_bytes` are private fields inside the MLS
message. The key, nonce, ciphertext size, and whole-ciphertext SHA-256 digest
are private there too. The server and CDN see only the opaque attachment ID
and ciphertext bytes.

## Client flow

- `IOSLargeFileSession` encrypts a transcoded MP4 or file to a bounded local
  ciphertext staging file, verifies the upload receipt, then sends private
  metadata through the connected MLS core.
- `AndroidLargeFileSession` provides the same flow using
  `AndroidLargeFileTransfer` and disposable ciphertext staging files.
- `WebLargeFileSession` encrypts bounded `Blob` slices into a host-provided
  durable ciphertext sink. Its returned source can feed
  `WebRtcFileTransfer`; the host can use OPFS, IndexedDB, or another durable
  sink implementation.
- `DesktopTextSession` uses `LargeFileEncryptor` and
  `decrypt_large_file` with file streams, then applies the same upload receipt
  and private MLS send boundary.

Uploaders receive a ciphertext file/source and must return the exact
attachment ID, size, and SHA-256 digest. Any mismatch stops the send. Download
paths verify every AEAD chunk and the complete ciphertext digest before
publishing plaintext. Failed decryptions must not publish partial output.

Large files may use the authenticated, ordered and reliable WebRTC
`LDT1` DataChannel flow. That channel carries ciphertext only and supports
durable resume, per-chunk digests, backpressure, and final whole-file
verification. Signaling and the MLS metadata exchange remain outside the
channel.

## Content-addressed providers

`ContentAddressedLargeFileEncryptor` can upload each encrypted chunk as a
CIDv1 SHA-256 raw block. The receiver gets the ordered private CID list from
`MediaMetadata`, fetches each block through an IPFS, Arweave, or Filecoin
adapter, verifies the CID and complete ciphertext digest, then decrypts. The
canonical provider-neutral URI is `ipfs://CID`; provider transaction or deal
IDs never enter public message or routing metadata. See
[content-addressed storage](content-addressed-storage.md).
