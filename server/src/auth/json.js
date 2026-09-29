// Strict request-body decoding with serde semantics: `deny_unknown_fields`,
// required fields, exact integer types, and UUID parsing like the `uuid`
// crate. Any failure is an `invalid_request` rejection.
import { AuthError } from '../errors.js'
import { parseUuid } from '../protocol.js'

const NUMBER_SOURCE = Symbol('jsonNumberSource')
const strictUtf8 = new TextDecoder('utf-8', { fatal: true, ignoreBOM: true })

const INTEGER_RANGES = {
  u8: [0n, 255n],
  u16: [0n, 65_535n],
  u32: [0n, 4_294_967_295n],
  u64: [0n, 18_446_744_073_709_551_615n],
  i64: [-(2n ** 63n), 2n ** 63n - 1n],
}

function invalid() {
  return new AuthError('Invalid')
}

/** axum `Json` accepts `application/json` and `application/*+json`. */
export function isJsonContentType(header) {
  if (typeof header !== 'string') {
    return false
  }
  const essence = header.split(';')[0].trim().toLowerCase()
  const [type, subtype] = essence.split('/')
  return type === 'application' && subtype !== undefined && (subtype === 'json' || subtype.endsWith('+json'))
}

function parseText(body) {
  let text
  try {
    text = strictUtf8.decode(body)
  } catch {
    throw invalid()
  }
  try {
    return JSON.parse(text, (_key, value, context) =>
      typeof value === 'number' ? { [NUMBER_SOURCE]: context.source } : value
    )
  } catch {
    throw invalid()
  }
}

function decodeValue(spec, value) {
  if (typeof spec === 'object' && spec.optional) {
    return value === null ? null : decodeValue(spec.optional, value)
  }
  if (typeof spec === 'object' && spec.array) {
    if (!Array.isArray(value)) {
      throw invalid()
    }
    return value.map(item => decodeValue(spec.array, item))
  }
  if (typeof spec === 'object' && spec.oneOf) {
    if (typeof value !== 'string' || !spec.oneOf.includes(value)) {
      throw invalid()
    }
    return value
  }
  switch (spec) {
    case 'string':
      if (typeof value !== 'string') {
        throw invalid()
      }
      return value
    case 'uuid': {
      const uuid = parseUuid(value)
      if (!uuid) {
        throw invalid()
      }
      return uuid
    }
    case 'bool':
      if (typeof value !== 'boolean') {
        throw invalid()
      }
      return value
    default: {
      const range = INTEGER_RANGES[spec]
      const source = value?.[NUMBER_SOURCE]
      if (!range || typeof source !== 'string' || !/^-?(0|[1-9][0-9]*)$/.test(source)) {
        throw invalid()
      }
      const integer = BigInt(source)
      if (integer < range[0] || integer > range[1]) {
        throw invalid()
      }
      return spec === 'u64' || spec === 'i64' ? integer : Number(integer)
    }
  }
}

/**
 * Decode a JSON body into a struct described by `schema`, an ordered map of
 * field name to type. Optional fields (`{ optional: T }`) may be omitted.
 */
export function decodeJsonBody(body, contentType, schema) {
  if (!isJsonContentType(contentType)) {
    throw invalid()
  }
  const parsed = parseText(body)
  const fields = Object.entries(schema)
  const result = {}
  if (Array.isArray(parsed)) {
    // serde also accepts a struct as a positional sequence.
    const required = fields.filter(([, spec]) => !(typeof spec === 'object' && spec.optional)).length
    if (parsed.length < required || parsed.length > fields.length) {
      throw invalid()
    }
    fields.forEach(([name, spec], index) => {
      result[name] = index < parsed.length ? decodeValue(spec, parsed[index]) : null
    })
    return result
  }
  if (parsed === null || typeof parsed !== 'object' || parsed[NUMBER_SOURCE] !== undefined) {
    throw invalid()
  }
  for (const key of Object.keys(parsed)) {
    if (!Object.prototype.hasOwnProperty.call(schema, key)) {
      throw invalid()
    }
  }
  for (const [name, spec] of fields) {
    if (!Object.prototype.hasOwnProperty.call(parsed, name)) {
      if (typeof spec === 'object' && spec.optional) {
        result[name] = null
        continue
      }
      throw invalid()
    }
    result[name] = decodeValue(spec, parsed[name])
  }
  return result
}

/** Serialize a response body. BigInt values are emitted as JSON integers. */
export function stringify(value) {
  return JSON.stringify(value, (_key, item) =>
    typeof item === 'bigint' ? JSON.rawJSON(item.toString()) : item
  )
}
