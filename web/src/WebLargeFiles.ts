import { requireCanonicalUUID } from './LinksWebClient'
import { createWebRtcTransferManifest } from './WebRtcFileTransfer'
import { WEBRTC_TRANSFER_CHUNK_BYTES } from './WebRtcFileTransfer'
import type { WebRtcFileSource, WebRtcTransferManifest } from './WebRtcFileTransfer'
import type { WebMessagingCore, WebTextMessaging } from './WebTextMessaging'

export const WEB_LARGE_FILE_CIPHERTEXT_CHUNK_BYTES = WEBRTC_TRANSFER_CHUNK_BYTES
export const WEB_LARGE_FILE_PLAINTEXT_CHUNK_BYTES =
  WEB_LARGE_FILE_CIPHERTEXT_CHUNK_BYTES - 16

export interface WebLargeFileMetadata {
  attachmentID: string
  mimeType: 'video/mp4' | 'application/octet-stream'
  originalSizeBytes: bigint
  ciphertextSizeBytes: bigint
  contentKey: Uint8Array
  nonce: Uint8Array
  ciphertextSHA256: Uint8Array
  width: number
  height: number
  durationMs: bigint
}

export interface WebLargeFileMetadataWasm {
  attachment_id(): string
  mime_type(): string
  original_size_bytes(): string
  ciphertext_size_bytes(): string
  content_key(): Uint8Array
  nonce(): Uint8Array
  ciphertext_sha256(): Uint8Array
  width(): number
  height(): number
  duration_ms(): string
}

export interface WebLargeFileUploadReceipt {
  attachmentID: string
  ciphertextSizeBytes: bigint
  ciphertextSHA256: Uint8Array
}

export interface WebPreparedLargeFile {
  metadata: WebLargeFileMetadata
  ciphertext: WebRtcFileSource
}

/** Sequential ciphertext staging. Implement with OPFS, IndexedDB, or a host file adapter. */
export interface WebLargeFileStagingSink {
  write(bytes: Uint8Array): Promise<void>
  finalize(metadata: WebLargeFileMetadata): Promise<WebRtcFileSource>
}

export interface WebLargeFileUploader {
  upload(accessToken: string, file: WebPreparedLargeFile): Promise<WebLargeFileUploadReceipt>
}

export interface WebLargeFileEncryptorWasm {
  encrypt_chunk(plaintext: Uint8Array): Uint8Array
  finish(): WebLargeFileMetadataWasm
}

export interface WebLargeFileDecryptorWasm {
  decrypt_chunk(chunkIndex: string, ciphertext: Uint8Array): Uint8Array
}

export interface WebLargeFileWasmModule {
  WebLargeFileEncryptor: new (
    attachmentID: string,
    mimeType: string,
    width: number,
    height: number,
    durationMs: string
  ) => WebLargeFileEncryptorWasm
  WebLargeFileDecryptor: new (
    attachmentID: string,
    mimeType: string,
    originalSizeBytes: string,
    ciphertextSizeBytes: string,
    contentKey: Uint8Array,
    nonce: Uint8Array,
    ciphertextSHA256: Uint8Array,
    width: number,
    height: number,
    durationMs: string
  ) => WebLargeFileDecryptorWasm
}

/** Thin Web host around the shared Rust chunk-encryption core. */
export class WebLargeFileCrypto {
  private readonly wasm: WebLargeFileWasmModule

  constructor(wasm: WebLargeFileWasmModule) {
    if (wasm === null || wasm === undefined) throw new Error('Large-file core unavailable')
    this.wasm = wasm
  }

  createEncryptor(
    attachmentID: string,
    mimeType: WebLargeFileMetadata['mimeType'],
    width = 0,
    height = 0,
    durationMs = 0n
  ): WebLargeFileEncryptor {
    validateLargeFileShape(attachmentID, mimeType, width, height, durationMs)
    return new WebLargeFileEncryptor(this.wasm, attachmentID, mimeType, width, height, durationMs)
  }

  createDecryptor(metadata: WebLargeFileMetadata): WebLargeFileDecryptor {
    validateMetadata(metadata)
    return new WebLargeFileDecryptor(this.wasm, metadata)
  }
}

export class WebLargeFileEncryptor {
  private readonly inner: WebLargeFileEncryptorWasm

  constructor(
    wasm: WebLargeFileWasmModule,
    attachmentID: string,
    mimeType: WebLargeFileMetadata['mimeType'],
    width: number,
    height: number,
    durationMs: bigint
  ) {
    this.inner = new wasm.WebLargeFileEncryptor(
      attachmentID,
      mimeType,
      width,
      height,
      durationMs.toString()
    )
  }

  encryptChunk(plaintext: Uint8Array): Uint8Array {
    if (plaintext.length === 0) throw new Error('Empty large-file chunk')
    return this.inner.encrypt_chunk(plaintext).slice()
  }

  finish(): WebLargeFileMetadata {
    const metadata = this.inner.finish()
    return {
      attachmentID: requireCanonicalUUID(metadata.attachment_id(), 'attachment ID'),
      mimeType: requireLargeFileMime(metadata.mime_type()),
      originalSizeBytes: BigInt(metadata.original_size_bytes()),
      ciphertextSizeBytes: BigInt(metadata.ciphertext_size_bytes()),
      contentKey: metadata.content_key().slice(),
      nonce: metadata.nonce().slice(),
      ciphertextSHA256: metadata.ciphertext_sha256().slice(),
      width: metadata.width(),
      height: metadata.height(),
      durationMs: BigInt(metadata.duration_ms())
    }
  }
}

export class WebLargeFileDecryptor {
  private readonly inner: WebLargeFileDecryptorWasm

  constructor(wasm: WebLargeFileWasmModule, metadata: WebLargeFileMetadata) {
    this.inner = new wasm.WebLargeFileDecryptor(
      metadata.attachmentID,
      metadata.mimeType,
      metadata.originalSizeBytes.toString(),
      metadata.ciphertextSizeBytes.toString(),
      metadata.contentKey,
      metadata.nonce,
      metadata.ciphertextSHA256,
      metadata.width,
      metadata.height,
      metadata.durationMs.toString()
    )
  }

  decryptChunk(chunkIndex: bigint, ciphertext: Uint8Array): Uint8Array {
    return this.inner.decrypt_chunk(chunkIndex.toString(), ciphertext).slice()
  }
}

export interface WebLargeFilePlaintextSink {
  prepare(metadata: WebLargeFileMetadata): Promise<void>
  write(offset: bigint, bytes: Uint8Array): Promise<void>
  finalize(metadata: WebLargeFileMetadata): Promise<void>
  abort?(metadata: WebLargeFileMetadata): Promise<void>
}

export interface WebLargeFileCiphertextSource extends WebRtcFileSource {
  sha256(): Promise<Uint8Array>
}

/** Decrypt a durable ciphertext source without buffering the full file. */
export async function decryptLargeFileSource(
  metadata: WebLargeFileMetadata,
  source: WebLargeFileCiphertextSource,
  sink: WebLargeFilePlaintextSink,
  crypto: WebLargeFileCrypto
): Promise<void> {
  validateMetadata(metadata)
  const decryptor = crypto.createDecryptor(metadata)
  await sink.prepare(metadata)
  let ciphertextOffset = 0n
  let plaintextOffset = 0n
  const chunkCount = (metadata.originalSizeBytes +
    BigInt(WEB_LARGE_FILE_PLAINTEXT_CHUNK_BYTES) - 1n) /
    BigInt(WEB_LARGE_FILE_PLAINTEXT_CHUNK_BYTES)
  try {
    for (let chunkIndex = 0n; chunkIndex < chunkCount; chunkIndex++) {
      const remaining = metadata.originalSizeBytes - plaintextOffset
      const plaintextLength = Number(remaining <
        BigInt(WEB_LARGE_FILE_PLAINTEXT_CHUNK_BYTES)
        ? remaining
        : BigInt(WEB_LARGE_FILE_PLAINTEXT_CHUNK_BYTES))
      const ciphertextLength = plaintextLength + 16
      const ciphertext = await source.read(ciphertextOffset, ciphertextLength)
      if (ciphertext.length !== ciphertextLength) throw new Error('Short large-file ciphertext')
      const plaintext = decryptor.decryptChunk(chunkIndex, ciphertext)
      if (plaintext.length !== plaintextLength) throw new Error('Invalid large-file plaintext')
      await sink.write(plaintextOffset, plaintext)
      ciphertextOffset += BigInt(ciphertextLength)
      plaintextOffset += BigInt(plaintext.length)
    }
    if (ciphertextOffset !== metadata.ciphertextSizeBytes ||
        plaintextOffset !== metadata.originalSizeBytes ||
        !sameBytes(await source.sha256(), metadata.ciphertextSHA256)) {
      throw new Error('Large-file ciphertext integrity failed')
    }
    await sink.finalize(metadata)
  } catch (error) {
    if (sink.abort !== undefined) await sink.abort(metadata)
    throw error
  }
}

export interface WebLargeFileSessionOptions {
  core: WebMessagingCore
  messaging: WebTextMessaging
  uploader: WebLargeFileUploader
  accessToken: () => string
  crypto: WebLargeFileCrypto
}

/** Web video/file flow: chunk-encrypt to durable ciphertext, then upload/send. */
export class WebLargeFileSession {
  private readonly core: WebMessagingCore
  private readonly messaging: WebTextMessaging
  private readonly uploader: WebLargeFileUploader
  private readonly accessToken: () => string
  private readonly crypto: WebLargeFileCrypto

  constructor(options: WebLargeFileSessionOptions) {
    if (options.core === null || options.messaging === null ||
        options.uploader === null || typeof options.accessToken !== 'function') {
      throw new Error('Invalid Web large-file session')
    }
    requireCanonicalUUID(options.core.userID, 'user ID')
    requireCanonicalUUID(options.core.deviceID, 'device ID')
    this.core = options.core
    this.messaging = options.messaging
    this.uploader = options.uploader
    this.accessToken = options.accessToken
    this.crypto = options.crypto
  }

  async prepareAndEncrypt(
    source: Blob,
    sink: WebLargeFileStagingSink,
    attachmentID: string,
    mimeType: WebLargeFileMetadata['mimeType'],
    width = 0,
    height = 0,
    durationMs = 0n
  ): Promise<WebPreparedLargeFile> {
    if (!(source instanceof Blob) || source.size === 0) throw new Error('Invalid large file')
    const encryptor = this.crypto.createEncryptor(
      attachmentID, mimeType, width, height, durationMs)
    for (let offset = 0; offset < source.size; offset += WEB_LARGE_FILE_PLAINTEXT_CHUNK_BYTES) {
      const plaintext = new Uint8Array(await source.slice(
        offset, offset + WEB_LARGE_FILE_PLAINTEXT_CHUNK_BYTES).arrayBuffer())
      if (plaintext.length === 0) throw new Error('Empty large-file chunk')
      await sink.write(encryptor.encryptChunk(plaintext))
      plaintext.fill(0)
    }
    const metadata = encryptor.finish()
    return { metadata, ciphertext: await sink.finalize(metadata) }
  }

  async upload(file: WebPreparedLargeFile): Promise<WebLargeFileUploadReceipt> {
    validatePreparedFile(file)
    const receipt = await this.uploader.upload(this.requireAccessToken(), file)
    if (!matchesReceipt(file.metadata, receipt)) {
      throw new Error('Invalid large-file upload receipt')
    }
    return receipt
  }

  send(
    conversationID: string,
    recipientUserID: string,
    file: WebPreparedLargeFile,
    receipt: WebLargeFileUploadReceipt
  ): void {
    validatePreparedFile(file)
    if (!matchesReceipt(file.metadata, receipt)) {
      throw new Error('Invalid large-file upload receipt')
    }
    if (this.core.sendLargeFile === undefined) throw new Error('Web large-file core unavailable')
    this.core.sendLargeFile(
      conversationID, recipientUserID, file.metadata, receipt, this.messaging)
  }

  private requireAccessToken(): string {
    const token = this.accessToken()
    if (typeof token !== 'string' || token.length === 0) {
      throw new Error('Authenticated Web session required')
    }
    return token
  }
}

export function transferManifestForLargeFile(
  metadata: WebLargeFileMetadata,
  transferID?: string
): WebRtcTransferManifest {
  validateMetadata(metadata)
  return createWebRtcTransferManifest(
    metadata.attachmentID,
    metadata.ciphertextSizeBytes,
    metadata.ciphertextSHA256,
    transferID
  )
}

function validateMetadata(metadata: WebLargeFileMetadata): void {
  validateLargeFileShape(
    metadata.attachmentID,
    metadata.mimeType,
    metadata.width,
    metadata.height,
    metadata.durationMs
  )
  if (metadata.originalSizeBytes <= 0n || metadata.ciphertextSizeBytes <= 0n ||
      metadata.contentKey.length !== 32 || metadata.nonce.length !== 12 ||
      metadata.ciphertextSHA256.length !== 32) {
    throw new Error('Invalid large-file metadata')
  }
  const chunkCount = (metadata.originalSizeBytes +
    BigInt(WEB_LARGE_FILE_PLAINTEXT_CHUNK_BYTES) - 1n) /
    BigInt(WEB_LARGE_FILE_PLAINTEXT_CHUNK_BYTES)
  const expectedCiphertext = metadata.originalSizeBytes + chunkCount * 16n
  if (metadata.ciphertextSizeBytes !== expectedCiphertext) {
    throw new Error('Invalid large-file ciphertext size')
  }
}

function validateLargeFileShape(
  attachmentID: string,
  mimeType: WebLargeFileMetadata['mimeType'],
  width: number,
  height: number,
  durationMs: bigint
): void {
  requireCanonicalUUID(attachmentID, 'attachment ID')
  requireLargeFileMime(mimeType)
  if (width < 0 || height < 0 || durationMs < 0n ||
      mimeType === 'video/mp4' && (width === 0 || height === 0 || durationMs === 0n) ||
      mimeType === 'application/octet-stream' && (width !== 0 || height !== 0 || durationMs !== 0n)) {
    throw new Error('Invalid large-file metadata')
  }
}

function requireLargeFileMime(value: string): WebLargeFileMetadata['mimeType'] {
  if (value !== 'video/mp4' && value !== 'application/octet-stream') {
    throw new Error('Invalid large-file MIME type')
  }
  return value
}

function validatePreparedFile(file: WebPreparedLargeFile): void {
  validateMetadata(file.metadata)
  if (file.ciphertext === null || file.ciphertext === undefined) {
    throw new Error('Missing large-file ciphertext source')
  }
}

function matchesReceipt(
  metadata: WebLargeFileMetadata,
  receipt: WebLargeFileUploadReceipt
): boolean {
  validateLargeFileReceipt(receipt)
  return receipt.attachmentID === metadata.attachmentID &&
    receipt.ciphertextSizeBytes === metadata.ciphertextSizeBytes &&
    sameBytes(receipt.ciphertextSHA256, metadata.ciphertextSHA256)
}

function validateLargeFileReceipt(receipt: WebLargeFileUploadReceipt): void {
  requireCanonicalUUID(receipt.attachmentID, 'attachment ID')
  if (receipt.ciphertextSizeBytes <= 0n || receipt.ciphertextSHA256.length !== 32) {
    throw new Error('Invalid large-file upload receipt')
  }
}

function sameBytes(left: Uint8Array, right: Uint8Array): boolean {
  return left.length === right.length && left.every((value, index) => value === right[index])
}
