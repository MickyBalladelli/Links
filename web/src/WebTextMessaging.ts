import {
  WebConnectionManager,
  WEB_MAX_FRAME_BYTES
} from './WebConnectionManager'
import type { WebConnectionState } from './WebConnectionManager'
import { requireCanonicalUUID } from './LinksWebClient'
import type {
  WebImageMetadata,
  WebImageUploadReceipt,
  WebEncryptedImage
} from './WebImages'
import type { WebLargeFileMetadata, WebLargeFileUploadReceipt } from './WebLargeFiles'
import { validateWebImageMetadata, validateWebImageUploadReceipt } from './WebImages'

export interface WebFileMetadata {
  attachmentID: string
  mimeType: string
  fileName: string
  ciphertextSizeBytes: bigint
  contentKey: Uint8Array
  nonce: Uint8Array
  ciphertextSHA256: Uint8Array
}

export interface WebFileUploadReceipt {
  attachmentID: string
  ciphertextSizeBytes: bigint
  ciphertextSHA256: Uint8Array
}

export interface WebCoreTransport {
  send(frame: Uint8Array): boolean
}

export type WebCoreFrameResult = 'pending' | 'recoveryComplete'

export interface WebReceivedTextMessage {
  conversationID: string
  senderDeviceID: string
  senderUserID: string
  text: string
  sequenceID: bigint
  sentAtMs: bigint
}

/** Adapter for the shared Rust client core and its durable browser stores. */
export interface WebMessagingCore extends WebCoreTransport {
  readonly userID: string
  readonly deviceID: string
  durableCursor(): bigint
  createHello(accessToken: string, lastSeenCursor: bigint): Uint8Array
  /** Call only after the frame has been received over the authenticated socket. */
  handleServerFrame(
    frame: Uint8Array,
    transport: WebCoreTransport,
    fullSync: boolean,
    onTextMessage: (message: WebReceivedTextMessage) => void
  ): WebCoreFrameResult
  /** Persist the outbox record before reporting transport success. */
  sendText(
    conversationID: string,
    recipientUserID: string,
    text: string,
    transport: WebCoreTransport
  ): void
  /** Surface-aware route, channel publishers can use broadcast MLS here. */
  sendSurfaceText?(
    surfaceID: string,
    conversationID: string,
    text: string,
    transport: WebCoreTransport
  ): void
  encodeImageBlurHash?(
    rgbPixels: Uint8Array,
    width: number,
    height: number
  ): string
  encryptImage?(
    image: Uint8Array,
    attachmentID: string,
    mimeType: string,
    width: number,
    height: number,
    blurHash: string
  ): WebEncryptedImage
  decryptImage?(metadata: WebImageMetadata, ciphertext: Uint8Array): Uint8Array
  sendImage?(
    conversationID: string,
    recipientUserID: string,
    metadata: WebImageMetadata,
    receipt: WebImageUploadReceipt,
    transport: WebCoreTransport
  ): void
  sendLargeFile?(
    conversationID: string,
    recipientUserID: string,
    metadata: WebLargeFileMetadata,
    receipt: WebLargeFileUploadReceipt,
    transport: WebCoreTransport
  ): void
  sendFile?(
    conversationID: string,
    recipientUserID: string,
    metadata: WebFileMetadata,
    receipt: WebFileUploadReceipt,
    transport: WebCoreTransport
  ): void
}

export interface WebTextMessagingOptions {
  endpoint: string
  core: WebMessagingCore
  /** Read the current bearer from memory when each Hello is created. */
  accessToken: () => string
  onState?: (state: WebConnectionState) => void
  onTextMessage?: (message: WebReceivedTextMessage) => void
  onFailure?: () => void
}

export type WebTextMessagingState = WebConnectionState

export const WEB_MAX_TEXT_BYTES = 64 * 1024

/** Web host for shared-core encrypted one-to-one text sync. */
export class WebTextMessaging implements WebCoreTransport {
  private readonly endpoint: string
  private readonly core: WebMessagingCore
  private readonly accessToken: () => string
  private readonly onStateCallback: (state: WebConnectionState) => void
  private readonly onTextMessage: (message: WebReceivedTextMessage) => void
  private readonly onFailure: () => void

  private manager: WebConnectionManager | null = null
  private currentState: WebConnectionState = 'stopped'
  private coreFailed = false

  constructor(options: WebTextMessagingOptions) {
    if (typeof options.endpoint !== 'string' || options.core === null ||
        typeof options.accessToken !== 'function') {
      throw new Error('Invalid Web text session')
    }
    requireCanonicalUUID(options.core.userID, 'user ID')
    requireCanonicalUUID(options.core.deviceID, 'device ID')
    this.endpoint = options.endpoint
    this.core = options.core
    this.accessToken = options.accessToken
    this.onStateCallback = options.onState ?? (() => {})
    this.onTextMessage = options.onTextMessage ?? (() => {})
    this.onFailure = options.onFailure ?? (() => {})
  }

  get state(): WebTextMessagingState {
    return this.currentState
  }

  get isConnected(): boolean {
    return this.currentState === 'ready' && this.manager?.isConnected === true
  }

  start(): void {
    if (this.manager !== null || this.coreFailed) return

    let manager: WebConnectionManager
    manager = new WebConnectionManager({
      endpoint: this.endpoint,
      helloProvider: () => this.createHello(),
      onFrame: frame => this.handleFrame(manager, frame),
      onState: state => this.handleManagerState(manager, state),
      onFailure: () => this.handleFailure(manager)
    })
    this.manager = manager
    this.coreFailed = false
    this.setState('connecting')
    manager.start()
  }

  stop(): void {
    const manager = this.manager
    this.manager = null
    this.coreFailed = false
    manager?.stop()
    this.setState('stopped')
  }

  shutdown(): void {
    const manager = this.manager
    this.manager = null
    this.coreFailed = false
    manager?.shutdown()
    this.setState('stopped')
  }

  send(frame: Uint8Array): boolean {
    return this.manager?.send(frame) === true
  }

  /** Encrypt and queue one direct text message through the shared core. */
  sendText(conversationID: string, recipientUserID: string, text: string): void {
    requireCanonicalUUID(conversationID, 'conversation ID')
    requireCanonicalUUID(recipientUserID, 'recipient user ID')
    if (typeof text !== 'string' || text.length === 0 ||
        new TextEncoder().encode(text).byteLength > WEB_MAX_TEXT_BYTES) {
      throw new Error('Invalid text message')
    }
    const manager = this.manager
    if (this.coreFailed || this.currentState !== 'ready' || manager === null ||
        !manager.isConnected) {
      throw new Error('Web text session is not connected')
    }
    this.core.sendText(conversationID, recipientUserID, text, manager)
  }

  /** Send private image metadata only after the exact ciphertext receipt. */
  sendImage(
    conversationID: string,
    recipientUserID: string,
    metadata: WebImageMetadata,
    receipt: WebImageUploadReceipt
  ): void {
    requireCanonicalUUID(conversationID, 'conversation ID')
    requireCanonicalUUID(recipientUserID, 'recipient user ID')
    validateWebImageMetadata(metadata)
    validateWebImageUploadReceipt(receipt)
    if (receipt.attachmentID !== metadata.attachmentID ||
        receipt.ciphertextSizeBytes !== metadata.ciphertextSizeBytes ||
        !sameBytes(receipt.ciphertextSHA256, metadata.ciphertextSHA256)) {
      throw new Error('Invalid image upload receipt')
    }
    const manager = this.manager
    if (this.coreFailed || this.currentState !== 'ready' || manager === null ||
        !manager.isConnected || this.core.sendImage === undefined) {
      throw new Error('Web image session is not connected')
    }
    this.core.sendImage(conversationID, recipientUserID, metadata, receipt, manager)
  }

  /** Send private file metadata only after the exact ciphertext receipt. */
  sendFile(
    conversationID: string,
    recipientUserID: string,
    metadata: WebFileMetadata,
    receipt: WebFileUploadReceipt
  ): void {
    requireCanonicalUUID(conversationID, 'conversation ID')
    requireCanonicalUUID(recipientUserID, 'recipient user ID')
    validateWebFileMetadata(metadata)
    validateWebFileUploadReceipt(receipt)
    if (!matchesFileReceipt(metadata, receipt)) {
      throw new Error('Invalid file upload receipt')
    }
    const manager = this.manager
    if (this.coreFailed || this.currentState !== 'ready' || manager === null ||
        !manager.isConnected || this.core.sendFile === undefined) {
      throw new Error('Web file session is not connected')
    }
    this.core.sendFile(conversationID, recipientUserID, metadata, receipt, manager)
  }

  private createHello(): Uint8Array {
    const token = this.accessToken()
    if (typeof token !== 'string' || token.length === 0) {
      throw new Error('Authenticated Web session required')
    }
    const hello = this.core.createHello(token, this.core.durableCursor())
    if (!(hello instanceof Uint8Array) || hello.byteLength === 0 ||
        hello.byteLength > WEB_MAX_FRAME_BYTES) {
      throw new Error('Invalid Web Hello frame')
    }
    return hello
  }

  private handleFrame(manager: WebConnectionManager, frame: Uint8Array): void {
    if (this.manager !== manager || this.coreFailed) return
    try {
      this.core.handleServerFrame(
        frame,
        manager,
        false,
        message => this.notify(() => this.onTextMessage(message))
      )
    } catch (error) {
      // Leave the core running. Stopping here froze the status on
      // Reconnecting and never opened another socket.
      this.setState('failed')
      this.notify(this.onFailure)
      throw error
    }
  }

  private handleManagerState(manager: WebConnectionManager, state: WebConnectionState): void {
    if (this.manager !== manager) return
    this.setState(this.coreFailed && state === 'stopped' ? 'failed' : state)
  }

  private handleFailure(manager: WebConnectionManager): void {
    if (this.manager !== manager || this.coreFailed) return
    this.setState('failed')
    this.notify(this.onFailure)
  }

  private setState(state: WebConnectionState): void {
    this.currentState = state
    this.notify(() => this.onStateCallback(state))
  }

  private notify(callback: () => void): void {
    try {
      callback()
    } catch {
      // UI callbacks must not break crypto, cursor commits or reconnects.
    }
  }
}

function sameBytes(left: Uint8Array, right: Uint8Array): boolean {
  return left.length === right.length && left.every((value, index) => value === right[index])
}

const WEB_FILE_MAX_CIPHERTEXT_BYTES = 20 * 1024 * 1024 + 16

export function validateWebFileMetadata(metadata: WebFileMetadata): void {
  requireCanonicalUUID(metadata.attachmentID, 'attachment ID')
  if (typeof metadata.mimeType !== 'string' || metadata.mimeType.length === 0 ||
      typeof metadata.fileName !== 'string' || metadata.fileName.trim().length === 0 ||
      metadata.ciphertextSizeBytes < 17n ||
      metadata.ciphertextSizeBytes > BigInt(WEB_FILE_MAX_CIPHERTEXT_BYTES) ||
      metadata.contentKey.length !== 32 || metadata.nonce.length !== 12 ||
      metadata.ciphertextSHA256.length !== 32) {
    throw new Error('Invalid Web file metadata')
  }
}

export function validateWebFileUploadReceipt(receipt: WebFileUploadReceipt): void {
  requireCanonicalUUID(receipt.attachmentID, 'attachment ID')
  if (receipt.ciphertextSizeBytes < 17n ||
      receipt.ciphertextSizeBytes > BigInt(WEB_FILE_MAX_CIPHERTEXT_BYTES) ||
      receipt.ciphertextSHA256.length !== 32) {
    throw new Error('Invalid Web file upload receipt')
  }
}

function matchesFileReceipt(
  metadata: WebFileMetadata,
  receipt: WebFileUploadReceipt
): boolean {
  return receipt.attachmentID === metadata.attachmentID &&
    receipt.ciphertextSizeBytes === metadata.ciphertextSizeBytes &&
    sameBytes(receipt.ciphertextSHA256, metadata.ciphertextSHA256)
}
