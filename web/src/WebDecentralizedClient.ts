import { requireCanonicalUUID } from './LinksWebClient'

export const WEB_DECENTRALIZED_MAX_ENDPOINTS = 8
export const WEB_DECENTRALIZED_MAX_RELAYS = 16
export const WEB_DECENTRALIZED_MAX_ENVELOPE_BYTES = 256 * 1024
export const WEB_DECENTRALIZED_MAX_CHUNK_BYTES = 256 * 1024
export const WEB_DECENTRALIZED_MAX_LEASE_MS = 10 * 60 * 1000

export type WebDecentralizedRelayMode = 'open' | 'token-incentivized'

export interface WebDecentralizedTransportAdapter {
  publishOpaque(endpoint: string, envelope: Uint8Array): Promise<void>
  replayOpaque(endpoint: string, afterCursor: bigint, limit: number): Promise<Uint8Array[]>
}

export interface WebDecentralizedChunkStore {
  uploadCiphertext(gateway: string, cid: string, ciphertext: Uint8Array): Promise<void>
  downloadCiphertext(gateway: string, cid: string): Promise<Uint8Array>
}

/** This flag is set only after the WASM/shared-core DHT verifier accepts the record. */
export interface WebDecentralizedMediaRelay {
  nodeID: string
  region: string
  endpoint: string
  turnURL?: string
  mode: WebDecentralizedRelayMode
  priceUnitsPerMinute: bigint
  maxBitrateKbps: number
  expiresAtMs: bigint
  supportsSFrame: true
  verified: true
}

export interface WebDecentralizedRelayAccess {
  token: string
  relayNodeID: string
  sessionID: string
  expiresAtMs: bigint
  maxDurationMs: bigint
}

export interface WebDecentralizedMediaRoute {
  relay: WebDecentralizedMediaRelay
  sessionID: string
  durationMs: bigint
  accessToken?: string
}

export interface WebDecentralizedClientOptions {
  region: string
  transportEndpoints: readonly string[]
  storageGateways: readonly string[]
  mediaRelays: readonly WebDecentralizedMediaRelay[]
  transport: WebDecentralizedTransportAdapter
  storage: WebDecentralizedChunkStore
}

/**
 * Browser host for decentralized Links routes. Transport retries preserve the
 * exact sealed envelope bytes. Storage retries preserve the CID and verify the
 * returned ciphertext before giving it to decryption.
 */
export class WebDecentralizedClient {
  private readonly regionValue: string
  private readonly transportEndpoints: readonly string[]
  private readonly storageGateways: readonly string[]
  private readonly mediaRelays: readonly WebDecentralizedMediaRelay[]
  private readonly transport: WebDecentralizedTransportAdapter
  private readonly storage: WebDecentralizedChunkStore

  constructor(options: WebDecentralizedClientOptions) {
    validateLocator(options.region, 'region')
    if (options.transportEndpoints.length === 0 ||
        options.transportEndpoints.length > WEB_DECENTRALIZED_MAX_ENDPOINTS ||
        options.storageGateways.length === 0 ||
        options.storageGateways.length > WEB_DECENTRALIZED_MAX_ENDPOINTS ||
        options.mediaRelays.length > WEB_DECENTRALIZED_MAX_RELAYS) {
      throw new Error('Invalid decentralized route')
    }
    options.transportEndpoints.forEach(endpoint => validateEndpoint(endpoint, 'wss:'))
    options.storageGateways.forEach(gateway => validateEndpoint(gateway, 'https:'))
    options.mediaRelays.forEach(relay => validateRelay(relay, options.region))
    if (options.transport === null || options.storage === null) {
      throw new Error('Invalid decentralized adapters')
    }
    this.regionValue = options.region
    this.transportEndpoints = [...options.transportEndpoints]
    this.storageGateways = [...options.storageGateways]
    this.mediaRelays = [...options.mediaRelays]
    this.transport = options.transport
    this.storage = options.storage
  }

  get region(): string {
    return this.regionValue
  }

  async publishOpaqueEnvelope(envelope: Uint8Array): Promise<void> {
    validateBytes(envelope, WEB_DECENTRALIZED_MAX_ENVELOPE_BYTES, 'envelope')
    for (const endpoint of this.transportEndpoints) {
      try {
        await this.transport.publishOpaque(endpoint, envelope.slice())
        return
      } catch {
        continue
      }
    }
    throw new Error('Decentralized transport unavailable')
  }

  async replayOpaque(afterCursor: bigint, limit = 100): Promise<Uint8Array[]> {
    if (afterCursor < 0n || limit < 1 || limit > 100) {
      throw new Error('Invalid decentralized replay')
    }
    for (const endpoint of this.transportEndpoints) {
      try {
        const envelopes = await this.transport.replayOpaque(endpoint, afterCursor, limit)
        if (envelopes.length > limit || envelopes.some(envelope =>
          envelope.length === 0 || envelope.length > WEB_DECENTRALIZED_MAX_ENVELOPE_BYTES)) {
          throw new Error('Invalid decentralized replay')
        }
        return envelopes.map(envelope => envelope.slice())
      } catch (error) {
        if (error instanceof Error && error.message === 'Invalid decentralized replay') throw error
        continue
      }
    }
    throw new Error('Decentralized transport unavailable')
  }

  async uploadCiphertextChunk(cid: string, ciphertext: Uint8Array): Promise<void> {
    if (await cidForBytesAsync(ciphertext) !== cid) throw new Error('Ciphertext CID mismatch')
    validateBytes(ciphertext, WEB_DECENTRALIZED_MAX_CHUNK_BYTES, 'ciphertext chunk')
    for (const gateway of this.storageGateways) {
      try {
        await this.storage.uploadCiphertext(gateway, cid, ciphertext.slice())
        return
      } catch {
        continue
      }
    }
    throw new Error('Decentralized storage unavailable')
  }

  async downloadCiphertextChunk(cid: string): Promise<Uint8Array> {
    validateCID(cid)
    for (const gateway of this.storageGateways) {
      try {
        const ciphertext = await this.storage.downloadCiphertext(gateway, cid)
        if (await cidForBytesAsync(ciphertext) !== cid) {
          throw new Error('Ciphertext CID mismatch')
        }
        return ciphertext.slice()
      } catch (error) {
        if (error instanceof Error && error.message === 'Ciphertext CID mismatch') throw error
        continue
      }
    }
    throw new Error('Decentralized storage unavailable')
  }

  selectMediaRelay(
    sessionID: string,
    durationMs: bigint,
    access?: WebDecentralizedRelayAccess,
    nowMs = BigInt(Date.now())
  ): WebDecentralizedMediaRoute {
    requireCanonicalUUID(sessionID, 'media session ID')
    if (durationMs <= 0n || durationMs > BigInt(WEB_DECENTRALIZED_MAX_LEASE_MS)) {
      throw new Error('Invalid media relay duration')
    }
    const open = this.mediaRelays.find(relay =>
      relay.mode === 'open' && relay.expiresAtMs > nowMs)
    if (open !== undefined) return { relay: open, sessionID, durationMs }

    for (const relay of this.mediaRelays) {
      if (relay.mode !== 'token-incentivized' || relay.expiresAtMs <= nowMs ||
          access === undefined || access.relayNodeID !== relay.nodeID ||
          access.sessionID !== sessionID || access.expiresAtMs <= nowMs ||
          access.maxDurationMs < durationMs) continue
      if (access.token.length === 0) throw new Error('Invalid media relay token')
      return { relay, sessionID, durationMs, accessToken: access.token }
    }
    throw new Error('No decentralized media relay available')
  }
}

function validateRelay(relay: WebDecentralizedMediaRelay, region: string): void {
  validateLocator(relay.nodeID, 'relay node ID')
  validateLocator(relay.region, 'relay region')
  if (relay.region !== region || relay.verified !== true || relay.supportsSFrame !== true ||
      relay.maxBitrateKbps <= 0 || relay.expiresAtMs <= 0n ||
      (relay.mode === 'open' && relay.priceUnitsPerMinute !== 0n) ||
      (relay.mode === 'token-incentivized' && relay.priceUnitsPerMinute <= 0n)) {
    throw new Error('Invalid media relay')
  }
  validateEndpoint(relay.endpoint, 'wss:')
  if (relay.turnURL !== undefined &&
      (!relay.turnURL.startsWith('turn:') && !relay.turnURL.startsWith('turns:'))) {
    throw new Error('Invalid TURN endpoint')
  }
  if (relay.turnURL !== undefined) validateEndpoint(relay.turnURL, 'turn')
}

function validateLocator(value: string, field: string): void {
  if (!/^[A-Za-z0-9._:-]{1,128}$/.test(value)) throw new Error(`Invalid ${field}`)
}

function validateEndpoint(value: string, scheme: string): void {
  if (value.length === 0 || value.length > 512 || !value.startsWith(scheme) ||
      /[\u0000-\u0020#]/.test(value)) throw new Error('Invalid decentralized endpoint')
}

function validateBytes(value: Uint8Array, max: number, field: string): void {
  if (!(value instanceof Uint8Array) || value.length === 0 || value.length > max) {
    throw new Error(`Invalid ${field}`)
  }
}

function validateCID(cid: string): void {
  if (!/^b[a-z2-7]{58}$/.test(cid)) throw new Error('Invalid content CID')
}

async function sha256(value: Uint8Array): Promise<Uint8Array> {
  if (globalThis.crypto?.subtle === undefined) throw new Error('Web Crypto unavailable')
  return new Uint8Array(await globalThis.crypto.subtle.digest('SHA-256', value))
}

function base32(value: Uint8Array): string {
  const alphabet = 'abcdefghijklmnopqrstuvwxyz234567'
  let output = ''
  let accumulator = 0
  let bits = 0
  for (const byte of value) {
    accumulator = (accumulator << 8) | byte
    bits += 8
    while (bits >= 5) {
      bits -= 5
      output += alphabet[(accumulator >>> bits) & 31]
      accumulator &= bits === 0 ? 0 : (1 << bits) - 1
    }
  }
  if (bits > 0) output += alphabet[(accumulator << (5 - bits)) & 31]
  return output
}

function cidForBytesSync(digest: Uint8Array): string {
  const binary = new Uint8Array(36)
  binary.set([1, 0x55, 0x12, 0x20])
  binary.set(digest, 4)
  return `b${base32(binary)}`
}

async function cidForBytesAsync(value: Uint8Array): Promise<string> {
  validateBytes(value, WEB_DECENTRALIZED_MAX_CHUNK_BYTES, 'ciphertext chunk')
  return cidForBytesSync(await sha256(value))
}
