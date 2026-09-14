import {
  WebConnectionManager,
  WEB_MAX_FRAME_BYTES
} from './WebConnectionManager'
import type { WebConnectionState } from './WebConnectionManager'
import { requireCanonicalUUID } from './LinksWebClient'
import type {
  WebCoreTransport,
  WebMessagingCore,
  WebReceivedTextMessage
} from './WebTextMessaging'

export type WebSurfaceKind = 'channel' | 'business' | 'bot'
export type WebSurfaceRole = 'owner' | 'admin' | 'member' | 'subscriber' | 'bot'

export interface WebSurfaceProfile {
  surfaceID: string
  kind: WebSurfaceKind
  role: WebSurfaceRole
  displayName: string
  verified: boolean
}

export function createWebSurfaceProfile(
  surfaceID: string,
  kind: WebSurfaceKind,
  role: WebSurfaceRole,
  displayName: string,
  verified = false
): WebSurfaceProfile {
  requireCanonicalUUID(surfaceID, 'surface ID')
  const nameBytes = new TextEncoder().encode(displayName).byteLength
  if (displayName.length === 0 || nameBytes > 80 || /[\u0000-\u001f\u007f]/u.test(displayName) ||
      !roleAllowed(kind, role)) {
    throw new Error('Invalid surface profile')
  }
  return { surfaceID, kind, role, displayName, verified }
}

export function surfaceCanSend(surface: WebSurfaceProfile): boolean {
  return surface.kind !== 'channel' || surface.role !== 'subscriber'
}

export function surfaceCanPublish(surface: WebSurfaceProfile): boolean {
  return (surface.kind === 'channel' || surface.kind === 'business') &&
    (surface.role === 'owner' || surface.role === 'admin')
}

export interface WebChannelBusinessBotOptions {
  endpoint: string
  core: WebMessagingCore
  accessToken: () => string
  surface: WebSurfaceProfile
  onState?: (state: WebConnectionState) => void
  onMessage?: (surface: WebSurfaceProfile, message: WebReceivedTextMessage) => void
  onFailure?: () => void
}

/** Web host for channel, business, and bot surfaces over shared Rust/WASM core. */
export class WebChannelBusinessBot implements WebCoreTransport {
  static readonly maximumTextBytes = 64 * 1024

  private readonly endpoint: string
  private readonly core: WebMessagingCore
  private readonly accessToken: () => string
  private readonly surfaceProfile: WebSurfaceProfile
  private readonly onStateCallback: (state: WebConnectionState) => void
  private readonly onMessageCallback: (
    surface: WebSurfaceProfile,
    message: WebReceivedTextMessage
  ) => void
  private readonly onFailureCallback: () => void
  private manager: WebConnectionManager | null = null
  private currentState: WebConnectionState = 'stopped'
  private coreFailed = false

  constructor(options: WebChannelBusinessBotOptions) {
    if (typeof options.endpoint !== 'string' || options.core === null ||
        typeof options.accessToken !== 'function') {
      throw new Error('Invalid Web surface session')
    }
    createWebSurfaceProfile(
      options.surface.surfaceID,
      options.surface.kind,
      options.surface.role,
      options.surface.displayName,
      options.surface.verified
    )
    requireCanonicalUUID(options.core.userID, 'user ID')
    requireCanonicalUUID(options.core.deviceID, 'device ID')
    this.endpoint = options.endpoint
    this.core = options.core
    this.accessToken = options.accessToken
    this.surfaceProfile = options.surface
    this.onStateCallback = options.onState ?? (() => {})
    this.onMessageCallback = options.onMessage ?? (() => {})
    this.onFailureCallback = options.onFailure ?? (() => {})
  }

  get surface(): WebSurfaceProfile {
    return this.surfaceProfile
  }

  get state(): WebConnectionState {
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
      onState: state => this.handleState(manager, state),
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

  sendText(conversationID: string, text: string): void {
    requireCanonicalUUID(conversationID, 'conversation ID')
    if (!surfaceCanSend(this.surfaceProfile) || typeof text !== 'string' || text.length === 0 ||
        new TextEncoder().encode(text).byteLength > WebChannelBusinessBot.maximumTextBytes) {
      throw new Error('Surface cannot send this text')
    }
    const manager = this.manager
    if (this.coreFailed || this.currentState !== 'ready' || manager === null ||
        !manager.isConnected) {
      throw new Error('Web surface session is not connected')
    }
    if (this.core.sendSurfaceText !== undefined) {
      this.core.sendSurfaceText(this.surfaceProfile.surfaceID, conversationID, text, manager)
    } else {
      this.core.sendText(conversationID, this.surfaceProfile.surfaceID, text, manager)
    }
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
      this.core.handleServerFrame(frame, manager, false, message => {
        this.notify(() => this.onMessageCallback(this.surfaceProfile, message))
      })
    } catch {
      this.coreFailed = true
      this.setState('failed')
      this.manager = null
      manager.stop()
      this.notify(this.onFailureCallback)
    }
  }

  private handleState(manager: WebConnectionManager, state: WebConnectionState): void {
    if (this.manager !== manager) return
    this.setState(this.coreFailed && state === 'stopped' ? 'failed' : state)
  }

  private handleFailure(manager: WebConnectionManager): void {
    if (this.manager !== manager || this.coreFailed) return
    this.setState('failed')
    this.notify(this.onFailureCallback)
  }

  private setState(state: WebConnectionState): void {
    this.currentState = state
    this.notify(() => this.onStateCallback(state))
  }

  private notify(callback: () => void): void {
    try {
      callback()
    } catch {
      // UI callbacks must not interrupt the shared core lifecycle.
    }
  }
}

function roleAllowed(kind: WebSurfaceKind, role: WebSurfaceRole): boolean {
  if (kind === 'channel') return role === 'owner' || role === 'admin' || role === 'subscriber'
  if (kind === 'business') return role === 'owner' || role === 'admin' || role === 'member'
  return role === 'bot'
}
