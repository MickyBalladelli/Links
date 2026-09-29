// links.v1 protobuf codec with prost-compatible wire behavior.
//
// Messages are plain objects with snake_case field names, BigInt for 64-bit
// integers, and Buffer for bytes. Encoding follows prost: fields in field
// number order, implicit-presence scalars omitted when they hold the default,
// explicit-presence (`optional` and oneof) values always written. Mailbox
// fingerprints and pre-key upload digests hash these bytes, so they must match
// the Rust encoder exactly.
import { readdirSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import Long from 'long'
import protobuf from 'protobufjs'

import { ProtocolError } from './errors.js'

protobuf.util.Long = Long
protobuf.configure()

const strictUtf8 = new TextDecoder('utf-8', { fatal: true })
function strictString() {
  const bytes = this.bytes()
  return strictUtf8.decode(bytes)
}
protobuf.Reader.prototype.string = strictString
protobuf.BufferReader.prototype.string = strictString

const repoRoot = join(dirname(fileURLToPath(import.meta.url)), '..', '..')
export const PROTO_ROOT = join(repoRoot, 'proto')

const root = new protobuf.Root()
root.resolvePath = (_origin, target) => join(PROTO_ROOT, target)
const protoFiles = readdirSync(join(PROTO_ROOT, 'links', 'v1'))
  .filter(name => name.endsWith('.proto'))
  .sort()
  .map(name => `links/v1/${name}`)
root.loadSync(protoFiles, { keepCase: true })
root.resolveAll()

const LONG_TYPES = new Set(['uint64', 'int64', 'sint64', 'fixed64', 'sfixed64'])
const UNSIGNED_LONG_TYPES = new Set(['uint64', 'fixed64'])

const typeCache = new Map()
export function messageType(name) {
  let type = typeCache.get(name)
  if (!type) {
    type = root.lookupType(`links.v1.${name}`)
    typeCache.set(name, type)
  }
  return type
}

export function enumValues(name) {
  return root.lookupEnum(`links.v1.${name}`).values
}

function isSyntheticOneof(oneof) {
  return oneof.fieldsArray.length === 1 && oneof.name === `_${oneof.fieldsArray[0].name}`
}

function scalarDefault(field) {
  if (field.resolvedType instanceof protobuf.Enum) {
    return 0
  }
  switch (field.type) {
    case 'string':
      return ''
    case 'bytes':
      return Buffer.alloc(0)
    case 'bool':
      return false
    default:
      return LONG_TYPES.has(field.type) ? 0n : 0
  }
}

function scalarToPlain(field, value) {
  if (LONG_TYPES.has(field.type)) {
    if (Long.isLong(value)) {
      return BigInt(value.toString())
    }
    return BigInt(value)
  }
  if (field.type === 'bytes') {
    return Buffer.from(value.buffer, value.byteOffset, value.byteLength)
  }
  return value
}

function valueToPlain(field, value) {
  if (field.resolvedType instanceof protobuf.Type) {
    return toPlain(field.resolvedType, value)
  }
  return scalarToPlain(field, value)
}

function toPlain(type, message) {
  const plain = {}
  for (const field of type.fieldsArray) {
    const present = Object.prototype.hasOwnProperty.call(message, field.name) && message[field.name] != null
    if (field.repeated) {
      plain[field.name] = present ? message[field.name].map(value => valueToPlain(field, value)) : []
    } else if (field.partOf || field.resolvedType instanceof protobuf.Type) {
      plain[field.name] = present ? valueToPlain(field, message[field.name]) : null
    } else {
      plain[field.name] = present ? scalarToPlain(field, message[field.name]) : scalarDefault(field)
    }
  }
  for (const oneof of type.oneofsArray) {
    if (isSyntheticOneof(oneof)) {
      continue
    }
    const selected = oneof.fieldsArray.filter(field => plain[field.name] != null)
    // prost keeps the last oneof member seen on the wire. protobufjs keeps
    // every member it saw, so keep only one to match the Rust enum shape.
    const chosen = selected.length ? selected[selected.length - 1].name : null
    for (const field of oneof.fieldsArray) {
      if (field.name !== chosen) {
        plain[field.name] = null
      }
    }
    plain[oneof.name] = chosen
  }
  return plain
}

function isDefaultScalar(field, value) {
  if (value == null) {
    return true
  }
  if (field.type === 'bytes') {
    return value.length === 0
  }
  if (field.type === 'string') {
    return value === ''
  }
  if (field.type === 'bool') {
    return value === false
  }
  if (LONG_TYPES.has(field.type)) {
    return BigInt(Long.isLong(value) ? value.toString() : value) === 0n
  }
  return value === 0
}

function scalarToWire(field, value) {
  if (LONG_TYPES.has(field.type)) {
    const text = Long.isLong(value) ? value.toString() : BigInt(value).toString()
    return Long.fromString(text, UNSIGNED_LONG_TYPES.has(field.type))
  }
  if (field.type === 'bytes') {
    return Buffer.isBuffer(value) ? value : Buffer.from(value)
  }
  return value
}

function valueToWire(field, value) {
  if (field.resolvedType instanceof protobuf.Type) {
    return toWire(field.resolvedType, value)
  }
  return scalarToWire(field, value)
}

function toWire(type, plain) {
  const wire = {}
  for (const field of type.fieldsArray) {
    const value = plain[field.name]
    if (field.repeated) {
      if (value?.length) {
        wire[field.name] = value.map(item => valueToWire(field, item))
      }
    } else if (field.partOf || field.resolvedType instanceof protobuf.Type) {
      if (value != null) {
        wire[field.name] = valueToWire(field, value)
      }
    } else if (!isDefaultScalar(field, value)) {
      wire[field.name] = scalarToWire(field, value)
    }
  }
  return wire
}

export function encode(typeName, plain) {
  const type = messageType(typeName)
  return Buffer.from(type.encode(toWire(type, plain)).finish())
}

export function encodedLen(typeName, plain) {
  return encode(typeName, plain).length
}

/** Decode wire bytes. Throws ProtocolError('Malformed') for any decode failure. */
export function decode(typeName, bytes) {
  const type = messageType(typeName)
  let message
  try {
    message = type.decode(bytes)
  } catch {
    throw new ProtocolError('Malformed')
  }
  return toPlain(type, message)
}

/** Build a fully-populated plain message from a partial object. */
export function create(typeName, partial = {}) {
  const type = messageType(typeName)
  const plain = {}
  for (const field of type.fieldsArray) {
    if (Object.prototype.hasOwnProperty.call(partial, field.name)) {
      plain[field.name] = partial[field.name]
    } else if (field.repeated) {
      plain[field.name] = []
    } else if (field.partOf || field.resolvedType instanceof protobuf.Type) {
      plain[field.name] = null
    } else {
      plain[field.name] = scalarDefault(field)
    }
  }
  for (const oneof of type.oneofsArray) {
    if (isSyntheticOneof(oneof)) {
      continue
    }
    const chosen = oneof.fieldsArray.find(field => plain[field.name] != null)
    plain[oneof.name] = chosen ? chosen.name : null
  }
  return plain
}

export function bytesEqual(left, right) {
  return Buffer.compare(Buffer.from(left), Buffer.from(right)) === 0
}

/** Deep structural equality over plain messages (prost `PartialEq`). */
export function messagesEqual(left, right) {
  if (left === right) {
    return true
  }
  if (left == null || right == null) {
    return false
  }
  if (Buffer.isBuffer(left) || Buffer.isBuffer(right)) {
    return Buffer.isBuffer(left) && Buffer.isBuffer(right) && left.equals(right)
  }
  if (Array.isArray(left)) {
    return Array.isArray(right) && left.length === right.length && left.every((item, index) => messagesEqual(item, right[index]))
  }
  if (typeof left === 'object') {
    const keys = Object.keys(left)
    return keys.length === Object.keys(right).length && keys.every(key => messagesEqual(left[key], right[key]))
  }
  return false
}
