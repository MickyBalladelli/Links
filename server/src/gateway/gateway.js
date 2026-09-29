// Gateway core: authentication, session fencing, encrypted-mailbox
// durability, and local routing. Ported from crates/gateway/src/lib.rs.
// Sealed payload bytes are never inspected.
import { randomUUID } from 'node:crypto'

import { GatewayError, ProtocolError, gatewayErrorFrom } from '../errors.js'
import { create, decode, encode, encodedLen } from '../proto.js'
import * as protocol from '../protocol.js'
import { MAX_SESSION_TTL_MS } from '../store/ephemeral.js'
import { readRequest } from '../store/relational.js'

export const HELLO_DEADLINE_MS = 5_000
export const HEARTBEAT_INTERVAL_MS = 30_000
export const HEARTBEAT_TIMEOUT_MS = 90_000
const MAX_ACCESS_TOKEN_BYTES = 512

const PROTOCOL_ERROR_CODE = {
  INVALID_ARGUMENT: 1,
  UNAUTHENTICATED: 2,
  UNSUPPORTED_VERSION: 3,
  TEMPORARILY_UNAVAILABLE: 6,
  SESSION_CONFLICT: 7,
}

const gatewayError = kind => new GatewayError(kind)

function validLocator(value) {
  return typeof value === 'string' && value.length > 0 && value.length <= 128 && /^[A-Za-z0-9:_.-]+$/.test(value)
}

export function gatewayConfig(gatewayId, region) {
  if (!validLocator(gatewayId) || !validLocator(region)) {
    throw gatewayError('Invalid')
  }
  return { gatewayId, region }
}

/** Bearer-token account service adapter. The token never enters a lease. */
export class AccountAuthDeviceAuthenticator {
  constructor(auth) {
    this.auth = auth
  }

  async authenticate(deviceId, accessToken) {
    let token
    try {
      token = new TextDecoder('utf-8', { fatal: true }).decode(accessToken)
    } catch {
      throw gatewayError('Authentication')
    }
    let account
    try {
      account = await this.auth.authenticate(token)
    } catch (error) {
      switch (error?.kind) {
        case 'Invalid':
        case 'Denied':
        case 'NotFound':
          throw gatewayError('Authentication')
        case 'Conflict':
        case 'UsernameConflict':
        case 'DeviceConflict':
          throw gatewayError('Conflict')
        default:
          throw gatewayError('Unavailable')
      }
    }
    if (account.device_id !== deviceId) {
      throw gatewayError('Authentication')
    }
    return { userId: account.user_id, deviceId: account.device_id }
  }
}

/** Local composition: no cross-region bus. */
export const localRegionBus = {
  async forward() {
    throw gatewayError('Unavailable')
  },
  async forwardSignal() {
    throw gatewayError('Unavailable')
  },
}

/** Local composition: offline recipients replay on reconnect; no push. */
export const localPushNotifier = {
  async notify() {},
}

function pushWakeup(recipientDeviceId, cursor) {
  protocol.validateId(recipientDeviceId)
  if (cursor === 0n || cursor > protocol.MAX_CURSOR) {
    throw gatewayError('Invalid')
  }
  return { recipient_device_id: recipientDeviceId, cursor }
}

export function decodeClientFrame(bytes) {
  if (bytes.length === 0 || bytes.length > protocol.MAX_FRAME_BYTES) {
    throw gatewayError('Invalid')
  }
  let frame
  try {
    frame = decode('ClientFrame', bytes)
  } catch {
    throw gatewayError('Invalid')
  }
  validateRequest(frame)
  return frame
}

export function encodeServerFrame(frame) {
  wrapProtocol(() => {
    protocol.validateId(frame.request_id)
    if (frame.body === 'compressed_batch') {
      protocol.decompressSyncBatch(frame.compressed_batch)
    }
    if (frame.body === 'web_rtc_signal') {
      protocol.validateWebRtcSignalDelivery(frame.web_rtc_signal)
    }
  })
  if (frame.body === 'web_rtc_signal' && frame.web_rtc_signal.request_id !== frame.request_id) {
    throw gatewayError('Invalid')
  }
  const bytes = encode('ServerFrame', frame)
  if (bytes.length > protocol.MAX_FRAME_BYTES) {
    throw gatewayError('Invalid')
  }
  return bytes
}

function wrapProtocol(work) {
  try {
    return work()
  } catch (error) {
    throw gatewayErrorFrom(error)
  }
}

function validateMlsBootstrapShape(bootstrap) {
  wrapProtocol(() => {
    protocol.validateId(bootstrap.conversation_id)
    protocol.validateId(bootstrap.recipient_device_id)
  })
  if (
    bootstrap.commit.length === 0 ||
    bootstrap.welcome.length === 0 ||
    bootstrap.commit.length > protocol.MAX_FRAME_BYTES ||
    bootstrap.welcome.length > protocol.MAX_FRAME_BYTES ||
    bootstrap.sender_mls_credential.length === 0 ||
    bootstrap.sender_mls_credential.length > 1024 ||
    bootstrap.sender_identity_public_key.length !== 32
  ) {
    throw gatewayError('Invalid')
  }
}

function validateRequest(frame) {
  wrapProtocol(() => protocol.validateId(frame.request_id))
  switch (frame.body) {
    case 'hello': {
      const hello = frame.hello
      wrapProtocol(() => protocol.validateId(hello.device_id))
      if (
        hello.protocol_version !== protocol.VERSION ||
        hello.device_access_token.length === 0 ||
        hello.device_access_token.length > MAX_ACCESS_TOKEN_BYTES ||
        hello.last_seen_cursor > protocol.MAX_CURSOR ||
        hello.supported_sync_compression.some(
          compression => compression !== 0 && compression !== protocol.SYNC_COMPRESSION_ZSTD_DICTIONARY_V1
        )
      ) {
        throw gatewayError(hello.protocol_version !== protocol.VERSION ? 'UnsupportedVersion' : 'Invalid')
      }
      return
    }
    case 'send':
      wrapProtocol(() => protocol.validateEnvelope(frame.send))
      return
    case 'replay':
      if (frame.replay.after_cursor > protocol.MAX_CURSOR || frame.replay.limit === 0 || frame.replay.limit > protocol.MAX_BATCH_ITEMS) {
        throw gatewayError('Invalid')
      }
      return
    case 'ack':
      if (frame.ack.through_cursor > protocol.MAX_CURSOR) {
        throw gatewayError('Invalid')
      }
      return
    case 'web_rtc_signal':
      wrapProtocol(() => protocol.validateWebRtcSignal(frame.web_rtc_signal))
      return
    case 'mls_bootstrap':
      validateMlsBootstrapShape(frame.mls_bootstrap)
      return
    default:
      throw gatewayError('Invalid')
  }
}

function serverFrame(requestId, body) {
  return create('ServerFrame', { request_id: requestId, ...body })
}

function acceptedRequestWithId(requestId, acceptedId) {
  return serverFrame(requestId, { accepted: create('Accepted', { envelope_id: acceptedId }) })
}

function temporaryUnavailable(requestId) {
  return {
    type: 'server',
    frame: serverFrame(requestId, {
      error: create('ProtocolError', { code: PROTOCOL_ERROR_CODE.TEMPORARILY_UNAVAILABLE, retry_after_ms: 1_000 }),
    }),
  }
}

function syncBatchAction(requestId, batch, useCompression) {
  wrapProtocol(() => protocol.validateSyncBatch(batch))
  const plain = serverFrame(requestId, { batch })
  if (!useCompression) {
    return { type: 'server', frame: plain }
  }
  let compressed
  try {
    compressed = protocol.compressSyncBatch(batch)
  } catch (error) {
    if (error instanceof ProtocolError && error.kind === 'TooLarge') {
      return { type: 'server', frame: plain }
    }
    throw gatewayErrorFrom(error)
  }
  const compressedFrame = serverFrame(requestId, { compressed_batch: compressed })
  if (encodedLen('ServerFrame', compressedFrame) < encodedLen('ServerFrame', plain)) {
    return { type: 'server', frame: compressedFrame }
  }
  return { type: 'server', frame: plain }
}

export class GatewaySession {
  constructor({ userId, deviceId, sessionId, lastAckCursor, syncCompression }) {
    this.userId = userId
    this.deviceId = deviceId
    this.sessionId = sessionId
    this.lastAckCursor = lastAckCursor
    this.syncCompression = syncCompression
  }
}

export class Gateway {
  constructor(config, state, queue, authenticator, bus = localRegionBus, push = localPushNotifier) {
    this.config = config
    this.state = state
    this.queue = queue
    this.authenticator = authenticator
    this.bus = bus
    this.push = push
  }

  async call(work) {
    try {
      return await work()
    } catch (error) {
      throw gatewayErrorFrom(error)
    }
  }

  /**
   * The first frame must be Hello. A second live session for the same
   * device is rejected with a session conflict.
   */
  async open(frame, nowMs) {
    validateRequest(frame)
    const requestId = frame.request_id
    if (frame.body !== 'hello') {
      throw gatewayError('Authentication')
    }
    const hello = frame.hello
    if (hello.protocol_version !== protocol.VERSION) {
      throw gatewayError('UnsupportedVersion')
    }
    const syncCompression = hello.supported_sync_compression.includes(protocol.SYNC_COMPRESSION_ZSTD_DICTIONARY_V1)
    const deviceId = protocol.parseUuid(hello.device_id)
    if (!deviceId) {
      throw gatewayError('Invalid')
    }
    const authenticated = await this.authenticator.authenticate(deviceId, hello.device_access_token)
    if (authenticated.deviceId !== deviceId || protocol.isNilUuid(authenticated.deviceId)) {
      throw gatewayError('Authentication')
    }
    const sessionId = randomUUID()
    const expiresAtMs = nowMs + MAX_SESSION_TTL_MS
    await this.call(() =>
      this.state.bind(
        { device_id: hello.device_id, session_id: sessionId, gateway_id: this.config.gatewayId, expires_at_ms: expiresAtMs },
        nowMs
      )
    )
    const request = await this.call(() =>
      readRequest(hello.device_id, hello.last_seen_cursor, protocol.MAX_BATCH_ITEMS, BigInt(nowMs))
    )
    let batch
    try {
      batch = await this.queue.read(request)
    } catch (error) {
      try {
        this.state.unbind(hello.device_id, sessionId)
      } catch {
        // The lease was just created with valid IDs; unbind cannot fail.
      }
      throw gatewayErrorFrom(error)
    }
    const actions = [
      {
        type: 'server',
        frame: serverFrame(requestId, {
          welcome: create('Welcome', {
            protocol_version: protocol.VERSION,
            heartbeat_seconds: HEARTBEAT_INTERVAL_MS / 1000,
          }),
        }),
      },
    ]
    const bootstraps = await this.call(() => this.queue.pendingMlsBootstraps(hello.device_id))
    for (const bootstrap of bootstraps) {
      actions.push({ type: 'server', frame: serverFrame(requestId, { mls_bootstrap: bootstrap }) })
    }
    if (batch.items.length) {
      actions.push(syncBatchAction(requestId, batch, syncCompression))
    }
    const session = new GatewaySession({
      userId: authenticated.userId,
      deviceId,
      sessionId,
      lastAckCursor: hello.last_seen_cursor,
      syncCompression,
    })
    return { session, actions }
  }

  async renew(session, nowMs) {
    return this.call(() => this.state.renew(session.deviceId, session.sessionId, nowMs + MAX_SESSION_TTL_MS, nowMs))
  }

  async close(session) {
    return this.call(() => this.state.unbind(session.deviceId, session.sessionId))
  }

  /**
   * Build the ordered mailbox frame for a local socket delivery, using the
   * recipient connection's durable acknowledgement cursor as checkpoint.
   */
  async localDelivery(session, lease, delivery, nowMs) {
    if (lease.session_id !== session.sessionId || lease.device_id !== session.deviceId || lease.expires_at_ms <= nowMs) {
      throw gatewayError('Authentication')
    }
    const active = await this.call(() => this.state.route(session.deviceId, nowMs))
    if (!active || active.session_id !== session.sessionId || active.gateway_id !== lease.gateway_id) {
      throw gatewayError('Authentication')
    }
    wrapProtocol(() => protocol.validateEnvelope(delivery.envelope))
    if (delivery.envelope.recipient_device_id !== session.deviceId || delivery.envelope.expires_at_ms <= BigInt(nowMs)) {
      return null
    }
    const request = await this.call(() =>
      readRequest(session.deviceId, session.lastAckCursor, protocol.MAX_BATCH_ITEMS, BigInt(nowMs))
    )
    const batch = await this.call(() => this.queue.read(request))
    if (!batch.items.some(item => item.cursor === delivery.cursor)) {
      return null
    }
    return syncBatchAction(delivery.envelope.envelope_id, batch, session.syncCompression).frame
  }

  /** Process one decoded frame from an open session. */
  async handle(session, frame, nowMs) {
    const active = await this.call(() => this.state.route(session.deviceId, nowMs))
    if (!active || active.session_id !== session.sessionId || active.gateway_id !== this.config.gatewayId) {
      throw gatewayError('Authentication')
    }
    validateRequest(frame)
    const requestId = frame.request_id
    switch (frame.body) {
      case 'hello':
        throw gatewayError('Authentication')
      case 'send': {
        const envelope = frame.send
        const recipientDeviceId = envelope.recipient_device_id
        await this.call(() => protocol.validateEnqueue(envelope, BigInt(nowMs)))
        const append = await this.call(() => this.queue.append(envelope, BigInt(nowMs)))
        const delivery = { envelope, cursor: append.cursor }
        const accepted = acceptedRequestWithId(requestId, envelope.envelope_id)
        const lease = await this.call(() => this.state.route(recipientDeviceId, nowMs))
        if (lease) {
          if (lease.gateway_id === this.config.gatewayId) {
            return [
              { type: 'server', frame: accepted },
              { type: 'localDelivery', lease, delivery },
            ]
          }
          await this.bus.forward(lease.gateway_id, delivery)
          return [{ type: 'server', frame: accepted }]
        }
        await this.push.notify(wrapProtocol(() => pushWakeup(recipientDeviceId, append.cursor)))
        return [{ type: 'server', frame: accepted }]
      }
      case 'replay': {
        const replay = frame.replay
        if (replay.after_cursor > protocol.MAX_CURSOR || replay.limit === 0 || replay.limit > protocol.MAX_BATCH_ITEMS) {
          throw gatewayError('Invalid')
        }
        const request = await this.call(() =>
          readRequest(session.deviceId, replay.after_cursor, replay.limit, BigInt(nowMs))
        )
        const batch = await this.call(() => this.queue.read(request))
        if (protocol.syncBatchEncodedLen(batch) > protocol.MAX_FRAME_BYTES - 128) {
          throw gatewayError('Invalid')
        }
        return [syncBatchAction(requestId, batch, session.syncCompression)]
      }
      case 'ack': {
        const through = frame.ack.through_cursor
        if (through > protocol.MAX_CURSOR || through < session.lastAckCursor) {
          throw gatewayError('Invalid')
        }
        await this.call(() => this.queue.acknowledge(session.deviceId, through, BigInt(nowMs)))
        session.lastAckCursor = through
        return []
      }
      case 'web_rtc_signal': {
        const signal = frame.web_rtc_signal
        const delivery = create('WebRtcSignalDelivery', {
          request_id: requestId,
          sender_device_id: session.deviceId,
          signal,
        })
        wrapProtocol(() => protocol.validateWebRtcSignalDelivery(delivery))
        const lease = await this.call(() => this.state.route(signal.target_device_id, nowMs))
        if (!lease) {
          return [temporaryUnavailable(delivery.request_id)]
        }
        if (lease.gateway_id === this.config.gatewayId) {
          return [{ type: 'localWebRtcSignal', lease, delivery }]
        }
        await this.bus.forwardSignal(lease.gateway_id, { delivery })
        return []
      }
      case 'mls_bootstrap': {
        const bootstrap = frame.mls_bootstrap
        validateMlsBootstrapShape(bootstrap)
        await this.call(() => this.queue.putPendingMlsBootstrap(bootstrap.recipient_device_id, bootstrap))
        const lease = await this.call(() => this.state.route(bootstrap.recipient_device_id, nowMs))
        if (!lease) {
          return [{ type: 'server', frame: acceptedRequestWithId(requestId, requestId) }]
        }
        if (lease.gateway_id !== this.config.gatewayId) {
          throw gatewayError('Unavailable')
        }
        return [
          { type: 'server', frame: acceptedRequestWithId(requestId, requestId) },
          { type: 'localMlsBootstrap', lease, requestId, bootstrap },
        ]
      }
      default:
        throw gatewayError('Invalid')
    }
  }
}

export function openErrorCode(error) {
  switch (error.kind) {
    case 'Authentication':
      return PROTOCOL_ERROR_CODE.UNAUTHENTICATED
    case 'UnsupportedVersion':
      return PROTOCOL_ERROR_CODE.UNSUPPORTED_VERSION
    case 'Protocol':
      return error.cause?.kind === 'UnsupportedVersion'
        ? PROTOCOL_ERROR_CODE.UNSUPPORTED_VERSION
        : PROTOCOL_ERROR_CODE.INVALID_ARGUMENT
    case 'Conflict':
      return PROTOCOL_ERROR_CODE.SESSION_CONFLICT
    case 'Unavailable':
      return PROTOCOL_ERROR_CODE.TEMPORARILY_UNAVAILABLE
    default:
      return PROTOCOL_ERROR_CODE.INVALID_ARGUMENT
  }
}
