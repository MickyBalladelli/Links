import { requireCanonicalUUID } from './LinksWebClient'
import type {
  WebFileMetadata,
  WebFileUploadReceipt
} from './WebTextMessaging'
import { validateWebFileMetadata, validateWebFileUploadReceipt } from './WebTextMessaging'

export const WEB_FILE_MAX_PLAINTEXT_BYTES = 20 * 1024 * 1024
export const WEB_FILE_MAX_CIPHERTEXT_BYTES = WEB_FILE_MAX_PLAINTEXT_BYTES + 16

export interface WebFileEncryptionWasm {
  attachment_id(): string
  mime_type(): string
  file_name(): string
  ciphertext_size_bytes(): string
  content_key(): Uint8Array
  nonce(): Uint8Array
  ciphertext_sha256(): Uint8Array
  ciphertext(): Uint8Array
}

export interface WebFileWasmModule {
  encrypt_file(
    attachmentID: string,
    plaintext: Uint8Array,
    mimeType: string,
    fileName: string
  ): WebFileEncryptionWasm
}

export interface WebEncryptedFile {
  metadata: WebFileMetadata
  ciphertext: Uint8Array
}

export class WebFileSession {
  constructor(private readonly wasm: WebFileWasmModule) {
    if (wasm === null || wasm === undefined || typeof wasm.encrypt_file !== 'function') {
      throw new Error('Web file core unavailable')
    }
  }

  async encrypt(source: Blob, fileName: string, mimeType: string): Promise<WebEncryptedFile> {
    if (!(source instanceof Blob) || source.size === 0 ||
        source.size > WEB_FILE_MAX_PLAINTEXT_BYTES) {
      throw new Error('Invalid Web file')
    }
    const plaintext = new Uint8Array(await source.arrayBuffer())
    try {
      const encrypted = this.wasm.encrypt_file(
        crypto.randomUUID(),
        plaintext,
        mimeType || 'application/octet-stream',
        fileName
      )
      const metadata: WebFileMetadata = {
        attachmentID: requireCanonicalUUID(encrypted.attachment_id(), 'attachment ID'),
        mimeType: encrypted.mime_type(),
        fileName: encrypted.file_name(),
        ciphertextSizeBytes: BigInt(encrypted.ciphertext_size_bytes()),
        contentKey: encrypted.content_key().slice(),
        nonce: encrypted.nonce().slice(),
        ciphertextSHA256: encrypted.ciphertext_sha256().slice()
      }
      const ciphertext = encrypted.ciphertext().slice()
      validateWebFileMetadata(metadata)
      if (BigInt(ciphertext.length) !== metadata.ciphertextSizeBytes) {
        throw new Error('Invalid Web file ciphertext')
      }
      return { metadata, ciphertext }
    } finally {
      plaintext.fill(0)
    }
  }

  async upload(
    accessToken: string,
    file: WebEncryptedFile
  ): Promise<WebFileUploadReceipt> {
    validateWebFileMetadata(file.metadata)
    if (BigInt(file.ciphertext.length) !== file.metadata.ciphertextSizeBytes) {
      throw new Error('Invalid Web file ciphertext')
    }
    const response = await fetch(
      `/links-api/v1/blobs/${encodeURIComponent(file.metadata.attachmentID)}`,
      {
        method: 'PUT',
        headers: {
          Authorization: `Bearer ${accessToken}`,
          'Content-Type': 'application/octet-stream'
        },
        body: file.ciphertext,
        cache: 'no-store',
        credentials: 'omit',
        redirect: 'error'
      }
    )
    if (!response.ok || response.status !== 204) {
      const error = new Error(`Attachment upload failed (${response.status})`)
      error.status = response.status
      throw error
    }
    const size = BigInt(response.headers.get('x-links-ciphertext-size') || '0')
    const digest = decodeHex(response.headers.get('x-links-ciphertext-sha256') || '')
    const expected = await sha256(file.ciphertext)
    if (size !== file.metadata.ciphertextSizeBytes ||
        !sameBytes(digest, expected) || !sameBytes(digest, file.metadata.ciphertextSHA256)) {
      throw new Error('Attachment upload integrity check failed')
    }
    return {
      attachmentID: file.metadata.attachmentID,
      ciphertextSizeBytes: size,
      ciphertextSHA256: digest
    }
  }
}

function decodeHex(value: string): Uint8Array {
  if (!/^[0-9a-f]{64}$/i.test(value)) return new Uint8Array()
  const bytes = new Uint8Array(32)
  for (let index = 0; index < bytes.length; index += 1) {
    bytes[index] = Number.parseInt(value.slice(index * 2, index * 2 + 2), 16)
  }
  return bytes
}

async function sha256(bytes: Uint8Array): Promise<Uint8Array> {
  return new Uint8Array(await crypto.subtle.digest('SHA-256', bytes.slice().buffer))
}

function sameBytes(left: Uint8Array, right: Uint8Array): boolean {
  return left.length === right.length && left.every((value, index) => value === right[index])
}

export { validateWebFileMetadata, validateWebFileUploadReceipt }
