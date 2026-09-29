// Strict base64url (no padding, canonical trailing bits) matching the Rust
// `base64::engine::general_purpose::URL_SAFE_NO_PAD` engine.
import { AuthError } from '../errors.js'

export function encode(bytes) {
  return Buffer.from(bytes).toString('base64url')
}

export function decodeBase64Url(value) {
  if (typeof value !== 'string' || !/^[A-Za-z0-9_-]*$/.test(value) || value.length % 4 === 1) {
    return null
  }
  const bytes = Buffer.from(value, 'base64url')
  if (bytes.toString('base64url') !== value) {
    return null
  }
  return bytes
}

/** `decode::<N>`: exactly N bytes, with an input length guard. */
export function decodeFixed(value, length) {
  if (typeof value !== 'string' || Buffer.byteLength(value, 'utf8') > length * 2) {
    throw new AuthError('Invalid')
  }
  const bytes = decodeBase64Url(value)
  if (!bytes || bytes.length !== length) {
    throw new AuthError('Invalid')
  }
  return bytes
}

/** `decode_blob`: 1..=max bytes, with an input length guard. */
export function decodeBlob(value, max) {
  if (typeof value !== 'string' || Buffer.byteLength(value, 'utf8') > max * 2) {
    throw new AuthError('Invalid')
  }
  const bytes = decodeBase64Url(value)
  if (!bytes || bytes.length === 0 || bytes.length > max) {
    throw new AuthError('Invalid')
  }
  return bytes
}

const WHITE_SPACE_EDGES = /^\p{White_Space}+|\p{White_Space}+$/gu

/** Rust `str::trim`: strips Unicode White_Space only. */
export function rustTrim(value) {
  return value.replace(WHITE_SPACE_EDGES, '')
}
