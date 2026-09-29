// Verifiable OPRF over ristretto255 for one-sided private contact discovery.
// Ported from crates/protocol/src/contact_psi.rs (server-side operations).
import { createHash } from 'node:crypto'
import { RistrettoPoint, ed25519 } from '@noble/curves/ed25519'

export const VERSION = 1
export const POINT_BYTES = 32
export const TOKEN_BYTES = 32
export const MAX_QUERY_ITEMS = 256
export const MAX_DIRECTORY_TOKENS = 1_000_000
export const FILTER_HASH_COUNT = 44
const FILTER_MIN_BITS = 8_192
const FILTER_BITS_PER_TOKEN = 64
const FILTER_MAX_BYTES = 16 * 1024 * 1024

const INPUT_DOMAIN = Buffer.from('links/contact-psi/input/v1\0')
const HASH_TO_GROUP_DOMAIN = Buffer.from('links/contact-psi/hash-to-group/v1\0')
const OUTPUT_DOMAIN = Buffer.from('links/contact-psi/output/v1\0')
const PROOF_DOMAIN = Buffer.from('links/contact-psi/dleq/v1\0')
const FILTER_DOMAIN = Buffer.from('links/contact-psi/filter/v1\0')

const L = ed25519.CURVE.n
const U64_MASK = (1n << 64n) - 1n

export class ContactPsiError extends Error {}

function numberFromLE(bytes) {
  return BigInt(`0x${Buffer.from(bytes).reverse().toString('hex') || '0'}`)
}

function scalarToLE(scalar) {
  return Buffer.from(scalar.toString(16).padStart(64, '0'), 'hex').reverse()
}

function sha512(...parts) {
  const hash = createHash('sha512')
  for (const part of parts) {
    hash.update(part)
  }
  return hash.digest()
}

function sha256(...parts) {
  const hash = createHash('sha256')
  for (const part of parts) {
    hash.update(part)
  }
  return hash.digest()
}

function scalarFromBytesModOrderWide(bytes) {
  return numberFromLE(bytes) % L
}

function scalarFromRandomness(randomness) {
  const scalar = scalarFromBytesModOrderWide(randomness)
  if (scalar === 0n) {
    throw new ContactPsiError('invalid contact PSI scalar')
  }
  return scalar
}

function serverScalar(serverSecret) {
  const scalar = scalarFromBytesModOrderWide(sha512(PROOF_DOMAIN, serverSecret))
  return scalar === 0n ? 1n : scalar
}

function hashToGroup(input) {
  return RistrettoPoint.hashToCurve(Uint8Array.from(sha512(HASH_TO_GROUP_DOMAIN, input)))
}

function compress(point) {
  return Buffer.from(point.toRawBytes())
}

function decodePoint(bytes) {
  let point
  try {
    point = RistrettoPoint.fromHex(Uint8Array.from(bytes))
  } catch {
    throw new ContactPsiError('invalid contact PSI point')
  }
  if (point.equals(RistrettoPoint.ZERO)) {
    throw new ContactPsiError('invalid contact PSI point')
  }
  return point
}

function multiply(point, scalar) {
  return scalar === 0n ? RistrettoPoint.ZERO : point.multiply(scalar)
}

function proofChallenge(publicKey, blinded, evaluated, a, b) {
  return scalarFromBytesModOrderWide(sha512(PROOF_DOMAIN, publicKey, blinded, evaluated, a, b))
}

function validateE164(phone) {
  const digits = phone.startsWith('+') ? phone.slice(1) : null
  if (digits === null || digits.length < 8 || digits.length > 15 || digits.startsWith('0') || !/^[0-9]+$/.test(digits)) {
    throw new ContactPsiError('phone must be canonical E.164')
  }
}

export function serverPublicKey(serverSecret) {
  return compress(RistrettoPoint.BASE.multiply(serverScalar(serverSecret)))
}

export function evaluateBlinded(serverSecret, blindedBytes, proofRandomness) {
  const blinded = decodePoint(blindedBytes)
  const secret = serverScalar(serverSecret)
  const nonce = scalarFromRandomness(proofRandomness)
  const evaluated = multiply(blinded, secret)
  if (evaluated.equals(RistrettoPoint.ZERO)) {
    throw new ContactPsiError('invalid contact PSI point')
  }
  const publicKey = compress(RistrettoPoint.BASE.multiply(secret))
  const evaluatedBytes = compress(evaluated)
  const a = RistrettoPoint.BASE.multiply(nonce)
  const b = multiply(blinded, nonce)
  const challenge = proofChallenge(publicKey, Buffer.from(blindedBytes), evaluatedBytes, compress(a), compress(b))
  const response = (nonce + challenge * secret) % L
  return {
    evaluatedPoint: evaluatedBytes,
    proof: Buffer.concat([scalarToLE(challenge), scalarToLE(response)]),
  }
}

/** Phone-derived directory token used by OTP enrollment. */
export function directoryToken(phoneE164, serverSecret) {
  validateE164(phoneE164)
  const input = sha256(INPUT_DOMAIN, phoneE164)
  const point = multiply(hashToGroup(input), serverScalar(serverSecret))
  return sha256(OUTPUT_DOMAIN, input, compress(point))
}

function filterHashes(token) {
  const digest = sha512(FILTER_DOMAIN, token)
  const first = digest.readBigUInt64BE(0)
  const step = digest.readBigUInt64BE(8) | 1n
  return [first, step]
}

function filterByteCount(itemCount) {
  if (itemCount > MAX_DIRECTORY_TOKENS) {
    throw new ContactPsiError('contact PSI batch or directory is too large')
  }
  const bitCount = Math.max(FILTER_MIN_BITS, Math.ceil((itemCount * FILTER_BITS_PER_TOKEN) / 8) * 8)
  const byteCount = bitCount / 8
  if (byteCount > FILTER_MAX_BYTES) {
    throw new ContactPsiError('contact PSI batch or directory is too large')
  }
  return byteCount
}

/** Bloom-style membership filter over directory tokens. */
export function filterFromTokens(tokens) {
  if (tokens.length > MAX_DIRECTORY_TOKENS) {
    throw new ContactPsiError('contact PSI batch or directory is too large')
  }
  const bits = Buffer.alloc(filterByteCount(tokens.length))
  const bitCount = BigInt(bits.length) * 8n
  for (const token of tokens) {
    const [first, step] = filterHashes(token)
    for (let index = 0n; index < BigInt(FILTER_HASH_COUNT); index += 1n) {
      const bit = ((first + ((index * step) & U64_MASK)) & U64_MASK) % bitCount
      bits[Number(bit / 8n)] |= 1 << Number(bit % 8n)
    }
  }
  return { bits, hashCount: FILTER_HASH_COUNT, itemCount: tokens.length }
}
