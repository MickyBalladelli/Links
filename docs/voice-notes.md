# Voice notes

`links-client-core::voice` defines the v1 voice-note profile. It accepts Opus
at 16–24 kbps, 20 ms frames, 8/12/16/24/48 kHz input and mono or stereo
channels. The current native encoder uses libopus in voice mode with
constrained VBR at the selected target and DTX for quiet frames.

Both `.ogg` and `.opus` use the Ogg Opus page format. The profile maps them to
`audio/ogg` and `audio/ogg; codecs=opus` respectively. `OggOpusWriter` emits `OpusHead`,
`OpusTags`, audio packets, monotonic granule positions and per-page Ogg CRCs.
`validate_ogg_opus` checks page framing, stream serial/sequence continuity,
header packets, packet bounds, CRCs, EOS and at least one audio packet before
the bytes are accepted.

`NativeOpusEncoder` takes exactly one configured PCM frame at a time, encodes
it with libopus, and sends the packet to the Ogg writer. The native dependency
is bundled through the Rust `opus` binding. WASM exposes the profile and Ogg
writer but does not compile native libopus; a browser host may provide Opus
packets from WebCodecs or another reviewed browser codec and pass them to the
same writer.

`encode_ogg_opus` is the native host helper for a complete PCM recording. It
pads only a partial final 20 ms frame; the private duration metadata still
records the unpadded capture duration.

The resulting container is still media plaintext at the codec boundary. The
host must encrypt the complete container as an attachment using the existing
E2EE provider. `OpusAudioMetadata` is placed inside the encrypted `Message`
and carries bitrate, sample rate, channel count, frame duration and the
container marker. Servers see only the existing opaque encrypted attachment
and generic `MediaMetadata`; upload/download and mobile recording/playback are
handled by the mobile voice-note adapters below.

## Mobile pipeline

`links-client-core::attachments` creates a fresh 256-bit content key and
12-byte nonce for every note. It encrypts the complete Ogg Opus container with
ChaCha20-Poly1305, binds the attachment ID as authenticated data, records the
ciphertext size and SHA-256 digest, and validates the decrypted Opus stream.
The key, nonce, digest, and Opus profile are private `MediaMetadata`; upload
services receive only the attachment ID and ciphertext.

Android uses `AndroidVoiceNotes` with API 29+ `MediaRecorder` Ogg/Opus capture
and a cache-scoped `MediaPlayer`. iOS uses `IOSVoiceNoteRecorder` for temporary
16-bit PCM capture, delegates Opus encode/decode and attachment crypto to the
shared core, and plays verified PCM through `AVAudioEngine`. Both hosts require
an exact upload receipt before sending the private media Message. Downloaded
bytes are checked, decrypted, and validated before any playback object is
created; temporary plaintext files/buffers are removed after use.

The shared send coordinator exposes `send_voice_note`: the host uploads the
opaque ciphertext, verifies the returned attachment ID/size/digest receipt,
then calls the normal MLS send path with only private `MediaMetadata`.
