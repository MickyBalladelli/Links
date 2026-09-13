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

The resulting container is still media plaintext at the codec boundary. The
host must encrypt the complete container as an attachment using the existing
E2EE provider. `OpusAudioMetadata` is placed inside the encrypted `Message`
and carries bitrate, sample rate, channel count, frame duration and the
container marker. Servers see only the existing opaque encrypted attachment
and generic `MediaMetadata`; upload/download and mobile recording/playback are
the next voice-note task.
