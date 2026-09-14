# WebRTC direct file transfer

Large attachments may use an already-authenticated, ordered and reliable
WebRTC DataChannel. Signaling, peer authentication, MLS metadata, and the
attachment content key stay outside this channel. The channel carries only
the encrypted attachment bytes, so a relay or signaling service cannot read
the file.

The shared `links-client-core::p2p_transfer` module defines the byte-compatible
`LDT1` frame contract used by native hosts and the browser adapter:

- A `Begin` frame carries transfer/attachment IDs, the `u64` ciphertext size,
  a fixed 256 KiB chunk size, and the expected ciphertext SHA-256.
- The receiver returns a durable resume offset. Each `Chunk` contains an
  absolute offset, its bytes, and a per-chunk SHA-256. Chunks must be ordered;
  out-of-order or duplicate chunks are rejected.
- The receiver acknowledges the next durable offset after its sink writes the
  ciphertext. `Finish` is accepted only when the complete ciphertext hash
  matches the MLS-provided manifest; `Complete` is sent after finalization.

`web/src/WebRtcFileTransfer.ts` applies backpressure at 4 MiB of buffered
DataChannel data and reads `Blob` sources in bounded slices. Its sink contract
supports a persisted temporary file, resume after reconnect, streaming hash,
and atomic finalization. No server upload-size limit is applied to this path;
the `u64` manifest and chunked channel are the limits instead.

`WebRtcSession` provides the separate authenticated signaling concern: it
exchanges SDP offers/answers and ICE candidates over `links.v1` before handing
the resulting peer connection to a file-transfer host. Hosts must not pass an
unordered, lossy, or unauthenticated DataChannel to the file transfer adapter.
