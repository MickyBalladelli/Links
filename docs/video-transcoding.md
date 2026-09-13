# Native video transcoding

Mobile video normalization uses the platform hardware codec stack before any
attachment encryption:

- iOS `IOSVideoTranscoder` reads frames with `AVAssetReader`, scales them with
  `AVMutableVideoComposition`, and encodes H.264 or HEVC with a
  `VTCompressionSession`. Hardware encoders are required; iOS 17.4+ also passes
  VideoToolbox's explicit hardware-only encoder specification. Compatible AAC
  audio is copied into the MP4, and `shouldOptimizeForNetworkUse` places the
  `moov` atom before media data.
- Android `AndroidVideoTranscoder` uses hardware `MediaCodec` decoder and
  encoder surfaces joined by an EGL scaler, then muxes MP4 with compatible AAC
  audio. Software codec names are rejected. `Mp4FastStart` then moves `moov`
  before `mdat` and updates absolute `stco`/`co64` chunk offsets.

Both adapters expose the same profiles: 1280x720 at 1.5 Mbps and 1920x1080 at
3 Mbps, 30 fps. The shared `links-client-core::video` module owns these profile
limits and the 256 MiB input bound. No transcoder receives attachment keys or
uploads media; its output remains a local plaintext staging file for the later
encrypted video pipeline.

Transcoding must run off the UI thread. If no hardware codec supports the
requested profile, the operation fails closed instead of silently using a
software encoder.
