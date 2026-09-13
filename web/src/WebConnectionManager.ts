export type WebConnectionState = 'stopped' | 'connecting' | 'ready' | 'failed'

export interface WebConnectionManagerOptions {
  endpoint: string
  /** Return one complete protobuf ClientFrame containing Hello. */
  helloProvider: () => Uint8Array
  /** Receive complete binary protobuf ServerFrame messages. */
  onFrame: (frame: Uint8Array) => void
  onState?: (state: WebConnectionState) => void
  onConnected?: () => void
  onDisconnected?: () => void
  onFailure?: () => void
}

export const WEB_HEARTBEAT_INTERVAL_MS = 30_000
export const WEB_HELLO_DEADLINE_MS = 5_000
export const WEB_INITIAL_BACKOFF_MS = 1_000
export const WEB_MAX_BACKOFF_MS = 30_000
export const WEB_STABLE_CONNECTION_MS = 30_000
export const WEB_MAX_FRAME_BYTES = 1024 * 1024

const CLOSE_NORMAL = 1000
const CLOSE_PROTOCOL_ERROR = 1002
const CLOSE_UNSUPPORTED_DATA = 1003

function validateEndpoint(endpoint: string): string {
  let parsed: URL
  try {
    parsed = new URL(endpoint)
  } catch {
    throw new Error('Invalid WebSocket endpoint')
  }
  if (parsed.protocol !== 'wss:' || parsed.hostname.length === 0 ||
      parsed.username.length !== 0 || parsed.password.length !== 0 ||
      parsed.search.length !== 0 || parsed.hash.length !== 0 ||
      parsed.pathname !== '/v1/connect') {
    throw new Error('Invalid WebSocket endpoint')
  }
  return endpoint
}

/** Browser WebSocket lifecycle for the binary links.v1 transport. */
export class WebConnectionManager {
  private readonly endpoint: string
  private readonly helloProvider: () => Uint8Array
  private readonly onFrame: (frame: Uint8Array) => void
  private readonly onState: (state: WebConnectionState) => void
  private readonly onConnected: () => void
  private readonly onDisconnected: () => void
  private readonly onFailure: () => void

  private socket: WebSocket | null = null
  private reconnectTimer: ReturnType<typeof setTimeout> | null = null
  private helloTimer: ReturnType<typeof setTimeout> | null = null
  private stableTimer: ReturnType<typeof setTimeout> | null = null
  private heartbeatTimer: ReturnType<typeof setInterval> | null = null
  private backoffMs = WEB_INITIAL_BACKOFF_MS
  private generation = 0
  private started = false
  private shutdownRequested = false
  private helloQueued = false
  private currentState: WebConnectionState = 'stopped'

  constructor(options: WebConnectionManagerOptions) {
    this.endpoint = validateEndpoint(options.endpoint)
    if (typeof options.helloProvider !== 'function' || typeof options.onFrame !== 'function') {
      throw new Error('Invalid WebSocket callbacks')
    }
    this.helloProvider = options.helloProvider
    this.onFrame = options.onFrame
    this.onState = options.onState ?? (() => {})
    this.onConnected = options.onConnected ?? (() => {})
    this.onDisconnected = options.onDisconnected ?? (() => {})
    this.onFailure = options.onFailure ?? (() => {})
  }

  get state(): WebConnectionState {
    return this.currentState
  }

  get isConnected(): boolean {
    return this.started && this.helloQueued && this.socket?.readyState === WebSocket.OPEN
  }

  start(): void {
    if (this.shutdownRequested || this.started) return
    this.started = true
    this.backoffMs = WEB_INITIAL_BACKOFF_MS
    this.setState('connecting')
    this.connectNow()
  }

  /** Stop reconnecting. The manager can be started again. */
  stop(): void {
    if (!this.started && this.socket === null) return
    this.started = false
    this.generation += 1
    this.clearReconnectTimer()
    this.clearSocketTimers()
    const active = this.socket
    this.socket = null
    this.helloQueued = false
    if (active !== null && active.readyState !== WebSocket.CLOSED) {
      active.close(CLOSE_NORMAL, 'client shutdown')
    }
    this.setState('stopped')
  }

  /** Permanently release the manager. */
  shutdown(): void {
    if (this.shutdownRequested) return
    this.shutdownRequested = true
    this.stop()
  }

  /** Send one complete binary protobuf frame. Tokens stay inside Hello bytes. */
  send(frame: Uint8Array): boolean {
    if (frame.byteLength === 0 || frame.byteLength > WEB_MAX_FRAME_BYTES || !this.isConnected) {
      return false
    }
    const active = this.socket
    if (active === null) return false
    try {
      active.send(frame)
      return true
    } catch {
      this.failSocket(active, true, CLOSE_PROTOCOL_ERROR)
      return false
    }
  }

  private connectNow(): void {
    if (!this.started || this.shutdownRequested || this.socket !== null) return
    const attempt = ++this.generation
    let socket: WebSocket
    try {
      socket = new WebSocket(this.endpoint, ['links.v1'])
      socket.binaryType = 'arraybuffer'
    } catch {
      this.reportFailure()
      this.scheduleReconnect()
      return
    }
    this.socket = socket
    this.helloQueued = false

    socket.onopen = () => {
      if (!this.isCurrent(attempt, socket)) {
        socket.close(CLOSE_NORMAL, 'stale connection')
        return
      }
      if (socket.protocol !== 'links.v1') {
        this.failSocket(socket, true, CLOSE_PROTOCOL_ERROR)
        return
      }
      this.scheduleHelloDeadline(attempt, socket)
      let hello: Uint8Array
      try {
        hello = this.helloProvider()
      } catch {
        this.failSocket(socket, true, CLOSE_PROTOCOL_ERROR)
        return
      }
      if (!(hello instanceof Uint8Array) ||
          hello.byteLength === 0 || hello.byteLength > WEB_MAX_FRAME_BYTES) {
        this.failSocket(socket, true, CLOSE_PROTOCOL_ERROR)
        return
      }
      try {
        socket.send(hello)
      } catch {
        this.failSocket(socket, true, CLOSE_PROTOCOL_ERROR)
        return
      }
      if (!this.isCurrent(attempt, socket)) return
      this.helloQueued = true
      this.clearHelloTimer()
      this.installHeartbeatCheck(attempt, socket)
      this.installStableReset(attempt, socket)
      this.setState('ready')
      this.notify(this.onConnected)
    }

    socket.onmessage = event => {
      if (!this.isCurrent(attempt, socket)) return
      const frame = this.binaryFrame(event.data)
      if (frame === null || frame.byteLength === 0 || frame.byteLength > WEB_MAX_FRAME_BYTES) {
        this.failSocket(socket, true, frame === null ? CLOSE_UNSUPPORTED_DATA : CLOSE_PROTOCOL_ERROR)
        return
      }
      try {
        this.onFrame(frame.slice())
      } catch {
        this.failSocket(socket, true, CLOSE_PROTOCOL_ERROR)
      }
    }

    socket.onerror = () => {
      if (this.isCurrent(attempt, socket)) this.failSocket(socket, true)
    }

    socket.onclose = () => {
      if (!this.isCurrent(attempt, socket)) return
      this.clearSocketTimers()
      this.socket = null
      this.helloQueued = false
      this.setState(this.started ? 'connecting' : 'stopped')
      if (this.started) {
        this.notify(this.onDisconnected)
        this.scheduleReconnect()
      }
    }
  }

  private binaryFrame(data: unknown): Uint8Array | null {
    if (data instanceof ArrayBuffer) return new Uint8Array(data)
    if (ArrayBuffer.isView(data)) {
      return new Uint8Array(data.buffer, data.byteOffset, data.byteLength)
    }
    return null
  }

  private isCurrent(attempt: number, socket: WebSocket): boolean {
    return this.started && !this.shutdownRequested && attempt === this.generation &&
      this.socket === socket
  }

  private failSocket(socket: WebSocket, reportFailure: boolean, closeCode = CLOSE_PROTOCOL_ERROR): void {
    if (this.socket !== socket) return
    if (reportFailure) this.reportFailure()
    this.generation += 1
    this.clearSocketTimers()
    this.socket = null
    this.helloQueued = false
    if (socket.readyState === WebSocket.OPEN || socket.readyState === WebSocket.CONNECTING) {
      try {
        socket.close(closeCode, 'protocol error')
      } catch {
        // The close event or reconnect path owns final cleanup.
      }
    }
    if (this.started) {
      this.setState('connecting')
      this.notify(this.onDisconnected)
      this.scheduleReconnect()
    }
  }

  private scheduleReconnect(): void {
    if (!this.started || this.reconnectTimer !== null) return
    const ceiling = Math.min(WEB_MAX_BACKOFF_MS, this.backoffMs)
    const delay = Math.floor(Math.random() * (ceiling + 1))
    this.backoffMs = Math.min(WEB_MAX_BACKOFF_MS, Math.max(WEB_INITIAL_BACKOFF_MS, this.backoffMs * 2))
    this.reconnectTimer = setTimeout(() => {
      this.reconnectTimer = null
      this.connectNow()
    }, delay)
  }

  private scheduleHelloDeadline(attempt: number, socket: WebSocket): void {
    this.clearHelloTimer()
    this.helloTimer = setTimeout(() => {
      this.helloTimer = null
      if (this.isCurrent(attempt, socket) && !this.helloQueued) {
        this.failSocket(socket, true, CLOSE_PROTOCOL_ERROR)
      }
    }, WEB_HELLO_DEADLINE_MS)
  }

  private installHeartbeatCheck(attempt: number, socket: WebSocket): void {
    this.heartbeatTimer = setInterval(() => {
      if (!this.isCurrent(attempt, socket)) return
      // Browser WebSocket automatically answers server ping frames. The API
      // exposes no ping/pong callbacks, so only detect a dead local socket;
      // server liveness remains authoritative through close/error events.
      if (socket.readyState !== WebSocket.OPEN) this.failSocket(socket, true)
    }, WEB_HEARTBEAT_INTERVAL_MS)
  }

  private installStableReset(attempt: number, socket: WebSocket): void {
    this.clearStableTimer()
    this.stableTimer = setTimeout(() => {
      if (this.isCurrent(attempt, socket)) this.backoffMs = WEB_INITIAL_BACKOFF_MS
      this.stableTimer = null
    }, WEB_STABLE_CONNECTION_MS)
  }

  private clearReconnectTimer(): void {
    if (this.reconnectTimer === null) return
    clearTimeout(this.reconnectTimer)
    this.reconnectTimer = null
  }

  private clearHelloTimer(): void {
    if (this.helloTimer === null) return
    clearTimeout(this.helloTimer)
    this.helloTimer = null
  }

  private clearStableTimer(): void {
    if (this.stableTimer === null) return
    clearTimeout(this.stableTimer)
    this.stableTimer = null
  }

  private clearSocketTimers(): void {
    this.clearHelloTimer()
    this.clearStableTimer()
    if (this.heartbeatTimer !== null) clearInterval(this.heartbeatTimer)
    this.heartbeatTimer = null
  }

  private setState(state: WebConnectionState): void {
    this.currentState = state
    this.notify(() => this.onState(state))
  }

  private reportFailure(): void {
    this.setState('failed')
    this.notify(this.onFailure)
  }

  private notify(callback: () => void): void {
    try {
      callback()
    } catch {
      // UI callbacks must not break socket lifecycle or reconnect handling.
    }
  }
}
