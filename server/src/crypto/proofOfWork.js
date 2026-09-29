// Hashcash-style proof-of-work for unverified one-to-one connection starts.
import { createHash } from 'node:crypto'

export const VERSION = 1
export const CHALLENGE_BYTES = 32
export const MIN_DIFFICULTY_BITS = 12
export const MAX_DIFFICULTY_BITS = 24
export const DEFAULT_DIFFICULTY_BITS = 18

const DOMAIN = Buffer.from('links/chat-request-pow/v1\0')

export function digest(challenge, nonce) {
  const nonceBytes = Buffer.alloc(8)
  nonceBytes.writeBigUInt64BE(BigInt(nonce))
  return createHash('sha256').update(DOMAIN).update(challenge).update(nonceBytes).digest()
}

export function leadingZeroBits(bytes) {
  let total = 0
  for (const byte of bytes) {
    const zeroes = byte === 0 ? 8 : Math.clz32(byte) - 24
    total += zeroes
    if (zeroes < 8) {
      break
    }
  }
  return total
}

export function verify(challenge, difficultyBits, nonce) {
  if (difficultyBits < MIN_DIFFICULTY_BITS || difficultyBits > MAX_DIFFICULTY_BITS) {
    return false
  }
  return leadingZeroBits(digest(challenge, nonce)) >= difficultyBits
}
