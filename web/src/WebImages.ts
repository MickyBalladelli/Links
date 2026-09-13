import { requireCanonicalUUID } from './LinksWebClient'
import type { WebMessagingCore, WebTextMessaging } from './WebTextMessaging'

export const WEB_IMAGE_MAX_EDGE = 1600
export const WEB_IMAGE_MAX_PLAINTEXT_BYTES = 32 * 1024 * 1024
export const WEB_IMAGE_TAG_BYTES = 16
export const WEB_IMAGE_MAX_CIPHERTEXT_BYTES =
  WEB_IMAGE_MAX_PLAINTEXT_BYTES + WEB_IMAGE_TAG_BYTES
export const WEB_IMAGE_CACHE_NAME = 'links-encrypted-images-v1'

const BLUR_HASH_ALPHABET =
  '0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz#$%*+,-.:;=?@[]^_{|}~'

export type WebImageMimeType = 'image/webp' | 'image/avif'

export interface WebImageMetadata {
  attachmentID: string
  mimeType: WebImageMimeType
  ciphertextSizeBytes: bigint
  contentKey: Uint8Array
  nonce: Uint8Array
  ciphertextSHA256: Uint8Array
  width: number
  height: number
  blurHash: string
}

export interface WebEncryptedImage {
  metadata: WebImageMetadata
  ciphertext: Uint8Array
}

export interface WebImageUploadReceipt {
  attachmentID: string
  ciphertextSizeBytes: bigint
  ciphertextSHA256: Uint8Array
}

export interface WebImageUploader {
  upload(accessToken: string, image: WebEncryptedImage): Promise<WebImageUploadReceipt>
  download(accessToken: string, metadata: WebImageMetadata): Promise<Uint8Array>
}

export interface WebImageRenderer {
  render(image: Blob, blurHash: string): Promise<void> | void
}

export interface WebPreparedImage {
  image: WebEncryptedImage
  width: number
  height: number
  mimeType: WebImageMimeType
}

export class WebImageProcessor {
  async normalize(source: Blob | ArrayBuffer | Uint8Array): Promise<{
    encoded: Uint8Array
    mimeType: WebImageMimeType
    width: number
    height: number
    rgbPixels: Uint8Array
  }> {
    const sourceBlob = toBlob(source)
    if (sourceBlob.size === 0 || sourceBlob.size > WEB_IMAGE_MAX_PLAINTEXT_BYTES) {
      throw new Error('Invalid Web image')
    }

    let bitmap: ImageBitmap
    try {
      bitmap = await createImageBitmap(sourceBlob, { imageOrientation: 'from-image' })
    } catch {
      throw new Error('Unable to decode Web image')
    }

    try {
      if (bitmap.width < 1 || bitmap.height < 1) {
        throw new Error('Invalid Web image dimensions')
      }
      const { width, height } = resizedDimensions(bitmap.width, bitmap.height)
      const canvas = document.createElement('canvas')
      canvas.width = width
      canvas.height = height
      const context = canvas.getContext('2d', { willReadFrequently: true })
      if (context === null) throw new Error('Canvas unavailable')
      context.drawImage(bitmap, 0, 0, width, height)
      const pixels = context.getImageData(0, 0, width, height).data
      const rgbPixels = new Uint8Array(width * height * 3)
      for (let sourceIndex = 0, targetIndex = 0;
           sourceIndex < pixels.length;
           sourceIndex += 4, targetIndex += 3) {
        rgbPixels[targetIndex] = pixels[sourceIndex]
        rgbPixels[targetIndex + 1] = pixels[sourceIndex + 1]
        rgbPixels[targetIndex + 2] = pixels[sourceIndex + 2]
      }

      const encoded = await encodeLossy(canvas, 'image/avif') ??
        await encodeLossy(canvas, 'image/webp')
      if (encoded === null) throw new Error('Web image encoder unavailable')
      const encodedBytes = new Uint8Array(await encoded.arrayBuffer())
      if (encodedBytes.length === 0 ||
          encodedBytes.length > WEB_IMAGE_MAX_PLAINTEXT_BYTES) {
        throw new Error('Encoded Web image is too large')
      }
      return {
        encoded: encodedBytes,
        mimeType: encoded.type as WebImageMimeType,
        width,
        height,
        rgbPixels
      }
    } finally {
      bitmap.close()
    }
  }
}

export class WebEncryptedImageCache {
  private readonly storage: CacheStorage

  constructor(storage?: CacheStorage) {
    this.storage = storage ?? globalThis.caches
    if (this.storage === undefined) throw new Error('Cache Storage unavailable')
  }

  async read(metadata: WebImageMetadata): Promise<Uint8Array | null> {
    validateWebImageMetadata(metadata)
    const cache = await this.storage.open(WEB_IMAGE_CACHE_NAME)
    const response = await cache.match(cacheKey(metadata.attachmentID))
    if (response === undefined) return null
    const ciphertext = new Uint8Array(await response.arrayBuffer())
    if (!await hasDigest(ciphertext, metadata.ciphertextSHA256) ||
        BigInt(ciphertext.length) !== metadata.ciphertextSizeBytes) {
      await cache.delete(cacheKey(metadata.attachmentID))
      return null
    }
    return ciphertext
  }

  async write(image: WebEncryptedImage): Promise<void> {
    validateEncryptedImage(image)
    if (!await hasDigest(image.ciphertext, image.metadata.ciphertextSHA256)) {
      throw new Error('Invalid Web image ciphertext digest')
    }
    const cache = await this.storage.open(WEB_IMAGE_CACHE_NAME)
    await cache.put(
      cacheKey(image.metadata.attachmentID),
      new Response(image.ciphertext, {
        headers: { 'content-type': 'application/octet-stream' }
      })
    )
  }

  async remove(attachmentID: string): Promise<void> {
    requireCanonicalUUID(attachmentID, 'attachment ID')
    const cache = await this.storage.open(WEB_IMAGE_CACHE_NAME)
    await cache.delete(cacheKey(attachmentID))
  }
}

export interface WebImageSessionOptions {
  core: WebMessagingCore
  messaging: WebTextMessaging
  uploader: WebImageUploader
  cache: WebEncryptedImageCache
  accessToken: () => string
  processor?: WebImageProcessor
}

/** Web image transfer boundary. Plaintext exists only during decode/render. */
export class WebImageSession {
  private readonly core: WebMessagingCore
  private readonly messaging: WebTextMessaging
  private readonly uploader: WebImageUploader
  private readonly cache: WebEncryptedImageCache
  private readonly accessToken: () => string
  private readonly processor: WebImageProcessor

  constructor(options: WebImageSessionOptions) {
    this.core = options.core
    this.messaging = options.messaging
    this.uploader = options.uploader
    this.cache = options.cache
    this.accessToken = options.accessToken
    this.processor = options.processor ?? new WebImageProcessor()
    requireCanonicalUUID(this.core.userID, 'user ID')
    requireCanonicalUUID(this.core.deviceID, 'device ID')
  }

  async prepareAndEncrypt(source: Blob | ArrayBuffer | Uint8Array): Promise<WebPreparedImage> {
    if (this.core.encodeImageBlurHash === undefined ||
        this.core.encryptImage === undefined) {
      throw new Error('Web image core unavailable')
    }
    const normalized = await this.processor.normalize(source)
    try {
      const blurHash = this.core.encodeImageBlurHash(
        normalized.rgbPixels, normalized.width, normalized.height)
      const image = this.core.encryptImage(
        normalized.encoded,
        crypto.randomUUID(),
        normalized.mimeType,
        normalized.width,
        normalized.height,
        blurHash
      )
      validateEncryptedImage(image)
      if (!await hasDigest(image.ciphertext, image.metadata.ciphertextSHA256)) {
        throw new Error('Invalid Web image ciphertext digest')
      }
      return {
        image,
        width: normalized.width,
        height: normalized.height,
        mimeType: normalized.mimeType
      }
    } finally {
      normalized.rgbPixels.fill(0)
    }
  }

  async upload(image: WebEncryptedImage): Promise<WebImageUploadReceipt> {
    validateEncryptedImage(image)
    if (!await hasDigest(image.ciphertext, image.metadata.ciphertextSHA256)) {
      throw new Error('Invalid Web image ciphertext digest')
    }
    const receipt = await this.uploader.upload(this.requireAccessToken(), image)
    if (!matchesReceipt(image.metadata, receipt)) {
      throw new Error('Invalid Web image upload receipt')
    }
    return receipt
  }

  send(
    conversationID: string,
    recipientUserID: string,
    image: WebEncryptedImage,
    receipt: WebImageUploadReceipt
  ): void {
    validateEncryptedImage(image)
    if (!matchesReceipt(image.metadata, receipt)) {
      throw new Error('Invalid Web image upload receipt')
    }
    this.messaging.sendImage(conversationID, recipientUserID, image.metadata, receipt)
  }

  async downloadAndRender(
    metadata: WebImageMetadata,
    renderer: WebImageRenderer
  ): Promise<void> {
    validateWebImageMetadata(metadata)
    if (this.core.decryptImage === undefined) {
      throw new Error('Web image core unavailable')
    }
    let ciphertext = await this.cache.read(metadata)
    if (ciphertext === null) {
      const downloaded = await this.uploader.download(
        this.requireAccessToken(), metadata)
      const image: WebEncryptedImage = { metadata, ciphertext: downloaded }
      validateEncryptedImage(image)
      await this.cache.write(image)
      ciphertext = downloaded
    }

    let plaintext: Uint8Array | null = null
    try {
      plaintext = this.core.decryptImage(metadata, ciphertext)
      if (!(plaintext instanceof Uint8Array) || plaintext.length === 0 ||
          plaintext.length > WEB_IMAGE_MAX_PLAINTEXT_BYTES) {
        throw new Error('Invalid decrypted Web image')
      }
      await renderer.render(new Blob([plaintext], { type: metadata.mimeType }), metadata.blurHash)
    } finally {
      ciphertext.fill(0)
      plaintext?.fill(0)
    }
  }

  private requireAccessToken(): string {
    const token = this.accessToken()
    if (typeof token !== 'string' || token.length === 0) {
      throw new Error('Authenticated Web session required')
    }
    return token
  }
}

/** Small DOM adapter. The caller may use blurHash to paint a placeholder first. */
export class WebImageElementRenderer implements WebImageRenderer {
  constructor(private readonly element: HTMLImageElement) {}

  async render(image: Blob, _blurHash: string): Promise<void> {
    const url = URL.createObjectURL(image)
    try {
      this.element.src = url
      await this.element.decode()
    } finally {
      URL.revokeObjectURL(url)
    }
  }
}

function toBlob(source: Blob | ArrayBuffer | Uint8Array): Blob {
  if (source instanceof Blob) return source
  if (source instanceof ArrayBuffer) return new Blob([source])
  return new Blob([source.slice().buffer])
}

function resizedDimensions(width: number, height: number): { width: number, height: number } {
  const scale = Math.min(1, WEB_IMAGE_MAX_EDGE / Math.max(width, height))
  return {
    width: Math.max(1, Math.round(width * scale)),
    height: Math.max(1, Math.round(height * scale))
  }
}

function encodeLossy(canvas: HTMLCanvasElement, mimeType: WebImageMimeType): Promise<Blob | null> {
  return new Promise(resolve => {
    canvas.toBlob(blob => resolve(blob?.type === mimeType ? blob : null), mimeType, 0.8)
  })
}

export function validateWebImageMetadata(metadata: WebImageMetadata): void {
  requireCanonicalUUID(metadata.attachmentID, 'attachment ID')
  if ((metadata.mimeType !== 'image/webp' && metadata.mimeType !== 'image/avif') ||
      metadata.ciphertextSizeBytes < 17n ||
      metadata.ciphertextSizeBytes > BigInt(WEB_IMAGE_MAX_CIPHERTEXT_BYTES) ||
      metadata.contentKey.length !== 32 || metadata.nonce.length !== 12 ||
      metadata.ciphertextSHA256.length !== 32 ||
      !Number.isInteger(metadata.width) || !Number.isInteger(metadata.height) ||
      metadata.width < 1 || metadata.width > WEB_IMAGE_MAX_EDGE ||
      metadata.height < 1 || metadata.height > WEB_IMAGE_MAX_EDGE ||
      !isBlurHash(metadata.blurHash)) {
    throw new Error('Invalid Web image metadata')
  }
}

export function validateWebImageUploadReceipt(receipt: WebImageUploadReceipt): void {
  requireCanonicalUUID(receipt.attachmentID, 'attachment ID')
  if (receipt.ciphertextSizeBytes < 17n ||
      receipt.ciphertextSizeBytes > BigInt(WEB_IMAGE_MAX_CIPHERTEXT_BYTES) ||
      receipt.ciphertextSHA256.length !== 32) {
    throw new Error('Invalid Web image upload receipt')
  }
}

function validateEncryptedImage(image: WebEncryptedImage): void {
  validateWebImageMetadata(image.metadata)
  if (BigInt(image.ciphertext.length) !== image.metadata.ciphertextSizeBytes ||
      image.ciphertext.length < 17 ||
      image.ciphertext.length > WEB_IMAGE_MAX_CIPHERTEXT_BYTES) {
    throw new Error('Invalid Web image ciphertext')
  }
}

function matchesReceipt(metadata: WebImageMetadata, receipt: WebImageUploadReceipt): boolean {
  validateWebImageUploadReceipt(receipt)
  return receipt.attachmentID === metadata.attachmentID &&
    receipt.ciphertextSizeBytes === metadata.ciphertextSizeBytes &&
    sameBytes(receipt.ciphertextSHA256, metadata.ciphertextSHA256)
}

async function hasDigest(bytes: Uint8Array, expected: Uint8Array): Promise<boolean> {
  return sameBytes(new Uint8Array(await crypto.subtle.digest('SHA-256', bytes)), expected)
}

function sameBytes(left: Uint8Array, right: Uint8Array): boolean {
  return left.length === right.length && left.every((value, index) => value === right[index])
}

function isBlurHash(value: string): boolean {
  return value.length === 28 && value[0] === 'L' &&
    Array.from(value).every(character => BLUR_HASH_ALPHABET.includes(character))
}

function cacheKey(attachmentID: string): Request {
  return new Request(`https://cache.links.invalid/v1/images/${attachmentID}`)
}
