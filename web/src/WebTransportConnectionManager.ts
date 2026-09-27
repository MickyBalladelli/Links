import type {
  WebConnectionManagerOptions,
  WebConnectionState,
  WebConnectionTransport
} from './WebConnectionManager'
import {
  WEB_HEARTBEAT_INTERVAL_MS,
  WEB_HELLO_DEADLINE_MS,
  WEB_INITIAL_BACKOFF_MS,
  WEB_MAX_BACKOFF_MS,
  WEB_MAX_FRAME_BYTES,
  WEB_STABLE_CONNECTION_MS
} from './WebConnectionManager'

const WEBTRANSPORT_FRAME_HEADER_BYTES = 4

interface WebTransportBidirectionalStreamLike {
  readable: ReadableStream<Uint8Array>
  writable: WritableStream<Uint8Array>
}

interface WebTransportLike {
  readonly ready: Promise<void>
  readonly closed: Promise<unknown>
  createBidirectionalStream(): Promise<WebTransportBidirectionalStreamLike>
  close(): void
}

interface WebTransportConstructorLike {
  new (url: string, options?: {
    congestionControl?: 'throughput' | 'low-latency'
  }): WebTransportLike
}

interface WebTransportGlobal {
  WebTransport?: WebTransportConstructorLike
}

const CLOSE_PROTOCOL_ERROR = 'protocol error'

/** True when the browser exposes the secure WebTransport HTTP/3 API. */
export function supportsWebTransport(): boolean {
  const runtime = globalThis as WebTransportGlobal
  return typeof runtime.WebTransport === 'function'
}

/** Validate the HTTPS endpoint used by the WebTransport signaling adapter. */
export function validateWebTransportEndpoint(endpoint: string): string {
  let parsed: URL
  try {
    parsed = new URL(endpoint)
  } catch {
    throw new Error('Invalid WebTransport endpoint')
  }
  if (parsed.protocol !== 'https:' || parsed.hostname.length === 0 ||
      parsed.username.length !== 0 || parsed.password.length !== 0 ||
      parsed.search.length !== 0 || parsed.hash.length !== 0 ||
      parsed.pathname !== '/v1/connect') {
    throw new Error('Invalid WebTransport endpoint')
  }
  return endpoint
}

/**
 * Reliable WebTransport fallback for the authenticated links.v1 signaling
 * stream. QUIC handles loss recovery; each protobuf frame is length-prefixed
 * because a bidirectional stream has no WebSocket-style message boundaries.
 */
export class WebTransportConnectionManager implements WebConnectionTransport {
  private readonly endpoint: string
  private readonly helloProvider: () => Uint8Array
  private readonly onFrame: (frame: Uint8Array) => void
  private readonly onState: (state: WebConnectionState) => void
  private readonly onConnected: () => void
  private readonly onDisconnected: () => void
  private readonly onFailure: () => void

  private transport: WebTransportLike | null = null
  private reader: ReadableStreamDefaultReader<Uint8Array> | null = null
  private writer: WritableStreamDefaultWriter<Uint8Array> | null = null
  private reconnectTimer: ReturnType<typeof setTimeout> | null = null
  private helloTimer: ReturnType<typeof setTimeout> | null = null
  private stableTimer: ReturnType<typeof setTimeout> | null = null
  private heartbeatTimer: ReturnType<typeof setInterval> | null = null
  private backoffMs = WEB_INITIAL_BACKOFF_MS
  private generation = 0
  private started = false
  private shutdownRequested = false
  private helloQueued = false
  private authenticated = false
  private receiveBuffer = new Uint8Array(0)
  private expectedFrameBytes: number | null = null
  private currentState: WebConnectionState = 'stopped'

  constructor(options: WebConnectionManagerOptions) {
    this.endpoint = validateWebTransportEndpoint(options.endpoint)
    if (typeof options.helloProvider !== 'function' || typeof options.onFrame !== 'function') {
      throw new Error('Invalid WebTransport callbacks')
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
    return this.started && this.authenticated && this.transport !== null &&
      this.writer !== null
  }

  start(): void {
    if (this.shutdownRequested || this.started) return
    this.started = true
    this.backoffMs = WEB_INITIAL_BACKOFF_MS
    this.setState('connecting')
    this.connectNow()
  }

  stop(): void {
    if (!this.started && this.transport === null) return
    this.started = false
    this.generation += 1
    this.clearReconnectTimer()
    this.clearTransportTimers()
    const active = this.transport
    this.transport = null
    this.reader = null
    this.writer = null
    this.helloQueued = false
    this.authenticated = false
    this.resetReceiveBuffer()
    if (active !== null) {
      try {
        active.close()
      } catch {
        // Closing is best effort; the generation fence owns cleanup.
      }
    }
    this.setState('stopped')
  }

  shutdown(): void {
    if (this.shutdownRequested) return
    this.shutdownRequested = true
    this.stop()
  }

  /** Send one length-prefixed protobuf frame on the reliable QUIC stream. */
  send(frame: Uint8Array): boolean {
    if (frame.byteLength === 0 || frame.byteLength > WEB_MAX_FRAME_BYTES ||
        !this.isConnected) return false
    const writer = this.writer
    const transport = this.transport
    if (writer === null || transport === null) return false
    const framed = new Uint8Array(WEBTRANSPORT_FRAME_HEADER_BYTES + frame.byteLength)
    new DataView(framed.buffer).setUint32(0, frame.byteLength, false)
    framed.set(frame, WEBTRANSPORT_FRAME_HEADER_BYTES)
    void this.writeFrame(writer, framed, transport, this.generation)
    return true
  }

  private connectNow(): void {
    if (!this.started || this.shutdownRequested || this.transport !== null) return
    const attempt = ++this.generation
    const runtime = globalThis as WebTransportGlobal
    const Constructor = runtime.WebTransport
    if (Constructor === undefined) {
      this.reportFailure()
      this.scheduleReconnect()
      return
    }

    let transport: WebTransportLike
    try {
      transport = new Constructor(this.endpoint, { congestionControl: 'low-latency' })
    } catch {
      this.reportFailure()
      this.scheduleReconnect()
      return
    }
    this.transport = transport
    this.reader = null
    this.writer = null
    this.helloQueued = false
    this.authenticated = false
    this.resetReceiveBuffer()
    void this.openStream(attempt, transport)
    void transport.closed.then(
      () => this.handleClosed(attempt, transport),
      () => this.handleClosed(attempt, transport)
    )
  }

  private async openStream(attempt: number, transport: WebTransportLike): Promise<void> {
    try {
      await transport.ready
      if (!this.isCurrent(attempt, transport)) return
      const stream = await transport.createBidirectionalStream()
      if (!this.isCurrent(attempt, transport)) return
      const reader = stream.readable.getReader()
      const writer = stream.writable.getWriter()
      this.reader = reader
      this.writer = writer
      this.scheduleHelloDeadline(attempt, transport)
      const hello = this.helloProvider()
      if (!(hello instanceof Uint8Array) || hello.byteLength === 0 ||
          hello.byteLength > WEB_MAX_FRAME_BYTES) {
        throw new Error('Invalid WebTransport Hello frame')
      }
      const framedHello = new Uint8Array(WEBTRANSPORT_FRAME_HEADER_BYTES + hello.byteLength)
      new DataView(framedHello.buffer).setUint32(0, hello.byteLength, false)
      framedHello.set(hello, WEBTRANSPORT_FRAME_HEADER_BYTES)
      await this.writeFrame(writer, framedHello, transport, attempt)
      if (!this.isCurrent(attempt, transport)) return
      this.helloQueued = true
      this.installHeartbeatCheck(attempt, transport, writer)
      this.installStableReset(attempt, transport)
      await this.readLoop(attempt, transport, reader)
    } catch {
      if (this.isCurrent(attempt, transport)) this.failTransport(transport, true)
    }
  }

  private async writeFrame(
    writer: WritableStreamDefaultWriter<Uint8Array>,
    frame: Uint8Array,
    transport: WebTransportLike,
    attempt: number
  ): Promise<void> {
    await writer.ready
    await writer.write(frame)
    if (!this.isCurrent(attempt, transport)) throw new Error('Stale WebTransport')
  }

  private async readLoop(
    attempt: number,
    transport: WebTransportLike,
    reader: ReadableStreamDefaultReader<Uint8Array>
  ): Promise<void> {
    while (this.isCurrent(attempt, transport)) {
      const result = await reader.read()
      if (result.done) throw new Error('WebTransport stream closed')
      if (!(result.value instanceof Uint8Array) || result.value.byteLength === 0) {
        throw new Error(CLOSE_PROTOCOL_ERROR)
      }
      this.consumeBytes(result.value)
    }
  }

  private consumeBytes(chunk: Uint8Array): void {
    const combined = new Uint8Array(this.receiveBuffer.byteLength + chunk.byteLength)
    combined.set(this.receiveBuffer)
    combined.set(chunk, this.receiveBuffer.byteLength)

    let offset = 0
    while (true) {
      if (this.expectedFrameBytes === null) {
        if (combined.byteLength - offset < WEBTRANSPORT_FRAME_HEADER_BYTES) break
        const length = new DataView(combined.buffer, combined.byteOffset + offset, 4)
          .getUint32(0, false)
        if (length === 0 || length > WEB_MAX_FRAME_BYTES) throw new Error(CLOSE_PROTOCOL_ERROR)
        this.expectedFrameBytes = length
        offset += WEBTRANSPORT_FRAME_HEADER_BYTES
      }
      const expected = this.expectedFrameBytes
      if (expected === null || combined.byteLength - offset < expected) break
      this.onFrame(combined.slice(offset, offset + expected))
      if (!this.authenticated) {
        this.authenticated = true
        this.clearHelloTimer()
        this.setState('ready')
        this.notify(this.onConnected)
      }
      offset += expected
      this.expectedFrameBytes = null
    }
    this.receiveBuffer = combined.slice(offset)
    if (this.receiveBuffer.byteLength > WEB_MAX_FRAME_BYTES + WEBTRANSPORT_FRAME_HEADER_BYTES) {
      throw new Error(CLOSE_PROTOCOL_ERROR)
    }
  }

  private handleClosed(attempt: number, transport: WebTransportLike): void {
    if (this.isCurrent(attempt, transport)) this.failTransport(transport, true)
  }

  private isCurrent(attempt: number, transport: WebTransportLike): boolean {
    return this.started && !this.shutdownRequested && attempt === this.generation &&
      this.transport === transport
  }

  private failTransport(transport: WebTransportLike, reportFailure: boolean): void {
    if (this.transport !== transport) return
    if (reportFailure) this.reportFailure()
    this.generation += 1
    this.clearTransportTimers()
    this.transport = null
    this.reader = null
    this.writer = null
    this.helloQueued = false
    this.authenticated = false
    this.resetReceiveBuffer()
    try {
      transport.close()
    } catch {
      // The close promise is fenced by generation.
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
    this.backoffMs = Math.min(WEB_MAX_BACKOFF_MS,
      Math.max(WEB_INITIAL_BACKOFF_MS, this.backoffMs * 2))
    this.reconnectTimer = setTimeout(() => {
      this.reconnectTimer = null
      this.connectNow()
    }, delay)
  }

  private scheduleHelloDeadline(attempt: number, transport: WebTransportLike): void {
    this.clearHelloTimer()
    this.helloTimer = setTimeout(() => {
      this.helloTimer = null
      if (this.isCurrent(attempt, transport) && !this.authenticated) {
        this.failTransport(transport, true)
      }
    }, WEB_HELLO_DEADLINE_MS)
  }

  private installHeartbeatCheck(
    attempt: number,
    transport: WebTransportLike,
    writer: WritableStreamDefaultWriter<Uint8Array>
  ): void {
    this.heartbeatTimer = setInterval(() => {
      if (!this.isCurrent(attempt, transport)) return
      void writer.ready.catch(() => this.failTransport(transport, true))
    }, WEB_HEARTBEAT_INTERVAL_MS)
  }

  private installStableReset(attempt: number, transport: WebTransportLike): void {
    this.clearStableTimer()
    this.stableTimer = setTimeout(() => {
      if (this.isCurrent(attempt, transport)) this.backoffMs = WEB_INITIAL_BACKOFF_MS
      this.stableTimer = null
    }, WEB_STABLE_CONNECTION_MS)
  }

  private resetReceiveBuffer(): void {
    this.receiveBuffer = new Uint8Array(0)
    this.expectedFrameBytes = null
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

  private clearTransportTimers(): void {
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
      // UI callbacks must not break QUIC lifecycle or reconnect handling.
    }
  }
}
