// Privacy Pass private tokens: RFC 9578 VOPRF(P-384, SHA-384) issuer side.
// Ported from crates/protocol/src/privacy_pass.rs.
import { createHash, timingSafeEqual } from 'node:crypto'
import { hash_to_field } from '@noble/curves/abstract/hash-to-curve'
import { hashToCurve, p384 } from '@noble/curves/p384'
import { sha384 } from '@noble/hashes/sha2'

export const VERSION = 1
export const TOKEN_TYPE = 0x0001
export const TOKEN_KEY_ID_BYTES = 32
export const NONCE_BYTES = 32
export const CHALLENGE_BYTES = 40
export const SCALAR_BYTES = 48
export const POINT_BYTES = 49
export const AUTHENTICATOR_BYTES = 48
export const TOKEN_BYTES = 2 + NONCE_BYTES + 32 + TOKEN_KEY_ID_BYTES + AUTHENTICATOR_BYTES

const CONTEXT = Buffer.from('OPRFV1-\x01-P384-SHA384', 'latin1')
const HASH_TO_GROUP_DST = Buffer.from('HashToGroup-OPRFV1-\x01-P384-SHA384', 'latin1')
const HASH_TO_SCALAR_DST = Buffer.from('HashToScalar-OPRFV1-\x01-P384-SHA384', 'latin1')
const DERIVE_KEY_DST = Buffer.from('DeriveKeyPairOPRFV1-\x01-P384-SHA384', 'latin1')
const SEED_DST_PREFIX = Buffer.from('Seed-')
const COMPOSITE_SUFFIX = Buffer.from('Composite')
const CHALLENGE_SUFFIX = Buffer.from('Challenge')
const FINALIZE_SUFFIX = Buffer.from('Finalize')

const Point = p384.ProjectivePoint
const N = p384.CURVE.n

export class PrivacyPassError extends Error {}

function mod(value) {
  const result = value % N
  return result >= 0n ? result : result + N
}

function scalarToBytes(scalar) {
  return Buffer.from(scalar.toString(16).padStart(SCALAR_BYTES * 2, '0'), 'hex')
}

function scalarFromSlice(bytes) {
  if (bytes.length !== SCALAR_BYTES) {
    throw new PrivacyPassError('invalid Privacy Pass scalar')
  }
  const scalar = BigInt(`0x${Buffer.from(bytes).toString('hex')}`)
  if (scalar >= N || scalar === 0n) {
    throw new PrivacyPassError('invalid Privacy Pass scalar')
  }
  return scalar
}

function hashToScalar(input, dst) {
  const [[scalar]] = hash_to_field(Uint8Array.from(input), 1, {
    DST: Uint8Array.from(dst),
    p: N,
    m: 1,
    k: 192,
    expand: 'xmd',
    hash: sha384,
  })
  return scalar
}

function hashToGroup(input) {
  const affine = hashToCurve(Uint8Array.from(input), { DST: Uint8Array.from(HASH_TO_GROUP_DST) })
  return Point.fromAffine(affine.toAffine())
}

function serializePoint(point) {
  if (point.equals(Point.ZERO)) {
    throw new PrivacyPassError('invalid Privacy Pass point')
  }
  return Buffer.from(point.toRawBytes(true))
}

function decodePoint(bytes) {
  let point
  try {
    point = Point.fromHex(Uint8Array.from(bytes))
  } catch {
    throw new PrivacyPassError('invalid Privacy Pass point')
  }
  if (bytes.length !== POINT_BYTES || point.equals(Point.ZERO)) {
    throw new PrivacyPassError('invalid Privacy Pass point')
  }
  return point
}

function lengthPrefix(length) {
  const bytes = Buffer.alloc(2)
  bytes.writeUInt16BE(length)
  return bytes
}

function appendBytes(parts, bytes) {
  parts.push(lengthPrefix(bytes.length), Buffer.from(bytes))
}

function deriveKey(seed) {
  const info = Buffer.from('PrivacyPass')
  const base = Buffer.concat([Buffer.from(seed), lengthPrefix(info.length), info])
  for (let counter = 0; counter <= 255; counter += 1) {
    const scalar = hashToScalar(Buffer.concat([base, Buffer.from([counter])]), DERIVE_KEY_DST)
    if (scalar !== 0n) {
      return scalar
    }
  }
  throw new PrivacyPassError('invalid Privacy Pass scalar')
}

function tokenInput(nonce, challengeDigest, tokenKeyId) {
  const tokenType = Buffer.alloc(2)
  tokenType.writeUInt16BE(TOKEN_TYPE)
  return Buffer.concat([tokenType, nonce, challengeDigest, tokenKeyId])
}

function computeComposites(publicKey, blinded, evaluated) {
  const seedParts = []
  appendBytes(seedParts, publicKey)
  appendBytes(seedParts, Buffer.concat([SEED_DST_PREFIX, CONTEXT]))
  const seed = createHash('sha384').update(Buffer.concat(seedParts)).digest()
  const transcript = []
  appendBytes(transcript, seed)
  transcript.push(lengthPrefix(0))
  appendBytes(transcript, serializePoint(blinded))
  appendBytes(transcript, serializePoint(evaluated))
  transcript.push(COMPOSITE_SUFFIX)
  const coefficient = hashToScalar(Buffer.concat(transcript), HASH_TO_SCALAR_DST)
  return [blinded.multiply(coefficient), evaluated.multiply(coefficient)]
}

function proofChallenge(publicKey, composite, evaluatedComposite, t2, t3) {
  const transcript = []
  for (const part of [publicKey, composite, evaluatedComposite, t2, t3]) {
    appendBytes(transcript, part)
  }
  transcript.push(CHALLENGE_SUFFIX)
  return hashToScalar(Buffer.concat(transcript), HASH_TO_SCALAR_DST)
}

function finalizeHash(input, issued) {
  const transcript = []
  appendBytes(transcript, input)
  appendBytes(transcript, issued)
  transcript.push(FINALIZE_SUFFIX)
  return createHash('sha384').update(Buffer.concat(transcript)).digest()
}

export function issuerParameters(seed) {
  const secret = deriveKey(seed)
  const publicKey = serializePoint(Point.BASE.multiply(secret))
  const tokenKeyId = createHash('sha256').update(publicKey).digest()
  return { publicKey, tokenKeyId }
}

export function evaluate(seed, request, proofRandomness) {
  if (request.tokenType !== TOKEN_TYPE) {
    throw new PrivacyPassError('invalid Privacy Pass token')
  }
  const parameters = issuerParameters(seed)
  if (request.truncatedTokenKeyId !== parameters.tokenKeyId[TOKEN_KEY_ID_BYTES - 1]) {
    throw new PrivacyPassError('Privacy Pass key mismatch')
  }
  const secret = deriveKey(seed)
  const blinded = decodePoint(request.blindedMessage)
  const evaluated = blinded.multiply(secret)
  const evaluatedMessage = serializePoint(evaluated)
  const [composite, evaluatedComposite] = computeComposites(
    parameters.publicKey,
    blinded,
    decodePoint(evaluatedMessage)
  )
  const nonce = scalarFromSlice(proofRandomness)
  const t2 = Point.BASE.multiply(nonce)
  const t3 = composite.multiply(nonce)
  const c = proofChallenge(
    parameters.publicKey,
    serializePoint(composite),
    serializePoint(evaluatedComposite),
    serializePoint(t2),
    serializePoint(t3)
  )
  const response = mod(nonce - c * secret)
  return {
    evaluatedMessage,
    proof: Buffer.concat([scalarToBytes(c), scalarToBytes(response)]),
  }
}

export function tokenFromBytes(bytes) {
  if (bytes.readUInt16BE(0) !== TOKEN_TYPE) {
    throw new PrivacyPassError('invalid Privacy Pass token')
  }
  const challengeStart = 2 + NONCE_BYTES
  const challengeEnd = challengeStart + 32
  const keyEnd = challengeEnd + TOKEN_KEY_ID_BYTES
  return {
    nonce: bytes.subarray(2, challengeStart),
    challengeDigest: bytes.subarray(challengeStart, challengeEnd),
    tokenKeyId: bytes.subarray(challengeEnd, keyEnd),
    authenticator: bytes.subarray(keyEnd),
  }
}

export function challengeExpiryMs(challenge) {
  return challenge.readBigUInt64BE(CHALLENGE_BYTES - 8)
}

export function verifyToken(token, challenge, nowMs, seed) {
  if (challengeExpiryMs(challenge) <= BigInt(nowMs)) {
    throw new PrivacyPassError('expired Privacy Pass challenge')
  }
  const parameters = issuerParameters(seed)
  if (!token.tokenKeyId.equals(parameters.tokenKeyId)) {
    throw new PrivacyPassError('Privacy Pass key mismatch')
  }
  const expectedChallengeDigest = createHash('sha256').update(challenge).digest()
  if (!token.challengeDigest.equals(expectedChallengeDigest)) {
    throw new PrivacyPassError('invalid Privacy Pass token')
  }
  const secret = deriveKey(seed)
  const input = tokenInput(token.nonce, token.challengeDigest, token.tokenKeyId)
  const issued = hashToGroup(input).multiply(secret)
  const expected = finalizeHash(input, serializePoint(issued))
  if (!timingSafeEqual(expected, token.authenticator)) {
    throw new PrivacyPassError('invalid Privacy Pass token')
  }
}
