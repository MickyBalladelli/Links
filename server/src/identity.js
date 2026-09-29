// Ed25519 identity verification, signed transcripts, and RFC 9420 basic
// credentials. Ported from crates/identity; every transcript is a byte-exact
// match because clients sign them with the Rust implementation.
import { createHash } from 'node:crypto'
import { ed25519 } from '@noble/curves/ed25519'

import { IdentityError } from './errors.js'
import {
  isNilUuid,
  parseUuid,
  uuidBytes,
  validateDeviceSubcertificate,
  validateHandle,
} from './protocol.js'

const Point = ed25519.ExtendedPoint
const CURVE_ORDER = ed25519.CURVE.n

const PQXDH_EC_ENCODING_TAG = 1
const PQXDH_KEM_ENCODING_TAG = 2
const PQXDH_IDENTITY_BINDING_DOMAIN = Buffer.from('links/pqxdh/identity-binding/v1\0')
const PQXDH_SIGNED_PREKEY_DOMAIN = Buffer.from('links/pqxdh/signed-prekey/v1\0')
const PQXDH_KEM_PREKEY_DOMAIN = Buffer.from('links/pqxdh/kem-prekey/v1\0')
const DEVICE_IDENTITY_DOMAIN = Buffer.from('links/device/v1\0')
const DEVICE_SUBCERTIFICATE_DOMAIN = Buffer.from('links/device-subcertificate/v1\0')

function u64be(value) {
  const bytes = Buffer.alloc(8)
  bytes.writeBigUInt64BE(BigInt(value))
  return bytes
}

function u32be(value) {
  const bytes = Buffer.alloc(4)
  bytes.writeUInt32BE(value)
  return bytes
}

function bytesToNumberLE(bytes) {
  return BigInt(`0x${Buffer.from(bytes).reverse().toString('hex') || '0'}`)
}

// Decompression follows curve25519-dalek: the high bit of y is the sign bit
// and y is not required to be canonical.
function decompress(bytes) {
  try {
    return Point.fromHex(Uint8Array.from(bytes), true)
  } catch {
    return null
  }
}

export function validatePublicKey(key) {
  if (!key || key.length !== 32) {
    throw new IdentityError('Invalid')
  }
  const point = decompress(key)
  if (!point || point.isSmallOrder()) {
    throw new IdentityError('Invalid')
  }
}

/**
 * `ed25519_dalek::VerifyingKey::verify_strict`: canonical s, R and A not of
 * small order, and the cofactorless equation compared on encoded R bytes.
 */
export function verify(key, transcript, signature) {
  validatePublicKey(key)
  if (!signature || signature.length !== 64) {
    throw new IdentityError('Authentication')
  }
  const A = decompress(key)
  const rBytes = Buffer.from(signature.subarray(0, 32))
  const s = bytesToNumberLE(signature.subarray(32, 64))
  if (s >= CURVE_ORDER) {
    throw new IdentityError('Authentication')
  }
  const R = decompress(rBytes)
  if (!R || R.isSmallOrder() || A.isSmallOrder()) {
    throw new IdentityError('Authentication')
  }
  const k =
    bytesToNumberLE(createHash('sha512').update(rBytes).update(Buffer.from(key)).update(transcript).digest()) %
    CURVE_ORDER
  const expected = Point.BASE.multiplyUnsafe(s).subtract(A.multiplyUnsafe(k))
  if (!Buffer.from(expected.toRawBytes()).equals(rBytes)) {
    throw new IdentityError('Authentication')
  }
}

export function pqxdhIdentityBindingTranscript(dhKey) {
  return Buffer.concat([PQXDH_IDENTITY_BINDING_DOMAIN, Buffer.from([PQXDH_EC_ENCODING_TAG]), dhKey])
}

export function pqxdhSignedPrekeyTranscript(identityDhKey, prekeyId, prekey) {
  return Buffer.concat([
    PQXDH_SIGNED_PREKEY_DOMAIN,
    Buffer.from([PQXDH_EC_ENCODING_TAG]),
    identityDhKey,
    u64be(prekeyId),
    Buffer.from([PQXDH_EC_ENCODING_TAG]),
    prekey,
  ])
}

export function pqxdhKemPrekeyTranscript(identityDhKey, prekeyId, oneTime, prekey) {
  return Buffer.concat([
    PQXDH_KEM_PREKEY_DOMAIN,
    Buffer.from([PQXDH_EC_ENCODING_TAG]),
    identityDhKey,
    u64be(prekeyId),
    Buffer.from([oneTime ? 1 : 0]),
    Buffer.from([PQXDH_KEM_ENCODING_TAG]),
    prekey,
  ])
}

export function devicePairingTranscript(userId, deviceId, mlsNodeId, publicKey, nonce) {
  if (isNilUuid(userId) || isNilUuid(deviceId) || isNilUuid(mlsNodeId)) {
    throw new IdentityError('Invalid')
  }
  validatePublicKey(publicKey)
  return Buffer.concat([
    Buffer.from('links/device-pairing/v1\0'),
    uuidBytes(userId),
    uuidBytes(deviceId),
    uuidBytes(mlsNodeId),
    publicKey,
    nonce,
  ])
}

function parseCertificateUuid(value) {
  const parsed = parseUuid(value)
  if (!parsed) {
    throw new IdentityError('Invalid')
  }
  return parsed
}

export function deviceSubcertificateTranscript(certificate) {
  try {
    validateDeviceSubcertificate({ ...certificate, signature: Buffer.alloc(64) })
  } catch {
    throw new IdentityError('Invalid')
  }
  const userId = parseCertificateUuid(certificate.user_id)
  const issuerDeviceId = parseCertificateUuid(certificate.issuer_device_id)
  const issuerMlsNodeId = parseCertificateUuid(certificate.issuer_mls_node_id)
  const subjectDeviceId = parseCertificateUuid(certificate.subject_device_id)
  const subjectMlsNodeId = parseCertificateUuid(certificate.subject_mls_node_id)
  if (certificate.issuer_public_key.length !== 32 || certificate.subject_public_key.length !== 32) {
    throw new IdentityError('Invalid')
  }
  validatePublicKey(certificate.issuer_public_key)
  validatePublicKey(certificate.subject_public_key)
  return Buffer.concat([
    DEVICE_SUBCERTIFICATE_DOMAIN,
    uuidBytes(userId),
    uuidBytes(issuerDeviceId),
    uuidBytes(issuerMlsNodeId),
    certificate.issuer_public_key,
    uuidBytes(subjectDeviceId),
    uuidBytes(subjectMlsNodeId),
    certificate.subject_public_key,
    u32be(certificate.delegation_role),
    u64be(certificate.issued_at_ms),
    u64be(certificate.expires_at_ms),
  ])
}

/** Verify issuer signature and validity window. */
export function verifyDeviceSubcertificate(certificate, nowMs) {
  try {
    validateDeviceSubcertificate(certificate)
  } catch {
    throw new IdentityError('Authentication')
  }
  if (nowMs < certificate.issued_at_ms || nowMs >= certificate.expires_at_ms) {
    throw new IdentityError('Authentication')
  }
  if (certificate.issuer_public_key.length !== 32) {
    throw new IdentityError('Authentication')
  }
  const transcript = deviceSubcertificateTranscript(certificate)
  try {
    verify(certificate.issuer_public_key, transcript, certificate.signature)
  } catch {
    throw new IdentityError('Authentication')
  }
}

function usernameTranscript(domain, challengeId, handle, deviceId, mlsNodeId, publicKey, challenge, expiresAtMs) {
  try {
    validateHandle(handle)
  } catch {
    throw new IdentityError('Invalid')
  }
  if (isNilUuid(challengeId) || isNilUuid(deviceId) || isNilUuid(mlsNodeId) || BigInt(expiresAtMs) === 0n) {
    throw new IdentityError('Invalid')
  }
  validatePublicKey(publicKey)
  return Buffer.concat([
    Buffer.from(domain),
    uuidBytes(challengeId),
    Buffer.from([handle.length]),
    Buffer.from(handle),
    uuidBytes(deviceId),
    uuidBytes(mlsNodeId),
    publicKey,
    challenge,
    u64be(expiresAtMs),
  ])
}

export function usernameRegistrationTranscript(...args) {
  return usernameTranscript('links/username-register/v2\0', ...args)
}

export function usernameLoginTranscript(...args) {
  return usernameTranscript('links/username-login/v2\0', ...args)
}

/**
 * RFC 9420 BasicCredential (credential_type=1) wrapping the application
 * identity. The 96-byte identity always uses MLS's two-byte varint prefix.
 */
export function mlsCredential({ userId, deviceId, mlsNodeId, publicKey }) {
  if (isNilUuid(userId) || isNilUuid(deviceId) || isNilUuid(mlsNodeId)) {
    throw new IdentityError('Invalid')
  }
  validatePublicKey(publicKey)
  const identity = Buffer.concat([
    DEVICE_IDENTITY_DOMAIN,
    uuidBytes(userId),
    uuidBytes(deviceId),
    uuidBytes(mlsNodeId),
    publicKey,
  ])
  return Buffer.concat([Buffer.from([0x00, 0x01]), mlsVarint(identity.length), identity])
}

function mlsVarint(length) {
  if (length < 0x40) {
    return Buffer.from([length])
  }
  if (length < 0x4000) {
    return Buffer.from([0x40 | (length >> 8), length & 0xff])
  }
  if (length < 0x40000000) {
    const bytes = Buffer.alloc(4)
    bytes.writeUInt32BE((0x80000000 | length) >>> 0)
    return bytes
  }
  throw new IdentityError('Invalid')
}
