import {
  WebConnectionManager,
  WEB_MAX_FRAME_BYTES
} from './WebConnectionManager'
import type { WebConnectionState } from './WebConnectionManager'
import { requireCanonicalUUID } from './LinksWebClient'

export interface WebCoreTransport {
  send(frame: Uint8Array): boolean
}

export type WebCoreFrameResult = 'pending' | 'recoveryComplete'

export interface WebReceivedTextMessage {
  conversationID: string
  senderDeviceID: string
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
    } catch {
      this.coreFailed = true
      this.setState('failed')
      this.manager = null
      manager.stop()
      this.notify(this.onFailure)
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
