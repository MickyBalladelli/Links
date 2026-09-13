import { requireCanonicalUUID } from './LinksWebClient'
import { createWebRtcTransferManifest } from './WebRtcFileTransfer'
import type { WebRtcTransferManifest } from './WebRtcFileTransfer'

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
