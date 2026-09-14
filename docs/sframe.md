# WebRTC SFrame media encryption

Links protects WebRTC audio and video at the encoded-frame boundary. The
browser's native `RTCRtpSFrameEncryptor` and `RTCRtpSFrameDecryptor` are
attached to `RTCRtpSender.transform` and `RTCRtpReceiver.transform` by
`web/src/WebRtcSFrame.ts`.

## Browser contract

The host must enable SFrame before offer/answer negotiation, import the MLS
epoch secret as a non-extractable AES-GCM `CryptoKey`, install it, and attach
the transforms before creating the local description:

```ts
const sframe = session.enableSFrame()
const key = await importWebRtcSFrameKey(mlsEpochKeyBytes)
await sframe.installKey({ key, keyID: 7n, epoch: 12n })
await session.attachSFrameTransforms()
await session.startOffer()
```

The controller supports `AES_128_GCM_SHA256_128` and per-frame protection.
It keeps only the current and previous key IDs, installs the new decryptor
key before removing the expired one, and rejects stale epochs or active key-ID
reuse. Decryption authentication, key-ID, and syntax failures are reported to
the host through `onError`.

The raw encoded frame bytes do not enter JavaScript. If the native SFrame
objects are unavailable, enabling SFrame throws and the host must not start a
media session without encryption. The W3C Encoded Transform API is still a
Working Draft, so each client must capability-gate it and retain a compatible
platform implementation before release.

## Key custody

MLS remains the source of epoch key material. This task only adds the local
browser binding and bounded key schedule. The next MLS control-channel task
must deliver epoch keys to each authorized device without placing key material
in signaling, SFU, queue, or blob-storage paths.
