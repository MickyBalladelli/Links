// Bounded, single-process session lease table (MemoryEphemeralState).
// Operations are synchronous, so each one is atomic on the event loop.
import { StoreError } from '../errors.js'
import { isValidId } from '../protocol.js'

export const MAX_SESSION_TTL_MS = 120_000

function validExpiry(expiresAtMs, nowMs) {
  return expiresAtMs > nowMs && expiresAtMs - nowMs <= MAX_SESSION_TTL_MS
}

function validKey(key) {
  return typeof key === 'string' && key.length > 0 && key.length <= 128 && /^[A-Za-z0-9:_-]+$/.test(key)
}

function requireId(value) {
  if (!isValidId(value)) {
    throw new StoreError('Protocol')
  }
}

export class MemoryEphemeralState {
  constructor(maxEntries) {
    if (!(maxEntries > 0)) {
      throw new StoreError('Invalid')
    }
    this.maxEntries = maxEntries
    this.sessions = new Map()
  }

  /** Claim the device route. An active lease cannot be replaced. */
  bind(lease, nowMs) {
    requireId(lease.device_id)
    requireId(lease.session_id)
    if (!validKey(lease.gateway_id) || !validExpiry(lease.expires_at_ms, nowMs)) {
      throw new StoreError('Invalid')
    }
    for (const [deviceId, existing] of this.sessions) {
      if (existing.expires_at_ms <= nowMs) {
        this.sessions.delete(deviceId)
      }
    }
    if (this.sessions.has(lease.device_id)) {
      throw new StoreError('Conflict')
    }
    if (this.sessions.size >= this.maxEntries) {
      throw new StoreError('Unavailable')
    }
    this.sessions.set(lease.device_id, { ...lease })
  }

  renew(deviceId, sessionId, expiresAtMs, nowMs) {
    requireId(deviceId)
    requireId(sessionId)
    if (!validExpiry(expiresAtMs, nowMs)) {
      throw new StoreError('Invalid')
    }
    const lease = this.sessions.get(deviceId)
    if (lease && lease.session_id === sessionId && lease.expires_at_ms > nowMs) {
      lease.expires_at_ms = expiresAtMs
      return true
    }
    return false
  }

  unbind(deviceId, sessionId) {
    requireId(deviceId)
    requireId(sessionId)
    if (this.sessions.get(deviceId)?.session_id === sessionId) {
      this.sessions.delete(deviceId)
      return true
    }
    return false
  }

  route(deviceId, nowMs) {
    requireId(deviceId)
    const lease = this.sessions.get(deviceId)
    if (lease && lease.expires_at_ms <= nowMs) {
      this.sessions.delete(deviceId)
      return null
    }
    return lease ? { ...lease } : null
  }
}
