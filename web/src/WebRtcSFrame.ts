export const WEBRTC_SFRAME_CIPHER_SUITE = 'AES_128_GCM_SHA256_128' as const

export type WebRtcSFrameCipherSuite = typeof WEBRTC_SFRAME_CIPHER_SUITE

export type WebRtcSFrameErrorType = 'authentication' | 'keyID' | 'syntax'

export interface WebRtcSFrameTransformError {
  errorType: WebRtcSFrameErrorType
  keyID: bigint | null
}

export interface WebRtcSFrameControllerOptions {
  cipherSuite?: WebRtcSFrameCipherSuite
  onError?: (error: WebRtcSFrameTransformError) => void
}

export interface WebRtcSFrameEpochKey {
  key: CryptoKey
  keyID: bigint | number
  epoch: bigint | number
}

export interface WebRtcSFrameControlKey {
  mediaSessionID: string
  keyID: bigint | number
  epoch: bigint | number
  key: BufferSource
}

interface SFrameTransformOptions {
  cipherSuite: WebRtcSFrameCipherSuite
}

interface SFrameEncryptorOptions extends SFrameTransformOptions {
  type: 'per-frame'
}

interface SFrameEncryptor {
  setEncryptionKey(key: CryptoKey, keyID: bigint): Promise<void>
}

interface SFrameDecryptor {
  addDecryptionKey(key: CryptoKey, keyID: bigint): Promise<void>
  removeDecryptionKey(keyID: bigint): Promise<void>
  onerror: ((event: Event) => void) | null
}

interface SFrameConstructor<T> {
  new (options: SFrameTransformOptions | SFrameEncryptorOptions): T
}

interface SFrameGlobal {
  RTCRtpSFrameEncryptor?: SFrameConstructor<SFrameEncryptor>
  RTCRtpSFrameDecryptor?: SFrameConstructor<SFrameDecryptor>
}

interface SenderWithTransform extends RTCRtpSender {
  transform?: SFrameEncryptor | null
}

interface ReceiverWithTransform extends RTCRtpReceiver {
  transform?: SFrameDecryptor | null
}

const MAX_CRYPTO_KEY_ID = 0xffff_ffff_ffff_ffffn

export class WebRtcSFrameError extends Error {
  constructor(message: string) {
    super(message)
    this.name = 'WebRtcSFrameError'
  }
}

/** True when the browser exposes the native W3C SFrame transform objects. */
export function supportsWebRtcSFrame(): boolean {
  const runtime = globalThis as SFrameGlobal
  return typeof runtime.RTCRtpSFrameEncryptor === 'function' &&
    typeof runtime.RTCRtpSFrameDecryptor === 'function'
}

/** Import a non-extractable AES-128 key for the native SFrame API. */
export async function importWebRtcSFrameKey(rawKey: BufferSource): Promise<CryptoKey> {
  const bytes = copySFrameKeyBytes(rawKey)
  if (typeof crypto === 'undefined' || crypto.subtle === undefined) {
    throw new WebRtcSFrameError('Web Crypto unavailable')
  }
  return crypto.subtle.importKey(
    'raw',
    bytes,
    { name: 'AES-GCM' },
    false,
    ['encrypt', 'decrypt']
  )
}

/** Return a local-only fingerprint for exact control-message replay checks. */
export async function fingerprintWebRtcSFrameKey(rawKey: BufferSource): Promise<string> {
  const bytes = copySFrameKeyBytes(rawKey)
  if (typeof crypto === 'undefined' || crypto.subtle === undefined) {
    throw new WebRtcSFrameError('Web Crypto unavailable')
  }
  const digest = await crypto.subtle.digest('SHA-256', bytes)
  return Array.from(new Uint8Array(digest), byte => byte.toString(16).padStart(2, '0')).join('')
}

/**
 * Binds native SFrame encryptors and decryptors to WebRTC media senders and
 * receivers. Raw encoded frames never enter JavaScript.
 */
export class WebRtcSFrameController {
  readonly cipherSuite: WebRtcSFrameCipherSuite
  private readonly peer: RTCPeerConnection
  private readonly onErrorCallback: (error: WebRtcSFrameTransformError) => void
  private readonly senderTransforms = new Map<RTCRtpSender, SFrameEncryptor>()
  private readonly receiverTransforms = new Map<RTCRtpReceiver, SFrameDecryptor>()
  private currentKey: WebRtcSFrameEpochKeyValue | null = null
  private previousKey: WebRtcSFrameEpochKeyValue | null = null

  constructor(peer: RTCPeerConnection, options: WebRtcSFrameControllerOptions = {}) {
    if (!supportsWebRtcSFrame()) throw new WebRtcSFrameError('Native SFrame unavailable')
    this.peer = peer
    this.cipherSuite = options.cipherSuite ?? WEBRTC_SFRAME_CIPHER_SUITE
    this.onErrorCallback = options.onError ?? (() => {})
  }

  get currentKeyID(): bigint | null {
    return this.currentKey?.keyID ?? null
  }

  get currentEpoch(): bigint | null {
    return this.currentKey?.epoch ?? null
  }

  get previousKeyID(): bigint | null {
    return this.previousKey?.keyID ?? null
  }

  async installKey(epochKey: WebRtcSFrameEpochKey): Promise<void> {
    validateCryptoKey(epochKey.key)
    const keyID = normalizeCryptoKeyID(epochKey.keyID, 'SFrame key ID')
    const epoch = normalizeCryptoKeyID(epochKey.epoch, 'SFrame epoch')
    const active = this.currentKey?.keyID === keyID
      ? this.currentKey
      : this.previousKey?.keyID === keyID ? this.previousKey : null
    if (active !== null) {
      if (active.epoch === epoch && active.key === epochKey.key) return
      throw new WebRtcSFrameError('SFrame key ID is already active')
    }
    if (this.currentKey !== null && epoch <= this.currentKey.epoch) {
      throw new WebRtcSFrameError('SFrame epoch moved backwards')
    }
    const next = { key: epochKey.key, keyID, epoch }
    const expired = this.previousKey
    this.previousKey = this.currentKey
    this.currentKey = next
    await Promise.all(Array.from(this.senderTransforms.values(), transform =>
      transform.setEncryptionKey(next.key, next.keyID)))
    await Promise.all(Array.from(this.receiverTransforms.values(), async transform => {
      await transform.addDecryptionKey(next.key, next.keyID)
      if (expired !== null) await transform.removeDecryptionKey(expired.keyID)
    }))
  }

  async attachSender(sender: RTCRtpSender): Promise<void> {
    const current = this.requireCurrentKey()
    const target = sender as SenderWithTransform
    requireTransformSlot(target, 'RTCRtpSender')
    const existing = target.transform
    const installed = this.senderTransforms.get(sender)
    if (installed !== undefined) return
    if (existing !== undefined && existing !== null) {
      throw new WebRtcSFrameError('RTCRtpSender already has a transform')
    }
    const transform = this.createEncryptor()
    target.transform = transform
    try {
      await transform.setEncryptionKey(current.key, current.keyID)
      this.senderTransforms.set(sender, transform)
    } catch (error) {
      target.transform = null
      throw error
    }
  }

  async attachReceiver(receiver: RTCRtpReceiver): Promise<void> {
    const current = this.requireCurrentKey()
    const target = receiver as ReceiverWithTransform
    requireTransformSlot(target, 'RTCRtpReceiver')
    const existing = target.transform
    const installed = this.receiverTransforms.get(receiver)
    if (installed !== undefined) return
    if (existing !== undefined && existing !== null) {
      throw new WebRtcSFrameError('RTCRtpReceiver already has a transform')
    }
    const transform = this.createDecryptor()
    transform.onerror = event => this.handleError(event)
    target.transform = transform
    try {
      if (this.previousKey !== null) {
        await transform.addDecryptionKey(this.previousKey.key, this.previousKey.keyID)
      }
      await transform.addDecryptionKey(current.key, current.keyID)
      this.receiverTransforms.set(receiver, transform)
    } catch (error) {
      target.transform = null
      throw error
    }
  }

  async attachTransceivers(transceivers = this.peer.getTransceivers()): Promise<void> {
    this.requireCurrentKey()
    for (const transceiver of transceivers) {
      if (transceiver.sender.track !== null) await this.attachSender(transceiver.sender)
      await this.attachReceiver(transceiver.receiver)
    }
  }

  detachSender(sender: RTCRtpSender): void {
    const transform = this.senderTransforms.get(sender)
    if (transform === undefined) return
    const target = sender as SenderWithTransform
    if (target.transform === transform) target.transform = null
    this.senderTransforms.delete(sender)
  }

  detachReceiver(receiver: RTCRtpReceiver): void {
    const transform = this.receiverTransforms.get(receiver)
    if (transform === undefined) return
    const target = receiver as ReceiverWithTransform
    if (target.transform === transform) target.transform = null
    this.receiverTransforms.delete(receiver)
  }

  close(): void {
    for (const sender of this.senderTransforms.keys()) this.detachSender(sender)
    for (const receiver of this.receiverTransforms.keys()) this.detachReceiver(receiver)
    this.currentKey = null
    this.previousKey = null
  }

  private createEncryptor(): SFrameEncryptor {
    const constructor = (globalThis as SFrameGlobal).RTCRtpSFrameEncryptor
    if (typeof constructor !== 'function') throw new WebRtcSFrameError('Native SFrame unavailable')
    return new constructor({ cipherSuite: this.cipherSuite, type: 'per-frame' })
  }

  private createDecryptor(): SFrameDecryptor {
    const constructor = (globalThis as SFrameGlobal).RTCRtpSFrameDecryptor
    if (typeof constructor !== 'function') throw new WebRtcSFrameError('Native SFrame unavailable')
    return new constructor({ cipherSuite: this.cipherSuite })
  }

  private requireCurrentKey(): WebRtcSFrameEpochKeyValue {
    if (this.currentKey === null) throw new WebRtcSFrameError('Install an SFrame key first')
    return this.currentKey
  }

  private handleError(event: Event): void {
    const candidate = event as Event & {
      errorType?: unknown
      keyID?: unknown
    }
    const errorType = candidate.errorType
    if (errorType !== 'authentication' && errorType !== 'keyID' && errorType !== 'syntax') return
    const keyID = candidate.keyID
    const normalizedKeyID = typeof keyID === 'bigint'
      ? keyID
      : typeof keyID === 'number' && Number.isSafeInteger(keyID) ? BigInt(keyID) : null
    try {
      this.onErrorCallback({ errorType, keyID: normalizedKeyID })
    } catch {
      // Host callbacks must not break the media transform.
    }
  }
}

interface WebRtcSFrameEpochKeyValue {
  key: CryptoKey
  keyID: bigint
  epoch: bigint
}

function validateCryptoKey(key: CryptoKey): void {
  if (key === null || typeof key !== 'object' || key.extractable !== false ||
      key.type !== 'secret' ||
      key.algorithm.name !== 'AES-GCM' || key.usages.includes('encrypt') === false ||
      key.usages.includes('decrypt') === false) {
    throw new WebRtcSFrameError('SFrame key must be non-extractable AES-GCM')
  }
  const algorithm = key.algorithm as KeyAlgorithm & { length?: number }
  if (algorithm.length !== undefined && algorithm.length !== 128) {
    throw new WebRtcSFrameError('SFrame requires a 128-bit AES-GCM key')
  }
}

function copySFrameKeyBytes(rawKey: BufferSource): Uint8Array {
  const bytes = rawKey instanceof ArrayBuffer
    ? new Uint8Array(rawKey)
    : new Uint8Array(rawKey.buffer, rawKey.byteOffset, rawKey.byteLength)
  if (bytes.byteLength !== 16) throw new WebRtcSFrameError('SFrame requires a 128-bit key')
  if (bytes.every(byte => byte === 0)) throw new WebRtcSFrameError('Invalid SFrame key')
  return bytes.slice()
}

function requireTransformSlot(
  target: SenderWithTransform | ReceiverWithTransform,
  name: string
): void {
  if (!('transform' in target)) throw new WebRtcSFrameError(`${name} Encoded Transform unavailable`)
}

function normalizeCryptoKeyID(value: bigint | number, field: string): bigint {
  const normalized = typeof value === 'bigint'
    ? value
    : Number.isSafeInteger(value) ? BigInt(value) : -1n
  if (normalized < 0n || normalized > MAX_CRYPTO_KEY_ID) {
    throw new WebRtcSFrameError(`Invalid ${field}`)
  }
  return normalized
}
