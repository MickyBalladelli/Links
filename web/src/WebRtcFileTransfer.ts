const MAGIC = new Uint8Array([0x4c, 0x44, 0x54, 0x31])
const VERSION = 1
const HEADER_BYTES = 6
const UUID_BYTES = 16
const DIGEST_BYTES = 32
const BEGIN_BYTES = HEADER_BYTES + UUID_BYTES + UUID_BYTES + 8 + 4 + DIGEST_BYTES
const OFFSET_BYTES = HEADER_BYTES + UUID_BYTES + 8
const CHUNK_HEADER_BYTES = HEADER_BYTES + UUID_BYTES + 8 + 4 + DIGEST_BYTES
const FINISH_BYTES = HEADER_BYTES + UUID_BYTES + DIGEST_BYTES
const COMPLETE_BYTES = HEADER_BYTES + UUID_BYTES + 1 + DIGEST_BYTES

export const WEBRTC_TRANSFER_CHUNK_BYTES = 256 * 1024
export const WEBRTC_TRANSFER_MAX_FRAME_BYTES = WEBRTC_TRANSFER_CHUNK_BYTES + 66
const BUFFERED_AMOUNT_HIGH_WATERMARK = 4 * 1024 * 1024
const BUFFERED_AMOUNT_LOW_WATERMARK = 1024 * 1024

export interface WebRtcTransferManifest {
  transferID: string
  attachmentID: string
  totalBytes: bigint
  ciphertextSHA256: Uint8Array
  chunkBytes?: number
}

export interface WebRtcFileSource {
  read(offset: bigint, length: number): Promise<Uint8Array>
}

export interface WebRtcFileSink {
  /** Return the durable prefix length already present for this transfer. */
  prepare(manifest: WebRtcTransferManifest): Promise<bigint>
  /** Write one chunk at its exact acknowledged offset. */
  write(offset: bigint, bytes: Uint8Array): Promise<void>
  /** Hash the complete ciphertext file without loading it into memory. */
  sha256(): Promise<Uint8Array>
  finalize(manifest: WebRtcTransferManifest): Promise<void>
}

type TransferFrame =
  | { kind: 'begin'; manifest: WebRtcTransferManifest }
  | { kind: 'resume'; transferID: string; nextOffset: bigint }
  | { kind: 'chunk'; transferID: string; offset: bigint; bytes: Uint8Array; sha256: Uint8Array }
  | { kind: 'finish'; transferID: string; ciphertextSHA256: Uint8Array }
  | { kind: 'ack'; transferID: string; nextOffset: bigint }
  | { kind: 'complete'; transferID: string; status: number; ciphertextSHA256: Uint8Array }

export function createWebRtcTransferManifest(
  attachmentID: string,
  totalBytes: bigint,
  ciphertextSHA256: Uint8Array,
  transferID = crypto.randomUUID()
): WebRtcTransferManifest {
  const manifest = {
    transferID: requireUUID(transferID, 'transfer ID'),
    attachmentID: requireUUID(attachmentID, 'attachment ID'),
    totalBytes,
    ciphertextSHA256: new Uint8Array(ciphertextSHA256),
    chunkBytes: WEBRTC_TRANSFER_CHUNK_BYTES
  }
  validateManifest(manifest)
  return manifest
}

/** Read a Blob/File in bounded slices; the whole attachment never enters RAM. */
export function webRtcBlobSource(blob: Blob): WebRtcFileSource {
  return {
    async read(offset, length) {
      if (offset > BigInt(Number.MAX_SAFE_INTEGER)) {
        throw new Error('Browser Blob offset exceeds safe integer range')
      }
      const start = Number(offset)
      const bytes = new Uint8Array(await blob.slice(start, start + length).arrayBuffer())
      return bytes
    }
  }
}

/**
 * Ordered/reliable WebRTC DataChannel file transfer. Signaling and MLS
 * metadata exchange happen outside this class; the channel must already be
 * authenticated to the peer and must carry ciphertext, never attachment keys.
 */
export class WebRtcFileTransfer {
  private readonly channel: RTCDataChannel
  private readonly frames: TransferFrame[] = []
  private readonly waiters: Array<{
    resolve: (frame: TransferFrame) => void
    reject: (error: Error) => void
  }> = []
  private closedError: Error | null = null

  constructor(channel: RTCDataChannel) {
    if (channel === undefined || channel === null) throw new Error('Missing DataChannel')
    if (channel.ordered !== true || channel.maxRetransmits !== null || channel.maxPacketLifeTime !== null) {
      throw new Error('File DataChannel must be ordered and reliable')
    }
    this.channel = channel
    this.channel.binaryType = 'arraybuffer'
    this.channel.bufferedAmountLowThreshold = BUFFERED_AMOUNT_LOW_WATERMARK
    this.channel.addEventListener('message', event => {
      void this.receiveMessage(event.data)
    })
    this.channel.addEventListener('close', () => {
      this.fail(new Error('DataChannel closed'))
    })
    this.channel.addEventListener('error', () => {
      this.fail(new Error('DataChannel failed'))
    })
  }

  async send(manifest: WebRtcTransferManifest, source: WebRtcFileSource): Promise<void> {
    validateManifest(manifest)
    await this.sendFrame({ kind: 'begin', manifest })
    const resume = await this.nextFrame()
    if (resume.kind !== 'resume' || resume.transferID !== manifest.transferID) {
      throw new Error('Unexpected transfer resume frame')
    }
    validateOffset(manifest, resume.nextOffset)

    let offset = resume.nextOffset
    while (offset < manifest.totalBytes) {
      const length = Number(minBigInt(BigInt(manifest.chunkBytes!), manifest.totalBytes - offset))
      const bytes = await source.read(offset, length)
      if (bytes.length !== length) throw new Error('Transfer source returned a short chunk')
      await this.sendFrame(await makeChunk(manifest.transferID, offset, bytes))
      const ack = await this.nextFrame()
      if (ack.kind !== 'ack' || ack.transferID !== manifest.transferID || ack.nextOffset !== offset + BigInt(length)) {
        throw new Error('Unexpected transfer acknowledgement')
      }
      offset = ack.nextOffset
    }

    await this.sendFrame({
      kind: 'finish',
      transferID: manifest.transferID,
      ciphertextSHA256: manifest.ciphertextSHA256
    })
    const complete = await this.nextFrame()
    if (complete.kind !== 'complete' || complete.transferID !== manifest.transferID ||
        complete.status !== 1 || !equalBytes(complete.ciphertextSHA256, manifest.ciphertextSHA256)) {
      throw new Error('Peer rejected transfer integrity')
    }
  }

  async receive(sink: WebRtcFileSink): Promise<WebRtcTransferManifest> {
    const begin = await this.nextFrame()
    if (begin.kind !== 'begin') throw new Error('Expected transfer manifest')
    validateManifest(begin.manifest)
    const manifest = begin.manifest
    let offset = await sink.prepare(manifest)
    validateOffset(manifest, offset)
    await this.sendFrame({ kind: 'resume', transferID: manifest.transferID, nextOffset: offset })

    while (true) {
      const frame = await this.nextFrame()
      if (frame.kind === 'chunk') {
        if (frame.transferID !== manifest.transferID || frame.offset !== offset) {
          throw new Error('Transfer chunk is out of order')
        }
        const expected = Number(minBigInt(BigInt(manifest.chunkBytes!), manifest.totalBytes - offset))
        if (frame.bytes.length !== expected || !equalBytes(frame.sha256, await sha256(frame.bytes))) {
          throw new Error('Transfer chunk integrity failed')
        }
        await sink.write(offset, frame.bytes)
        offset += BigInt(frame.bytes.length)
        await this.sendFrame({ kind: 'ack', transferID: manifest.transferID, nextOffset: offset })
      } else if (frame.kind === 'finish') {
        const digest = await sink.sha256()
        const complete = frame.transferID === manifest.transferID && offset === manifest.totalBytes &&
          equalBytes(frame.ciphertextSHA256, manifest.ciphertextSHA256) &&
          equalBytes(digest, manifest.ciphertextSHA256)
        if (!complete) {
          await this.sendFrame({
            kind: 'complete',
            transferID: manifest.transferID,
            status: 2,
            ciphertextSHA256: manifest.ciphertextSHA256
          })
          throw new Error('Transfer ciphertext integrity failed')
        }
        await sink.finalize(manifest)
        await this.sendFrame({
          kind: 'complete',
          transferID: manifest.transferID,
          status: 1,
          ciphertextSHA256: manifest.ciphertextSHA256
        })
        return manifest
      } else {
        throw new Error('Unexpected transfer frame')
      }
    }
  }

  close(): void {
    this.fail(new Error('Transfer cancelled'))
  }

  private async sendFrame(frame: TransferFrame): Promise<void> {
    const bytes = encodeFrame(frame)
    while (this.channel.bufferedAmount > BUFFERED_AMOUNT_HIGH_WATERMARK) {
      await this.waitForBufferedAmountLow()
    }
    if (this.channel.readyState !== 'open') throw new Error('DataChannel is not open')
    this.channel.send(bytes)
  }

  private nextFrame(): Promise<TransferFrame> {
    if (this.closedError !== null) return Promise.reject(this.closedError)
    const frame = this.frames.shift()
    if (frame !== undefined) return Promise.resolve(frame)
    return new Promise((resolve, reject) => this.waiters.push({ resolve, reject }))
  }

  private async receiveMessage(data: unknown): Promise<void> {
    try {
      const bytes = data instanceof ArrayBuffer
        ? new Uint8Array(data)
        : data instanceof Blob
          ? new Uint8Array(await data.arrayBuffer())
          : data instanceof Uint8Array
            ? data
            : (() => { throw new Error('DataChannel sent non-binary data') })()
      this.pushFrame(decodeFrame(bytes))
    } catch (error) {
      this.fail(error instanceof Error ? error : new Error('Invalid transfer frame'))
    }
  }

  private pushFrame(frame: TransferFrame): void {
    const waiter = this.waiters.shift()
    if (waiter !== undefined) waiter.resolve(frame)
    else this.frames.push(frame)
  }

  private fail(error: Error): void {
    if (this.closedError !== null) return
    this.closedError = error
    while (this.waiters.length > 0) this.waiters.shift()!.reject(error)
  }

  private waitForBufferedAmountLow(): Promise<void> {
    return new Promise((resolve, reject) => {
      const onLow = () => {
        cleanup()
        resolve()
      }
      const onClose = () => {
        cleanup()
        reject(new Error('DataChannel closed'))
      }
      const cleanup = () => {
        this.channel.removeEventListener('bufferedamountlow', onLow)
        this.channel.removeEventListener('close', onClose)
      }
      this.channel.addEventListener('bufferedamountlow', onLow)
      this.channel.addEventListener('close', onClose)
      if (this.channel.bufferedAmount <= BUFFERED_AMOUNT_LOW_WATERMARK) {
        cleanup()
        resolve()
      }
    })
  }
}

async function makeChunk(transferID: string, offset: bigint, bytes: Uint8Array): Promise<TransferFrame> {
  return {
    kind: 'chunk',
    transferID,
    offset,
    bytes,
    sha256: await sha256(bytes)
  }
}

function encodeFrame(frame: TransferFrame): Uint8Array {
  const body = frame.kind === 'begin'
    ? encodeBegin(frame.manifest)
    : frame.kind === 'resume'
      ? encodeOffset(2, frame.transferID, frame.nextOffset)
      : frame.kind === 'chunk'
        ? encodeChunk(frame)
        : frame.kind === 'finish'
          ? encodeFinish(frame)
          : frame.kind === 'ack'
            ? encodeOffset(5, frame.transferID, frame.nextOffset)
            : encodeComplete(frame)
  if (body.length > WEBRTC_TRANSFER_MAX_FRAME_BYTES) throw new Error('Transfer frame too large')
  return body
}

function encodeBegin(manifest: WebRtcTransferManifest): Uint8Array {
  const bytes = new Uint8Array(BEGIN_BYTES)
  const view = header(bytes, 1)
  bytes.set(uuidBytes(manifest.transferID), HEADER_BYTES)
  bytes.set(uuidBytes(manifest.attachmentID), HEADER_BYTES + UUID_BYTES)
  view.setBigUint64(HEADER_BYTES + UUID_BYTES * 2, manifest.totalBytes)
  view.setUint32(HEADER_BYTES + UUID_BYTES * 2 + 8, manifest.chunkBytes!, false)
  bytes.set(manifest.ciphertextSHA256, HEADER_BYTES + UUID_BYTES * 2 + 12)
  return bytes
}

function encodeOffset(kind: number, transferID: string, nextOffset: bigint): Uint8Array {
  const bytes = new Uint8Array(OFFSET_BYTES)
  const view = header(bytes, kind)
  bytes.set(uuidBytes(transferID), HEADER_BYTES)
  view.setBigUint64(HEADER_BYTES + UUID_BYTES, nextOffset)
  return bytes
}

function encodeChunk(frame: Extract<TransferFrame, { kind: 'chunk' }>): Uint8Array {
  const bytes = new Uint8Array(CHUNK_HEADER_BYTES + frame.bytes.length)
  const view = header(bytes, 3)
  bytes.set(uuidBytes(frame.transferID), HEADER_BYTES)
  view.setBigUint64(HEADER_BYTES + UUID_BYTES, frame.offset)
  view.setUint32(HEADER_BYTES + UUID_BYTES + 8, frame.bytes.length, false)
  bytes.set(frame.sha256, HEADER_BYTES + UUID_BYTES + 12)
  bytes.set(frame.bytes, CHUNK_HEADER_BYTES)
  return bytes
}

function encodeFinish(frame: Extract<TransferFrame, { kind: 'finish' }>): Uint8Array {
  const bytes = new Uint8Array(FINISH_BYTES)
  header(bytes, 4)
  bytes.set(uuidBytes(frame.transferID), HEADER_BYTES)
  bytes.set(frame.ciphertextSHA256, HEADER_BYTES + UUID_BYTES)
  return bytes
}

function encodeComplete(frame: Extract<TransferFrame, { kind: 'complete' }>): Uint8Array {
  const bytes = new Uint8Array(COMPLETE_BYTES)
  header(bytes, 6)
  bytes.set(uuidBytes(frame.transferID), HEADER_BYTES)
  bytes[HEADER_BYTES + UUID_BYTES] = frame.status
  bytes.set(frame.ciphertextSHA256, HEADER_BYTES + UUID_BYTES + 1)
  return bytes
}

function header(bytes: Uint8Array, kind: number): DataView {
  bytes.set(MAGIC)
  bytes[4] = VERSION
  bytes[5] = kind
  return new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength)
}

function decodeFrame(bytes: Uint8Array): TransferFrame {
  if (bytes.length < HEADER_BYTES || bytes.length > WEBRTC_TRANSFER_MAX_FRAME_BYTES ||
      !equalBytes(bytes.subarray(0, 4), MAGIC) || bytes[4] !== VERSION) {
    throw new Error('Invalid transfer frame')
  }
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength)
  const kind = bytes[5]
  if (kind === 1) {
    if (bytes.length !== BEGIN_BYTES) throw new Error('Invalid transfer manifest')
    const manifest = {
      transferID: uuidString(bytes.subarray(HEADER_BYTES, HEADER_BYTES + UUID_BYTES)),
      attachmentID: uuidString(bytes.subarray(HEADER_BYTES + UUID_BYTES, HEADER_BYTES + UUID_BYTES * 2)),
      totalBytes: view.getBigUint64(HEADER_BYTES + UUID_BYTES * 2),
      chunkBytes: view.getUint32(HEADER_BYTES + UUID_BYTES * 2 + 8),
      ciphertextSHA256: bytes.slice(HEADER_BYTES + UUID_BYTES * 2 + 12)
    }
    validateManifest(manifest)
    return { kind: 'begin', manifest }
  }
  if (kind === 2 || kind === 5) {
    if (bytes.length !== OFFSET_BYTES) throw new Error('Invalid transfer offset')
    return {
      kind: kind === 2 ? 'resume' : 'ack',
      transferID: uuidString(bytes.subarray(HEADER_BYTES, HEADER_BYTES + UUID_BYTES)),
      nextOffset: view.getBigUint64(HEADER_BYTES + UUID_BYTES)
    }
  }
  if (kind === 3) {
    if (bytes.length < CHUNK_HEADER_BYTES) throw new Error('Invalid transfer chunk')
    const length = view.getUint32(HEADER_BYTES + UUID_BYTES + 8)
    if (length === 0 || length > WEBRTC_TRANSFER_CHUNK_BYTES ||
        bytes.length !== CHUNK_HEADER_BYTES + length) throw new Error('Invalid transfer chunk')
    const payload = bytes.slice(CHUNK_HEADER_BYTES)
    const digest = bytes.slice(HEADER_BYTES + UUID_BYTES + 12, CHUNK_HEADER_BYTES)
    return {
      kind: 'chunk',
      transferID: uuidString(bytes.subarray(HEADER_BYTES, HEADER_BYTES + UUID_BYTES)),
      offset: view.getBigUint64(HEADER_BYTES + UUID_BYTES),
      bytes: payload,
      sha256: digest
    }
  }
  if (kind === 4) {
    if (bytes.length !== FINISH_BYTES) throw new Error('Invalid transfer finish')
    return {
      kind: 'finish',
      transferID: uuidString(bytes.subarray(HEADER_BYTES, HEADER_BYTES + UUID_BYTES)),
      ciphertextSHA256: bytes.slice(HEADER_BYTES + UUID_BYTES)
    }
  }
  if (kind === 6) {
    if (bytes.length !== COMPLETE_BYTES || bytes[HEADER_BYTES + UUID_BYTES] < 1 || bytes[HEADER_BYTES + UUID_BYTES] > 3) {
      throw new Error('Invalid transfer completion')
    }
    return {
      kind: 'complete',
      transferID: uuidString(bytes.subarray(HEADER_BYTES, HEADER_BYTES + UUID_BYTES)),
      status: bytes[HEADER_BYTES + UUID_BYTES],
      ciphertextSHA256: bytes.slice(HEADER_BYTES + UUID_BYTES + 1)
    }
  }
  throw new Error('Unknown transfer frame')
}

function validateManifest(manifest: WebRtcTransferManifest): void {
  requireUUID(manifest.transferID, 'transfer ID')
  requireUUID(manifest.attachmentID, 'attachment ID')
  if (manifest.totalBytes <= 0n || manifest.ciphertextSHA256.length !== DIGEST_BYTES ||
      (manifest.chunkBytes ?? WEBRTC_TRANSFER_CHUNK_BYTES) !== WEBRTC_TRANSFER_CHUNK_BYTES) {
    throw new Error('Invalid transfer manifest')
  }
  manifest.chunkBytes = WEBRTC_TRANSFER_CHUNK_BYTES
}

function validateOffset(manifest: WebRtcTransferManifest, offset: bigint): void {
  if (offset < 0n || offset > manifest.totalBytes ||
      (offset !== manifest.totalBytes && offset % BigInt(manifest.chunkBytes!) !== 0n)) {
    throw new Error('Invalid transfer resume offset')
  }
}

function requireUUID(value: string, field: string): string {
  if (!/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/.test(value) ||
      value === '00000000-0000-0000-0000-000000000000') throw new Error(`Invalid ${field}`)
  return value
}

function uuidBytes(value: string): Uint8Array {
  const hex = requireUUID(value, 'UUID').replace(/-/g, '')
  const bytes = new Uint8Array(UUID_BYTES)
  for (let index = 0; index < UUID_BYTES; index++) bytes[index] = Number.parseInt(hex.slice(index * 2, index * 2 + 2), 16)
  return bytes
}

function uuidString(bytes: Uint8Array): string {
  if (bytes.length !== UUID_BYTES) throw new Error('Invalid UUID')
  const hex = Array.from(bytes, byte => byte.toString(16).padStart(2, '0')).join('')
  return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`
}

function minBigInt(left: bigint, right: bigint): bigint {
  return left < right ? left : right
}

async function sha256(bytes: Uint8Array): Promise<Uint8Array> {
  const copy = new ArrayBuffer(bytes.byteLength)
  new Uint8Array(copy).set(bytes)
  return new Uint8Array(await crypto.subtle.digest('SHA-256', copy))
}

function equalBytes(left: Uint8Array, right: Uint8Array): boolean {
  if (left.length !== right.length) return false
  let difference = 0
  for (let index = 0; index < left.length; index++) difference |= left[index] ^ right[index]
  return difference === 0
}
