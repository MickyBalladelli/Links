// Wire validation shared by the gateway, the mailbox store, and the auth API.
// Ported from crates/protocol; limits and checks must stay in lockstep.
import { zstdCompressSync, zstdDecompressSync, constants as zlibConstants } from 'node:zlib'

import { ProtocolError } from './errors.js'
import { create, decode, encode, encodedLen } from './proto.js'

export const VERSION = 1
export const MAX_ENVELOPE_BYTES = 256 * 1024
export const MAX_MESSAGE_BYTES = 64 * 1024
export const MAX_BATCH_ITEMS = 100
export const MAX_FRAME_BYTES = 1024 * 1024
export const MAX_WEBRTC_SDP_BYTES = 256 * 1024
export const MAX_WEBRTC_SDP_MID_BYTES = 128
export const MAX_PREKEY_UPLOAD_BYTES = 256 * 1024
export const MAX_ONE_TIME_PREKEYS = 100
export const ML_KEM_768_PUBLIC_KEY_BYTES = 1184
export const MAX_RETENTION_MS = 30n * 24n * 60n * 60n * 1000n
export const MAX_SUBCERTIFICATE_TTL_MS = 365n * 24n * 60n * 60n * 1000n
export const MAX_VERIFICATION_BADGE_TTL_MS = 365n * 24n * 60n * 60n * 1000n
export const MAX_CURSOR = 2n ** 63n - 1n
export const U64_MAX = 2n ** 64n - 1n
const ED25519_DID_KEY_MULTICODEC = Buffer.from([0xed, 0x01])

export const SYNC_COMPRESSION_ZSTD_DICTIONARY_V1 = 1
export const SYNC_COMPRESSION_DICTIONARY_ID_V1 = 1
export const SYNC_COMPRESSION_LEVEL = 3
export const SYNC_ZSTD_DICTIONARY_V1 = Buffer.from(
  'links.v1 SyncBatch QueueItem Envelope Tombstone cursor recipient_device_id ' +
    'after_cursor next_cursor high_watermark envelope_id expires_at_ms sealed_payload ' +
    'protocol_version acknowledged expired message_id conversation_id sender_device_id ' +
    'sequence_id sent_at_ms content text one-to-one mailbox replay cursor state sync',
  'latin1'
)

const CANONICAL_UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/
export const NIL_UUID = '00000000-0000-0000-0000-000000000000'

/**
 * Parse a UUID the way `uuid::Uuid::parse_str` does: hyphenated, simple,
 * braced, or URN form, any case. Returns the canonical lowercase hyphenated
 * form or null.
 */
export function parseUuid(value) {
  if (typeof value !== 'string') {
    return null
  }
  let text = value
  if (text.length === 45 && text.startsWith('urn:uuid:')) {
    text = text.slice(9)
  } else if (text.length === 38 && text.startsWith('{') && text.endsWith('}')) {
    text = text.slice(1, -1)
  }
  let hex
  if (text.length === 32) {
    hex = text
  } else if (
    text.length === 36 &&
    text[8] === '-' &&
    text[13] === '-' &&
    text[18] === '-' &&
    text[23] === '-'
  ) {
    hex = text.slice(0, 8) + text.slice(9, 13) + text.slice(14, 18) + text.slice(19, 23) + text.slice(24)
  } else {
    return null
  }
  if (!/^[0-9a-fA-F]{32}$/.test(hex)) {
    return null
  }
  hex = hex.toLowerCase()
  return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`
}

/** `uuid::Error` Display text for a string that `parseUuid` rejected. */
export function uuidParseError(value) {
  const bytes = Buffer.from(value, 'utf8')
  if (bytes.length === 0 || bytes.length > 45) {
    return `invalid length: found ${bytes.length}`
  }
  const chars = []
  let offset = 0
  for (const character of value) {
    chars.push([offset, character])
    offset += Buffer.byteLength(character, 'utf8')
  }
  let start = 0
  let end = bytes.length
  let format = 'any'
  if (value.startsWith('{') && value.endsWith('}') && value.length > 1) {
    start = 1
    end = bytes.length - 1
    format = 'braced'
  } else if (value.startsWith('urn:uuid:')) {
    start = 9
    format = 'urn'
  }
  let hyphens = 0
  const groupBounds = []
  for (const [index, character] of chars) {
    if (index < start || index >= end) {
      continue
    }
    const relative = index - start
    if (/^[0-9a-fA-F]$/.test(character)) {
      continue
    }
    if (character === '-') {
      if (format === 'any') {
        format = 'hyphenated'
      }
      if (hyphens < 4) {
        groupBounds[hyphens] = relative
      }
      hyphens += 1
      continue
    }
    return `invalid character: found \`${character}\` at ${index}`
  }
  if (format === 'any') {
    return `invalid length: found ${bytes.length}`
  }
  if (hyphens !== 4) {
    return `invalid group count: expected 5, found ${hyphens + 1}`
  }
  const blockStarts = [0, 9, 14, 19, 24]
  const expected = [8, 4, 4, 4, 12]
  for (let group = 0; group < 4; group += 1) {
    if (groupBounds[group] !== blockStarts[group + 1] - 1) {
      return `invalid group length in group ${group}: expected ${expected[group]}, found ${groupBounds[group] - blockStarts[group]}`
    }
  }
  return `invalid group length in group 4: expected 12, found ${bytes.length - blockStarts[4]}`
}

export function uuidBytes(uuid) {
  return Buffer.from(uuid.replaceAll('-', ''), 'hex')
}

export function uuidFromBytes(bytes) {
  const hex = Buffer.from(bytes).toString('hex')
  return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`
}

export function isNilUuid(uuid) {
  return uuid === NIL_UUID
}

export function validateId(value) {
  if (typeof value !== 'string' || !CANONICAL_UUID.test(value) || value === NIL_UUID) {
    throw new ProtocolError('Invalid', 'id')
  }
}

export function isValidId(value) {
  return typeof value === 'string' && CANONICAL_UUID.test(value) && value !== NIL_UUID
}

export function validateHandle(value) {
  if (
    typeof value !== 'string' ||
    value.length < 3 ||
    value.length > 32 ||
    !/^[a-z0-9_]+$/.test(value) ||
    !/^[a-z]/.test(value)
  ) {
    throw new ProtocolError('Invalid', 'handle')
  }
}

export function validateGatewayLocator(value) {
  if (!value || value.length > 128 || !/^[A-Za-z0-9:_.-]+$/.test(value)) {
    throw new ProtocolError('Invalid', 'gateway locator')
  }
}

function allZero(bytes) {
  return bytes.every(byte => byte === 0)
}

const BASE58_ALPHABET = '123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz'
function base58btcEncode(bytes) {
  const digits = [0]
  for (const byte of bytes) {
    let carry = byte
    for (let index = 0; index < digits.length; index += 1) {
      const value = digits[index] * 256 + carry
      digits[index] = value % 58
      carry = Math.floor(value / 58)
    }
    while (carry > 0) {
      digits.push(carry % 58)
      carry = Math.floor(carry / 58)
    }
  }
  let leadingZeroes = 0
  while (leadingZeroes < bytes.length && bytes[leadingZeroes] === 0) {
    leadingZeroes += 1
  }
  let encoded = '1'.repeat(leadingZeroes)
  for (let index = digits.length - 1; index >= 0; index -= 1) {
    encoded += BASE58_ALPHABET[digits[index]]
  }
  return encoded
}

export function didKeyForEd25519(publicKey) {
  if (publicKey.length !== 32 || allZero(publicKey)) {
    throw new ProtocolError('Invalid', 'ed25519 public key')
  }
  return `did:key:z${base58btcEncode(Buffer.concat([ED25519_DID_KEY_MULTICODEC, Buffer.from(publicKey)]))}`
}

export function validateWebRtcSignal(signal) {
  validateId(signal.session_id)
  validateId(signal.target_device_id)
  const sdpBytes = Buffer.byteLength(signal.sdp, 'utf8')
  if (
    !(signal.kind >= 1 && signal.kind <= 3) ||
    sdpBytes === 0 ||
    sdpBytes > MAX_WEBRTC_SDP_BYTES ||
    Buffer.byteLength(signal.sdp_mid, 'utf8') > MAX_WEBRTC_SDP_MID_BYTES
  ) {
    throw new ProtocolError('Invalid', 'WebRTC signal')
  }
}

export function validateWebRtcSignalDelivery(delivery) {
  validateId(delivery.request_id)
  validateId(delivery.sender_device_id)
  if (!delivery.signal) {
    throw new ProtocolError('Invalid', 'WebRTC signal')
  }
  validateWebRtcSignal(delivery.signal)
  if (delivery.signal.target_device_id === delivery.sender_device_id) {
    throw new ProtocolError('Invalid', 'WebRTC signal target')
  }
}

export function validateDeviceSubcertificate(certificate) {
  if (certificate.protocol_version !== VERSION) {
    throw new ProtocolError('UnsupportedVersion')
  }
  validateId(certificate.user_id)
  validateId(certificate.issuer_device_id)
  validateId(certificate.issuer_mls_node_id)
  validateId(certificate.subject_device_id)
  validateId(certificate.subject_mls_node_id)
  const issued = certificate.issued_at_ms
  const expires = certificate.expires_at_ms
  if (
    certificate.issuer_device_id === certificate.subject_device_id ||
    certificate.issuer_mls_node_id === certificate.subject_mls_node_id ||
    certificate.issuer_public_key.length !== 32 ||
    certificate.subject_public_key.length !== 32 ||
    allZero(certificate.issuer_public_key) ||
    allZero(certificate.subject_public_key) ||
    !(certificate.delegation_role === 1 || certificate.delegation_role === 2) ||
    issued === 0n ||
    expires <= issued ||
    expires - issued > MAX_SUBCERTIFICATE_TTL_MS ||
    certificate.signature.length !== 64 ||
    encodedLen('DeviceSubCertificate', certificate) > MAX_MESSAGE_BYTES
  ) {
    throw new ProtocolError('Invalid', 'device subcertificate')
  }
}

export function validateVerificationBadge(badge) {
  if (badge.protocol_version !== VERSION) {
    throw new ProtocolError('UnsupportedVersion')
  }
  validateId(badge.badge_id)
  validateId(badge.subject_user_id)
  if (badge.subject_handle != null) {
    validateHandle(badge.subject_handle)
  }
  const issued = badge.issued_at_ms
  const expires = badge.expires_at_ms
  if (
    badge.issuer_public_key.length !== 32 ||
    allZero(badge.issuer_public_key) ||
    !(badge.badge_kind === 1 || badge.badge_kind === 2) ||
    issued === 0n ||
    expires <= issued ||
    expires - issued > MAX_VERIFICATION_BADGE_TTL_MS ||
    badge.signature.length !== 64 ||
    encodedLen('VerificationBadge', badge) > MAX_MESSAGE_BYTES
  ) {
    throw new ProtocolError('Invalid', 'verification badge')
  }
}

/** Validate storage/routing shape without treating expiration as a wire error on replay. */
export function validateEnvelope(envelope) {
  if (envelope.protocol_version !== VERSION) {
    throw new ProtocolError('UnsupportedVersion')
  }
  validateId(envelope.envelope_id)
  validateId(envelope.recipient_device_id)
  if (envelope.sealed_payload.length === 0 || envelope.expires_at_ms === 0n) {
    throw new ProtocolError('Invalid', 'envelope')
  }
  if (encodedLen('Envelope', envelope) > MAX_ENVELOPE_BYTES) {
    throw new ProtocolError('TooLarge')
  }
}

/** Called when first accepting an envelope; never extend expiry on retry. */
export function validateEnqueue(envelope, nowMs) {
  validateEnvelope(envelope)
  if (envelope.expires_at_ms <= nowMs || envelope.expires_at_ms - nowMs > MAX_RETENTION_MS) {
    throw new ProtocolError('InvalidRetention')
  }
}

export function decodeEnvelope(bytes) {
  if (bytes.length > MAX_ENVELOPE_BYTES) {
    throw new ProtocolError('TooLarge')
  }
  const envelope = decode('Envelope', bytes)
  validateEnvelope(envelope)
  return envelope
}

function validateCurvePrekey(key) {
  if (key.id === 0n || key.id > MAX_CURSOR || key.public_key.length !== 32 || allZero(key.public_key)) {
    throw new ProtocolError('Invalid', 'curve prekey')
  }
}

function validateKemPrekey(key, oneTime) {
  if (
    key.id === 0n ||
    key.id > MAX_CURSOR ||
    key.one_time !== oneTime ||
    key.public_key.length !== ML_KEM_768_PUBLIC_KEY_BYTES ||
    key.signature.length !== 64
  ) {
    throw new ProtocolError('Invalid', 'KEM prekey')
  }
}

function validatePrekeyProfile(profile) {
  const identity = profile.identity
  if (!identity) {
    throw new ProtocolError('Invalid', 'PQXDH identity')
  }
  if (
    identity.signing_key.length !== 32 ||
    identity.dh_key.length !== 32 ||
    allZero(identity.dh_key) ||
    identity.binding_signature.length !== 64
  ) {
    throw new ProtocolError('Invalid', 'PQXDH identity')
  }
  const signed = profile.signed_prekey
  if (!signed || !signed.prekey) {
    throw new ProtocolError('Invalid', 'signed prekey')
  }
  validateCurvePrekey(signed.prekey)
  if (signed.signature.length !== 64) {
    throw new ProtocolError('Invalid', 'signed prekey signature')
  }
  if (!profile.last_resort_kem_prekey) {
    throw new ProtocolError('Invalid', 'last-resort KEM prekey')
  }
  validateKemPrekey(profile.last_resort_kem_prekey, false)
}

export function validatePrekeyUpload(upload) {
  if (upload.protocol_version !== VERSION) {
    throw new ProtocolError('UnsupportedVersion')
  }
  validateId(upload.device_id)
  validateId(upload.upload_id)
  if (
    upload.profile_revision === 0n ||
    upload.profile_revision > MAX_CURSOR ||
    encodedLen('PreKeyUpload', upload) > MAX_PREKEY_UPLOAD_BYTES ||
    upload.one_time_curve_prekeys.length > MAX_ONE_TIME_PREKEYS ||
    upload.one_time_kem_prekeys.length > MAX_ONE_TIME_PREKEYS
  ) {
    throw new ProtocolError('Invalid', 'prekey upload')
  }
  const profile = upload.profile
  if (!profile) {
    throw new ProtocolError('Invalid', 'prekey profile')
  }
  validatePrekeyProfile(profile)
  const curveIds = new Set([profile.signed_prekey.prekey.id])
  for (const key of upload.one_time_curve_prekeys) {
    validateCurvePrekey(key)
    if (curveIds.has(key.id)) {
      throw new ProtocolError('Invalid', 'duplicate curve prekey')
    }
    curveIds.add(key.id)
  }
  const kemIds = new Set([profile.last_resort_kem_prekey.id])
  for (const key of upload.one_time_kem_prekeys) {
    validateKemPrekey(key, true)
    if (kemIds.has(key.id)) {
      throw new ProtocolError('Invalid', 'duplicate KEM prekey')
    }
    kemIds.add(key.id)
  }
}

function kemPrekeysEqual(left, right) {
  return (
    left.id === right.id &&
    left.one_time === right.one_time &&
    left.public_key.equals(right.public_key) &&
    left.signature.equals(right.signature)
  )
}

export function validatePrekeyBundle(bundle) {
  if (bundle.protocol_version !== VERSION) {
    throw new ProtocolError('UnsupportedVersion')
  }
  validateId(bundle.device_id)
  if (bundle.profile_revision === 0n || bundle.profile_revision > MAX_CURSOR) {
    throw new ProtocolError('Invalid', 'prekey bundle')
  }
  const profile = bundle.profile
  if (!profile) {
    throw new ProtocolError('Invalid', 'prekey profile')
  }
  validatePrekeyProfile(profile)
  const signedId = profile.signed_prekey.prekey.id
  if (bundle.one_time_curve_prekey) {
    validateCurvePrekey(bundle.one_time_curve_prekey)
    if (bundle.one_time_curve_prekey.id === signedId) {
      throw new ProtocolError('Invalid', 'duplicate curve prekey')
    }
  }
  const kem = bundle.kem_prekey
  if (!kem) {
    throw new ProtocolError('Invalid', 'KEM prekey')
  }
  validateKemPrekey(kem, kem.one_time)
  const lastResort = profile.last_resort_kem_prekey
  if ((!kem.one_time && !kemPrekeysEqual(kem, lastResort)) || (kem.one_time && kem.id === lastResort.id)) {
    throw new ProtocolError('Invalid', 'KEM prekey selection')
  }
}

// Sync batch encoded length computed incrementally: a SyncBatch with items is
// its item-free header followed by one length-delimited field 5 per item.
function varintLen(value) {
  let length = 1
  let remaining = value
  while (remaining >= 128) {
    remaining = Math.floor(remaining / 128)
    length += 1
  }
  return length
}

export function queueItemFieldLen(item) {
  const itemLen = encodedLen('QueueItem', item)
  return 1 + varintLen(itemLen) + itemLen
}

export function syncBatchEncodedLen(batch) {
  const header = encodedLen('SyncBatch', { ...batch, items: [] })
  return batch.items.reduce((total, item) => total + queueItemFieldLen(item), header)
}

/** Validate the batch before compression and after decompression. */
export function validateSyncBatch(batch) {
  if (
    batch.recipient_device_id === '' ||
    batch.items.length > MAX_BATCH_ITEMS ||
    syncBatchEncodedLen(batch) > MAX_FRAME_BYTES - 128 ||
    batch.high_watermark > MAX_CURSOR ||
    batch.next_cursor > batch.high_watermark
  ) {
    throw new ProtocolError('Invalid', 'sync batch')
  }
  validateId(batch.recipient_device_id)
  let expected = batch.after_cursor
  for (const item of batch.items) {
    expected += 1n
    if (expected > U64_MAX || item.cursor !== expected) {
      throw new ProtocolError('Invalid', 'sync cursor')
    }
    if (item.entry === 'envelope') {
      validateEnvelope(item.envelope)
      if (item.envelope.recipient_device_id !== batch.recipient_device_id) {
        throw new ProtocolError('Invalid', 'sync recipient')
      }
    } else if (item.entry === 'tombstone' && (item.tombstone.reason === 1 || item.tombstone.reason === 2)) {
      // Expired or acknowledged tombstone.
    } else {
      throw new ProtocolError('Invalid', 'sync entry')
    }
  }
  if (batch.next_cursor !== expected || (batch.items.length === 0 && batch.next_cursor !== batch.high_watermark)) {
    throw new ProtocolError('Invalid', 'sync checkpoint')
  }
}

export function compressSyncBatch(batch) {
  validateSyncBatch(batch)
  const bytes = encode('SyncBatch', batch)
  let compressedPayload
  try {
    compressedPayload = zstdCompressSync(bytes, {
      params: { [zlibConstants.ZSTD_c_compressionLevel]: SYNC_COMPRESSION_LEVEL },
      dictionary: SYNC_ZSTD_DICTIONARY_V1,
    })
  } catch {
    throw new ProtocolError('Malformed')
  }
  if (compressedPayload.length > MAX_FRAME_BYTES - 128) {
    throw new ProtocolError('TooLarge')
  }
  if (bytes.length > 0xffffffff) {
    throw new ProtocolError('TooLarge')
  }
  const compressed = create('CompressedSyncBatch', {
    compression: SYNC_COMPRESSION_ZSTD_DICTIONARY_V1,
    dictionary_id: SYNC_COMPRESSION_DICTIONARY_ID_V1,
    uncompressed_size: bytes.length,
    compressed_payload: compressedPayload,
  })
  if (encodedLen('CompressedSyncBatch', compressed) > MAX_FRAME_BYTES - 128) {
    throw new ProtocolError('TooLarge')
  }
  return compressed
}

export function decompressSyncBatch(compressed) {
  const limit = MAX_FRAME_BYTES - 128
  const encoded = encodedLen('CompressedSyncBatch', compressed)
  if (
    compressed.compression !== SYNC_COMPRESSION_ZSTD_DICTIONARY_V1 ||
    compressed.dictionary_id !== SYNC_COMPRESSION_DICTIONARY_ID_V1 ||
    compressed.compressed_payload.length === 0 ||
    compressed.compressed_payload.length > limit ||
    compressed.uncompressed_size === 0 ||
    compressed.uncompressed_size > limit ||
    encoded > limit
  ) {
    throw new ProtocolError(compressed.uncompressed_size > limit || encoded > limit ? 'TooLarge' : 'Malformed')
  }
  let bytes
  try {
    bytes = zstdDecompressSync(compressed.compressed_payload, {
      dictionary: SYNC_ZSTD_DICTIONARY_V1,
      maxOutputLength: compressed.uncompressed_size,
    })
  } catch {
    throw new ProtocolError('Malformed')
  }
  if (bytes.length !== compressed.uncompressed_size) {
    throw new ProtocolError('Malformed')
  }
  const batch = decode('SyncBatch', bytes)
  validateSyncBatch(batch)
  return batch
}
