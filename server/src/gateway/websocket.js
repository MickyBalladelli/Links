// links.v1 WebSocket adapter. Ported from crates/gateway/src/websocket.rs.
//
// Each connection accepts one binary protobuf frame at a time and routes
// local deliveries to the connection named by the gateway lease. Inbound
// frames, outbound deliveries, and heartbeats for one socket run strictly in
// sequence, like the Rust per-socket select loop.
import { createServer } from 'node:http'
import { WebSocketServer } from 'ws'

import { GatewayError } from '../errors.js'
import { create } from '../proto.js'
import * as protocol from '../protocol.js'
import {
  HEARTBEAT_INTERVAL_MS,
  HELLO_DEADLINE_MS,
  decodeClientFrame,
  encodeServerFrame,
  openErrorCode,
} from './gateway.js'

export const WEBSOCKET_PATH = '/v1/connect'
export const WEBSOCKET_SUBPROTOCOL = 'links.v1'

const STATUS_TEXT = { 400: 'Bad Request', 404: 'Not Found', 405: 'Method Not Allowed', 426: 'Upgrade Required' }

function pathname(url) {
  const index = url.indexOf('?')
  return index === -1 ? url : url.slice(0, index)
}

function requestedProtocols(req) {
  const header = req.headers['sec-websocket-protocol']
  if (!header) {
    return []
  }
  return header.split(',').map(value => value.trim())
}

/** WebSocketUpgrade extractor checks, in axum's order. */
function upgradeRejection(req) {
  if (req.method !== 'GET') {
    return [405, 'Request method must be `GET`']
  }
  const connection = (req.headers.connection ?? '').toLowerCase().split(',').map(value => value.trim())
  if (!connection.includes('upgrade')) {
    return [400, "Connection header did not include 'upgrade'"]
  }
  if ((req.headers.upgrade ?? '').toLowerCase() !== 'websocket') {
    return [400, "`Upgrade` header did not include 'websocket'"]
  }
  if (req.headers['sec-websocket-version'] !== '13') {
    return [400, '`Sec-WebSocket-Version` header did not include \'13\'']
  }
  if (!req.headers['sec-websocket-key']) {
    return [400, '`Sec-WebSocket-Key` header missing']
  }
  if (!requestedProtocols(req).includes(WEBSOCKET_SUBPROTOCOL)) {
    return [400, '']
  }
  return null
}

function writeRawResponse(socket, status, text) {
  const body = Buffer.from(text)
  const headers = [`HTTP/1.1 ${status} ${STATUS_TEXT[status] ?? ''}`, `content-length: ${body.length}`]
  if (body.length) {
    headers.push('content-type: text/plain; charset=utf-8')
  }
  headers.push('connection: close', '', '')
  socket.end(Buffer.concat([Buffer.from(headers.join('\r\n')), body]))
}

const now = () => Date.now()

export class WebSocketAdapter {
  constructor(gateway) {
    this.gateway = gateway
    this.connections = new Map()
    this.wss = new WebSocketServer({
      noServer: true,
      maxPayload: protocol.MAX_FRAME_BYTES,
      perMessageDeflate: false,
      handleProtocols: protocols => (protocols.has(WEBSOCKET_SUBPROTOCOL) ? WEBSOCKET_SUBPROTOCOL : false),
    })
  }

  createServer() {
    const server = createServer((req, res) => {
      if (pathname(req.url) !== WEBSOCKET_PATH) {
        res.writeHead(404, { 'content-length': 0 }).end()
        return
      }
      const [status, text] = upgradeRejection(req) ?? [426, "WebSocket request couldn't be upgraded since no upgrade state was present"]
      res.writeHead(status, { 'content-type': 'text/plain; charset=utf-8' }).end(text)
    })
    server.on('upgrade', (req, socket, head) => {
      socket.on('error', () => socket.destroy())
      if (pathname(req.url) !== WEBSOCKET_PATH) {
        writeRawResponse(socket, 404, '')
        return
      }
      const rejection = upgradeRejection(req)
      if (rejection) {
        writeRawResponse(socket, rejection[0], rejection[1])
        return
      }
      this.wss.handleUpgrade(req, socket, head, ws => this.accept(ws))
    })
    return server
  }

  accept(ws) {
    new Connection(this, ws).start()
  }

  sendTo(sessionId, outbound) {
    const connection = this.connections.get(sessionId)
    if (!connection || !connection.enqueue(() => connection.deliverOutbound(outbound))) {
      throw new GatewayError('Unavailable')
    }
  }

  unregister(sessionId) {
    this.connections.delete(sessionId)
  }

  async dispatchActions(currentSessionId, actions, nowMs) {
    for (const action of actions) {
      switch (action.type) {
        case 'server':
          this.sendTo(currentSessionId, { type: 'frame', frame: action.frame })
          break
        case 'localDelivery':
          try {
            this.sendTo(action.lease.session_id, {
              type: 'delivery',
              lease: action.lease,
              delivery: action.delivery,
              nowMs,
            })
          } catch {
            // The envelope is already in the durable mailbox. A stale
            // recipient socket must not tear down the sender; the
            // recipient will receive it through replay after reconnecting.
            this.unregister(action.lease.session_id)
          }
          break
        case 'localWebRtcSignal':
          this.sendTo(action.lease.session_id, { type: 'webRtcSignal', lease: action.lease, delivery: action.delivery })
          break
        case 'localMlsBootstrap':
          try {
            this.sendTo(action.lease.session_id, {
              type: 'mlsBootstrap',
              lease: action.lease,
              requestId: action.requestId,
              bootstrap: action.bootstrap,
            })
          } catch {
            // Bootstrap data is persisted before dispatch. Keep the sender
            // connected and let the recipient recover it on its next
            // authenticated connection.
            this.unregister(action.lease.session_id)
          }
          break
        default:
          throw new GatewayError('Unavailable')
      }
    }
  }
}

class Connection {
  constructor(adapter, ws) {
    this.adapter = adapter
    this.gateway = adapter.gateway
    this.ws = ws
    this.session = null
    this.sessionId = null
    this.finished = false
    this.chain = Promise.resolve()
    this.heartbeat = null
    this.helloTimer = null
  }

  start() {
    const ws = this.ws
    this.helloTimer = setTimeout(() => this.drop(), HELLO_DEADLINE_MS)
    let firstMessage = true
    const takeFirst = () => {
      const first = firstMessage
      if (first) {
        firstMessage = false
        clearTimeout(this.helloTimer)
      }
      return first
    }
    ws.on('message', (data, isBinary) => {
      const bytes = Buffer.isBuffer(data) ? data : Buffer.concat(Array.isArray(data) ? data : [Buffer.from(data)])
      const first = takeFirst()
      this.enqueue(() => (first ? this.onHello(bytes, isBinary) : this.onFrame(bytes, isBinary)))
    })
    // Ping and pong are messages to the socket loop: before Hello they end
    // the connection, afterwards they renew the lease.
    const keepalive = () => {
      if (takeFirst()) {
        this.drop()
        return
      }
      this.enqueue(() => this.onKeepalive())
    }
    ws.on('ping', keepalive)
    ws.on('pong', keepalive)
    ws.on('close', () => this.enqueue(() => this.stop()))
    ws.on('error', () => this.enqueue(() => this.stop()))
  }

  /** Queue work behind every earlier event for this socket. */
  enqueue(task) {
    if (this.finished) {
      return false
    }
    this.chain = this.chain.then(async () => {
      if (this.finished) {
        return
      }
      try {
        await task()
      } catch {
        await this.stop()
      }
    })
    return true
  }

  async onHello(bytes, isBinary) {
    if (!isBinary) {
      this.drop()
      return
    }
    let frame
    try {
      frame = decodeClientFrame(bytes)
    } catch {
      this.drop()
      return
    }
    const requestId = frame.request_id
    const openedAtMs = now()
    let opened
    try {
      opened = await this.gateway.open(frame, openedAtMs)
    } catch (error) {
      const errorFrame = create('ServerFrame', {
        request_id: requestId,
        error: create('ProtocolError', { code: openErrorCode(error), retry_after_ms: 0 }),
      })
      try {
        await this.send(encodeServerFrame(errorFrame))
      } catch {
        // The socket is closing either way.
      }
      this.finished = true
      this.ws.close()
      setTimeout(() => this.ws.terminate(), 1_000).unref()
      return
    }
    this.session = opened.session
    this.sessionId = opened.session.sessionId
    this.adapter.connections.set(this.sessionId, this)
    try {
      await this.adapter.dispatchActions(this.sessionId, opened.actions, openedAtMs)
    } catch {
      await this.stop()
      return
    }
    this.heartbeat = setInterval(() => this.enqueue(() => this.onHeartbeat()), HEARTBEAT_INTERVAL_MS)
  }

  async onHeartbeat() {
    if (!(await this.renewOrFalse())) {
      await this.stop()
      return
    }
    await new Promise((resolve, reject) => this.ws.ping(Buffer.alloc(0), undefined, error => (error ? reject(error) : resolve())))
  }

  async onKeepalive() {
    if (!this.session) {
      return
    }
    if (!(await this.renewOrFalse())) {
      await this.stop()
    }
  }

  async renewOrFalse() {
    try {
      return await this.gateway.renew(this.session, now())
    } catch {
      return false
    }
  }

  async onFrame(bytes, isBinary) {
    if (!isBinary) {
      await this.stop()
      return
    }
    const frame = decodeClientFrame(bytes)
    const nowMs = now()
    const actions = await this.gateway.handle(this.session, frame, nowMs)
    await this.adapter.dispatchActions(this.sessionId, actions, nowMs)
  }

  async deliverOutbound(outbound) {
    const frame = await this.resolveOutbound(outbound)
    if (frame) {
      await this.send(encodeServerFrame(frame))
    }
  }

  async resolveOutbound(outbound) {
    const session = this.session
    switch (outbound.type) {
      case 'frame':
        return outbound.frame
      case 'delivery':
        return this.gateway.localDelivery(session, outbound.lease, outbound.delivery, outbound.nowMs)
      case 'webRtcSignal': {
        const { lease, delivery } = outbound
        if (lease.session_id !== session.sessionId || lease.device_id !== session.deviceId) {
          throw new GatewayError('Authentication')
        }
        protocol.validateWebRtcSignalDelivery(delivery)
        if (delivery.signal.target_device_id !== session.deviceId) {
          throw new GatewayError('Authentication')
        }
        return create('ServerFrame', { request_id: delivery.request_id, web_rtc_signal: delivery })
      }
      case 'mlsBootstrap': {
        const { lease, requestId, bootstrap } = outbound
        if (
          lease.session_id !== session.sessionId ||
          lease.device_id !== session.deviceId ||
          bootstrap.recipient_device_id !== session.deviceId
        ) {
          throw new GatewayError('Authentication')
        }
        return create('ServerFrame', { request_id: requestId, mls_bootstrap: bootstrap })
      }
      default:
        throw new GatewayError('Unavailable')
    }
  }

  send(bytes) {
    return new Promise((resolve, reject) => {
      this.ws.send(bytes, { binary: true }, error => (error ? reject(error) : resolve()))
    })
  }

  /** Drop the socket without a session (pre-Hello failures). */
  drop() {
    if (this.finished) {
      return
    }
    this.finished = true
    clearTimeout(this.helloTimer)
    this.ws.terminate()
  }

  /** Leave the socket loop: unregister, release the lease, drop the socket. */
  async stop() {
    if (this.finished) {
      return
    }
    this.finished = true
    clearTimeout(this.helloTimer)
    clearInterval(this.heartbeat)
    if (this.session) {
      this.adapter.unregister(this.sessionId)
      try {
        await this.gateway.close(this.session)
      } catch {
        // Lease release failures leave the lease to expire on its own.
      }
    }
    this.ws.terminate()
  }
}
