// Minimal Links test client: username registration/login over HTTP and
// links.v1 frames over WebSocket. Used by the parity and integration tests.
import { randomBytes, randomUUID } from 'node:crypto'
import { ed25519 } from '@noble/curves/ed25519'
import WebSocket from 'ws'

import { usernameLoginTranscript, usernameRegistrationTranscript } from '../src/identity.js'
import { create, decode, encode } from '../src/proto.js'

export function b64(bytes) {
  return Buffer.from(bytes).toString('base64url')
}

export async function http(base, method, path, { token, json, body, headers = {} } = {}) {
  const init = { method, headers: { ...headers } }
  if (token) {
    init.headers.authorization = `Bearer ${token}`
  }
  if (json !== undefined) {
    init.headers['content-type'] = 'application/json'
    init.body = typeof json === 'string' ? json : JSON.stringify(json)
  } else if (body !== undefined) {
    init.body = body
  }
  const response = await fetch(`${base}${path}`, init)
  const bytes = Buffer.from(await response.arrayBuffer())
  let parsed = null
  try {
    parsed = JSON.parse(bytes.toString('utf8'))
  } catch {
    // Non-JSON body.
  }
  return { status: response.status, headers: response.headers, bytes, json: parsed }
}

export function testHandle(prefix) {
  return `${prefix}_${randomUUID().replaceAll('-', '').slice(0, 24)}`
}

export async function registerAccount(base, handle) {
  const secret = randomBytes(32)
  const publicKey = Buffer.from(ed25519.getPublicKey(secret))
  const deviceId = randomUUID()
  const mlsNodeId = randomUUID()
  const challenge = await http(base, 'POST', '/v1/auth/username/challenge', {
    json: { handle, purpose: 'registration', device_id: deviceId, mls_node_id: mlsNodeId, public_key: b64(publicKey) },
  })
  if (challenge.status !== 200) {
    throw new Error(`challenge failed: ${challenge.status} ${challenge.bytes}`)
  }
  const transcript = usernameRegistrationTranscript(
    challenge.json.challenge_id,
    handle,
    deviceId,
    mlsNodeId,
    publicKey,
    Buffer.from(challenge.json.challenge, 'base64url'),
    BigInt(challenge.json.expires_at_ms)
  )
  const signature = Buffer.from(ed25519.sign(transcript, secret))
  const registered = await http(base, 'POST', '/v1/auth/username/register', {
    json: { challenge_id: challenge.json.challenge_id, signature: b64(signature) },
  })
  if (registered.status !== 200) {
    throw new Error(`register failed: ${registered.status} ${registered.bytes}`)
  }
  return {
    handle,
    secret,
    publicKey,
    deviceId,
    mlsNodeId,
    userId: registered.json.session.user_id,
    token: registered.json.session.access_token,
    registration: registered.json,
  }
}

export async function loginAccount(base, account) {
  const challenge = await http(base, 'POST', '/v1/auth/username/challenge', {
    json: {
      handle: account.handle,
      purpose: 'login',
      device_id: account.deviceId,
      mls_node_id: account.mlsNodeId,
      public_key: b64(account.publicKey),
    },
  })
  const transcript = usernameLoginTranscript(
    challenge.json.challenge_id,
    account.handle,
    account.deviceId,
    account.mlsNodeId,
    account.publicKey,
    Buffer.from(challenge.json.challenge, 'base64url'),
    BigInt(challenge.json.expires_at_ms)
  )
  const login = await http(base, 'POST', '/v1/auth/username/login', {
    json: { challenge_id: challenge.json.challenge_id, signature: b64(ed25519.sign(transcript, account.secret)) },
  })
  if (login.status !== 200) {
    throw new Error(`login failed: ${login.status} ${login.bytes}`)
  }
  account.token = login.json.session.access_token
  return login.json
}

export class FrameSocket {
  static async open(url, protocols = ['links.v1']) {
    const ws = new WebSocket(url, protocols)
    const socket = new FrameSocket(ws)
    await new Promise((resolve, reject) => {
      ws.once('open', resolve)
      ws.once('error', reject)
      ws.once('unexpected-response', (_req, res) => reject(new Error(`unexpected response ${res.statusCode}`)))
    })
    return socket
  }

  constructor(ws) {
    this.ws = ws
    this.frames = []
    this.waiters = []
    this.closed = false
    ws.on('message', (data, isBinary) => {
      this.frames.push(isBinary ? decode('ServerFrame', Buffer.from(data)) : { text: data.toString() })
      this.flush()
    })
    ws.on('close', (code) => {
      this.closed = true
      this.closeCode = code
      this.flush()
    })
    ws.on('error', () => {})
  }

  flush() {
    while (this.waiters.length && (this.frames.length || this.closed)) {
      const waiter = this.waiters.shift()
      waiter(this.frames.length ? this.frames.shift() : null)
    }
  }

  next(timeoutMs = 5_000) {
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => reject(new Error('timed out waiting for frame')), timeoutMs)
      this.waiters.push(frame => {
        clearTimeout(timer)
        resolve(frame)
      })
      this.flush()
    })
  }

  send(frame) {
    this.ws.send(encode('ClientFrame', create('ClientFrame', frame)))
  }

  hello(account, { lastSeenCursor = 0n, compression = [] } = {}) {
    this.send({
      request_id: randomUUID(),
      hello: create('Hello', {
        protocol_version: 1,
        device_id: account.deviceId,
        device_access_token: Buffer.from(account.token),
        last_seen_cursor: lastSeenCursor,
        supported_sync_compression: compression,
      }),
    })
  }

  close() {
    this.ws.close()
  }
}
