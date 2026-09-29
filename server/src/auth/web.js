// HTTP API for the account service. Ported from crates/account-auth/src/web.rs.
//
// Routing reproduces axum semantics: exact, case-sensitive paths; 405 with an
// Allow header when the path exists for another method; an empty 404
// otherwise. Extractor order is preserved so rejection precedence (path,
// query, body, bearer) produces the same status codes as the Rust server.
import express from 'express'

import { AuthError } from '../errors.js'
import { decode, encode as encodeProto } from '../proto.js'
import { MAX_FRAME_BYTES, MAX_PREKEY_UPLOAD_BYTES, parseUuid, uuidParseError } from '../protocol.js'
import { decodeJsonBody, stringify } from './json.js'
import { MAX_BLOB_BYTES } from './service.js'

const KB4 = 4096
const KB16 = 16 * 1024
const KB64 = 64 * 1024
const PROFILE_PICTURE_LIMIT = 131_072

const ERROR_RESPONSES = {
  Invalid: [400, 'invalid_request'],
  Denied: [401, 'authentication_failed'],
  NotFound: [404, 'not_found'],
  RateLimited: [429, 'rate_limited'],
  Conflict: [409, 'conflicting_write'],
  UsernameConflict: [409, 'username_exists'],
  DeviceConflict: [409, 'device_already_registered'],
  Unavailable: [503, 'temporarily_unavailable'],
}

class Rejection extends Error {
  constructor(status, text) {
    super(text)
    this.status = status
    this.text = text
  }
}

const S = {
  string: 'string',
  uuid: 'uuid',
  bool: 'bool',
  u8: 'u8',
  u16: 'u16',
  u32: 'u32',
  u64: 'u64',
}

const SCHEMAS = {
  usernameChallenge: {
    handle: S.string,
    purpose: { oneOf: ['registration', 'login'] },
    device_id: S.uuid,
    mls_node_id: S.uuid,
    public_key: S.string,
  },
  usernameProof: { challenge_id: S.uuid, signature: S.string },
  usernameChange: { handle: S.string },
  displayNameChange: { display_name: S.string },
  directoryProfileSync: { user_ids: { array: S.uuid } },
  contactPsiQuery: { protocol_version: S.u32, blinded_inputs: { array: S.string } },
  privacyPassIssue: {
    protocol_version: S.u32,
    token_type: S.u16,
    truncated_token_key_id: S.u8,
    blinded_message: S.string,
  },
  privacyPassRedeem: { protocol_version: S.u32, token: S.string, challenge: S.string },
  chatPowVerify: { protocol_version: S.u32, challenge: S.string, nonce: S.u64 },
  deviceRegistration: {
    device_id: S.uuid,
    mls_node_id: S.uuid,
    public_key: S.string,
    nonce: S.string,
    signature: S.string,
  },
  delegatedDeviceRegistration: { certificate: S.string, nonce: S.string, signature: S.string },
  createGroup: { group_id: S.uuid, kind: { optional: S.string } },
  organizationControls: { mini_apps_enabled: S.bool, bots_enabled: S.bool },
  setGroupRole: { role: S.string },
  adminUserStatus: { disabled: S.bool },
  passkeyRegistrationFinish: {
    challenge_id: S.uuid,
    credential_id: S.string,
    client_data_json: S.string,
    attestation_object: S.string,
  },
  passkeyAssertionFinish: {
    challenge_id: S.uuid,
    credential_id: S.string,
    client_data_json: S.string,
    authenticator_data: S.string,
    signature: S.string,
  },
  encryptedKeyBackup: {
    backup_id: S.uuid,
    device_id: S.uuid,
    credential_id: S.string,
    encrypted_envelope: S.string,
  },
}

function readBody(req, limit) {
  const declared = Number.parseInt(req.headers['content-length'] ?? '', 10)
  if (Number.isFinite(declared) && declared > limit) {
    req.resume()
    return Promise.resolve(null)
  }
  return new Promise((resolve, reject) => {
    const chunks = []
    let size = 0
    let exceeded = false
    req.on('data', chunk => {
      if (exceeded) {
        return
      }
      size += chunk.length
      if (size > limit) {
        exceeded = true
        chunks.length = 0
        return
      }
      chunks.push(chunk)
    })
    req.on('end', () => resolve(exceeded ? null : Buffer.concat(chunks)))
    req.on('error', reject)
  })
}

/** Bytes extractor: exceeding the limit is a 413 rejection. */
async function bytesBody(req, limit) {
  const body = await readBody(req, limit)
  if (body === null) {
    throw new Rejection(413, 'Failed to buffer the request body: length limit exceeded')
  }
  return body
}

/** `Result<Json<T>, JsonRejection>` mapped to `invalid_request`. */
async function jsonBody(req, limit, schema) {
  const body = await readBody(req, limit)
  if (body === null) {
    return () => {
      throw new AuthError('Invalid')
    }
  }
  try {
    const value = decodeJsonBody(body, req.headers['content-type'], schema)
    return () => value
  } catch (error) {
    return () => {
      throw error
    }
  }
}

function visibleAscii(value) {
  return typeof value === 'string' && /^[\t\x20-\x7e]*$/.test(value)
}

function bearer(req) {
  const header = req.headers.authorization
  if (!visibleAscii(header) || !header.startsWith('Bearer ')) {
    throw new AuthError('Denied')
  }
  return header.slice('Bearer '.length)
}

function requireProtobuf(req) {
  const header = req.headers['content-type']
  if (!visibleAscii(header) || header.split(';')[0].trim() !== 'application/x-protobuf') {
    throw new AuthError('Invalid')
  }
}

function pathUuid(req, ...names) {
  return names.map(name => {
    const value = req.params[name]
    const uuid = parseUuid(value)
    if (!uuid) {
      throw new Rejection(
        400,
        `Invalid URL: Cannot parse \`${name}\` with value \`${value}\`: UUID parsing failed: ${uuidParseError(value)}`
      )
    }
    return uuid
  })
}

/** Rust `i64::from_str` error text, or null when the value parses. */
function i64ParseError(text) {
  if (text === '') {
    return 'cannot parse integer from empty string'
  }
  if (!/^[+-]?[0-9]+$/.test(text)) {
    return 'invalid digit found in string'
  }
  const value = BigInt(text)
  if (value > 2n ** 63n - 1n) {
    return 'number too large to fit in target type'
  }
  if (value < -(2n ** 63n)) {
    return 'number too small to fit in target type'
  }
  return null
}

function adminUsersQuery(req) {
  const index = req.originalUrl.indexOf('?')
  const query = new URLSearchParams(index === -1 ? '' : req.originalUrl.slice(index + 1))
  const seen = new Set()
  for (const key of query.keys()) {
    if ((key === 'search' || key === 'limit') && seen.has(key)) {
      throw new Rejection(400, `Failed to deserialize query string: duplicate field \`${key}\``)
    }
    seen.add(key)
  }
  const search = query.get('search') ?? ''
  let limit = 100n
  if (query.has('limit')) {
    const text = query.get('limit')
    const error = i64ParseError(text)
    if (error) {
      throw new Rejection(400, `Failed to deserialize query string: limit: ${error}`)
    }
    limit = BigInt(text)
  }
  return { search, limit }
}

function requireAdmin(auth, req) {
  const key = req.headers['x-links-admin-key']
  if (!visibleAscii(key)) {
    throw new AuthError('Denied')
  }
  auth.authorizeAdminKey(key)
}

// setHeader and Buffer bodies keep Express from appending a charset to the
// Content-Type header, which axum does not send.
function sendJson(res, value, status = 200) {
  res.status(status).setHeader('content-type', 'application/json')
  res.send(Buffer.from(stringify(value)))
}

function sendNoContent(res) {
  res.status(204).end()
}

function sendProtobuf(res, typeName, message) {
  res.status(200).setHeader('content-type', 'application/x-protobuf')
  res.send(encodeProto(typeName, message))
}

function sendError(res, error) {
  if (error instanceof Rejection) {
    res.status(error.status).setHeader('content-type', 'text/plain; charset=utf-8')
    res.send(Buffer.from(error.text))
    return
  }
  const kind = error instanceof AuthError ? error.kind : 'Unavailable'
  const [status, code] = ERROR_RESPONSES[kind] ?? ERROR_RESPONSES.Unavailable
  if (status === 429) {
    res.set('retry-after', '3600')
  }
  sendJson(res, { error: code }, status)
}

function methodsWithHead(methods) {
  return methods.includes('GET') && !methods.includes('HEAD') ? [...methods, 'HEAD'] : methods
}

export function createAuthApp(auth) {
  const app = express()
  app.disable('x-powered-by')
  app.disable('etag')
  app.set('trust proxy', false)
  const router = express.Router({ strict: true, caseSensitive: true, mergeParams: false })

  app.use((req, res, next) => {
    res.set('cache-control', 'no-store')
    res.on('finish', () => {
      if (res.statusCode === 429) {
        console.error('Authentication rate limit saturated; inspect ingress and provider metrics.')
      }
    })
    next()
  })

  const routes = []
  function route(path, handlers) {
    routes.push([path, handlers])
  }

  const peer = req => req.socket.remoteAddress ?? ''

  route('/v1/auth/username/challenge', {
    POST: async (req, res) => {
      const request = await jsonBody(req, KB4, SCHEMAS.usernameChallenge)
      sendJson(res, await auth.startUsernameChallenge(request(), peer(req)))
    },
  })
  route('/v1/auth/username/register', {
    POST: async (req, res) => {
      const request = await jsonBody(req, KB4, SCHEMAS.usernameProof)
      sendJson(res, await auth.registerUsername(request(), peer(req)))
    },
  })
  route('/v1/auth/username/login', {
    POST: async (req, res) => {
      const request = await jsonBody(req, KB4, SCHEMAS.usernameProof)
      sendJson(res, await auth.loginUsername(request(), peer(req)))
    },
  })
  route('/v1/account/username', {
    GET: async (req, res) => sendJson(res, await auth.currentUsername(bearer(req))),
    PUT: async (req, res) => {
      const request = await jsonBody(req, KB4, SCHEMAS.usernameChange)
      const token = bearer(req)
      sendJson(res, await auth.changeUsername(token, request()))
    },
  })
  route('/v1/account/display-name', {
    GET: async (req, res) => sendJson(res, await auth.currentDisplayName(bearer(req))),
    PUT: async (req, res) => {
      const request = await jsonBody(req, KB4, SCHEMAS.displayNameChange)
      const token = bearer(req)
      sendJson(res, await auth.changeDisplayName(token, request()))
    },
  })
  route('/v1/account/devices', {
    GET: async (req, res) => sendJson(res, await auth.accountDevices(bearer(req))),
  })
  route('/v1/auth/logout', {
    POST: async (req, res) => {
      await auth.logout(bearer(req))
      sendNoContent(res)
    },
  })
  route('/v1/auth/sessions/others', {
    DELETE: async (req, res) => {
      await auth.revokeOtherSessions(bearer(req))
      sendNoContent(res)
    },
  })
  route('/v1/auth/me', {
    GET: async (req, res) => sendJson(res, await auth.authenticate(bearer(req))),
  })
  route('/v1/organization/controls', {
    GET: async (req, res) => sendJson(res, await auth.organizationControls(bearer(req))),
    PUT: async (req, res) => {
      const request = await jsonBody(req, KB4, SCHEMAS.organizationControls)
      const token = bearer(req)
      sendJson(res, await auth.setOrganizationControls(token, request()))
    },
  })
  route('/v1/devices', {
    POST: async (req, res) => {
      const request = await jsonBody(req, KB4, SCHEMAS.deviceRegistration)
      const token = bearer(req)
      sendJson(res, await auth.registerDevice(token, request()))
    },
  })
  route('/v1/devices/delegated', {
    POST: async (req, res) => {
      const request = await jsonBody(req, KB4, SCHEMAS.delegatedDeviceRegistration)
      const token = bearer(req)
      sendJson(res, await auth.registerDelegatedDevice(token, request()))
    },
  })
  route('/v1/devices/:device_id', {
    DELETE: async (req, res) => {
      const [deviceId] = pathUuid(req, 'device_id')
      sendJson(res, await auth.revokeDevice(bearer(req), deviceId))
    },
  })
  if (!auth.isLoopbackUsernameDev()) {
    // Phone OTP enrollment is compiled out of the loopback username
    // composition; the routes do not exist there.
    route('/v1/auth/start', { POST: async () => { throw new AuthError('Unavailable') } })
    route('/v1/auth/finish', { POST: async () => { throw new AuthError('Unavailable') } })
  }

  route('/v1/groups', {
    POST: async (req, res) => {
      const request = await jsonBody(req, KB4, SCHEMAS.createGroup)
      const token = bearer(req)
      sendJson(res, await auth.createGroup(token, request()))
    },
  })
  route('/v1/groups/:group_id/members', {
    GET: async (req, res) => {
      const [groupId] = pathUuid(req, 'group_id')
      sendJson(res, await auth.groupMembers(bearer(req), groupId))
    },
  })
  route('/v1/groups/:group_id/members/:user_id/role', {
    PUT: async (req, res) => {
      const [groupId, userId] = pathUuid(req, 'group_id', 'user_id')
      const request = await jsonBody(req, KB4, SCHEMAS.setGroupRole)
      const token = bearer(req)
      await auth.setGroupRole(token, groupId, userId, request())
      sendNoContent(res)
    },
  })
  route('/v1/groups/:group_id/members/:user_id', {
    DELETE: async (req, res) => {
      const [groupId, userId] = pathUuid(req, 'group_id', 'user_id')
      await auth.removeGroupMember(bearer(req), groupId, userId)
      sendNoContent(res)
    },
  })
  route('/v1/groups/:group_id', {
    DELETE: async (req, res) => {
      const [groupId] = pathUuid(req, 'group_id')
      await auth.deleteGroup(bearer(req), groupId)
      sendNoContent(res)
    },
  })

  route('/v1/passkeys/register/start', {
    POST: async (req, res) => sendJson(res, await auth.passkeyRegistrationStart(bearer(req))),
  })
  route('/v1/passkeys/register/finish', {
    POST: async (req, res) => {
      const request = await jsonBody(req, KB16, SCHEMAS.passkeyRegistrationFinish)
      const token = bearer(req)
      request()
      sendJson(res, await auth.passkeyRegistrationFinish(token))
    },
  })
  route('/v1/passkeys/assert/start', {
    POST: async (req, res) => sendJson(res, await auth.passkeyAssertionStart(bearer(req))),
  })
  route('/v1/passkeys/assert/finish', {
    POST: async (req, res) => {
      const request = await jsonBody(req, KB16, SCHEMAS.passkeyAssertionFinish)
      const token = bearer(req)
      request()
      sendJson(res, await auth.passkeyAssertionFinish(token))
    },
  })
  route('/v1/passkey-backups', {
    PUT: async (req, res) => {
      const request = await jsonBody(req, KB16, SCHEMAS.encryptedKeyBackup)
      const token = bearer(req)
      await auth.putEncryptedKeyBackup(token, request())
      sendNoContent(res)
    },
  })
  route('/v1/passkey-backups/:backup_id', {
    GET: async (req, res) => {
      const [backupId] = pathUuid(req, 'backup_id')
      sendJson(res, await auth.getEncryptedKeyBackup(bearer(req), backupId))
    },
  })

  route('/v1/prekeys', {
    PUT: async (req, res) => {
      const body = await bytesBody(req, MAX_PREKEY_UPLOAD_BYTES)
      requireProtobuf(req)
      let upload
      try {
        upload = decode('PreKeyUpload', body)
      } catch {
        throw new AuthError('Invalid')
      }
      sendProtobuf(res, 'PreKeyInventory', await auth.uploadPrekeys(bearer(req), upload))
    },
  })
  route('/v1/prekeys/status', {
    GET: async (req, res) => sendProtobuf(res, 'PreKeyInventory', await auth.prekeyInventory(bearer(req))),
  })
  route('/v1/prekeys/:device_id/claim', {
    POST: async (req, res) => {
      const [deviceId] = pathUuid(req, 'device_id')
      sendProtobuf(res, 'PreKeyBundle', await auth.claimPrekeyBundle(bearer(req), deviceId))
    },
  })

  route('/v1/mls/key-package', {
    PUT: async (req, res) => {
      const body = await bytesBody(req, MAX_FRAME_BYTES)
      await auth.putMlsKeyPackage(bearer(req), body)
      sendNoContent(res)
    },
  })
  route('/v1/mls/key-package/:device_id', {
    GET: async (req, res) => {
      const [deviceId] = pathUuid(req, 'device_id')
      const keyPackage = await auth.getMlsKeyPackage(bearer(req), deviceId)
      res.status(200).setHeader('content-type', 'application/x-protobuf')
      res.send(keyPackage)
    },
  })

  route('/v1/directory/users/:user_id', {
    GET: async (req, res) => {
      const [userId] = pathUuid(req, 'user_id')
      const directory = await auth.lookupUsernameDirectoryByUserId(bearer(req), userId, peer(req))
      if (directory) {
        sendJson(res, directory)
      } else {
        res.status(404).end()
      }
    },
  })
  route('/v1/directory/profiles/sync', {
    POST: async (req, res) => {
      const request = await jsonBody(req, KB16, SCHEMAS.directoryProfileSync)
      const token = bearer(req)
      sendJson(res, await auth.syncDirectoryProfiles(token, request().user_ids))
    },
  })
  route('/v1/directory/:handle/picture', {
    GET: async (req, res) => {
      const raw = req.params.handle
      const handle = raw.startsWith('@') ? raw.slice(1) : raw
      const jpeg = await auth.lookupProfilePicture(handle, peer(req))
      if (jpeg) {
        res.status(200).setHeader('content-type', 'image/jpeg')
        res.send(jpeg)
      } else {
        res.status(404).end()
      }
    },
  })
  route('/v1/directory/:handle', {
    GET: async (req, res) => {
      const raw = req.params.handle
      const handle = raw.startsWith('@') ? raw.slice(1) : raw
      if (handle === '' || handle.startsWith('@')) {
        throw new AuthError('Invalid')
      }
      const directory = await auth.lookupUsernameDirectory(handle, peer(req))
      if (directory) {
        sendJson(res, directory)
      } else {
        res.status(404).end()
      }
    },
  })

  route('/v1/profile/picture', {
    PUT: async (req, res) => {
      const body = await bytesBody(req, PROFILE_PICTURE_LIMIT)
      await auth.putProfilePicture(bearer(req), body)
      sendNoContent(res)
    },
    DELETE: async (req, res) => {
      await auth.deleteProfilePicture(bearer(req))
      sendNoContent(res)
    },
  })

  route('/v1/blobs/:attachment_id', {
    PUT: async (req, res) => {
      const attachmentId = req.params.attachment_id
      const body = await bytesBody(req, MAX_BLOB_BYTES)
      if (req.headers['content-type'] !== 'application/octet-stream') {
        throw new AuthError('Invalid')
      }
      const { size, digest } = await auth.putEncryptedAttachment(bearer(req), attachmentId, body)
      res.set('x-links-ciphertext-size', String(size))
      res.set('x-links-ciphertext-sha256', digest.toString('hex'))
      sendNoContent(res)
    },
    GET: async (req, res) => {
      const attachmentId = req.params.attachment_id
      const { ciphertext, digest } = await auth.getEncryptedAttachment(bearer(req), attachmentId)
      res.setHeader('content-type', 'application/octet-stream')
      res.set('x-links-ciphertext-size', String(ciphertext.length))
      res.set('x-links-ciphertext-sha256', digest.toString('hex'))
      res.status(200).send(ciphertext)
    },
  })

  route('/v1/admin/users', {
    GET: async (req, res) => {
      const query = adminUsersQuery(req)
      requireAdmin(auth, req)
      sendJson(res, await auth.adminUsers(query.search, query.limit))
    },
  })
  route('/v1/admin/users/:user_id', {
    DELETE: async (req, res) => {
      const [userId] = pathUuid(req, 'user_id')
      requireAdmin(auth, req)
      await auth.adminDeleteUser(userId)
      sendNoContent(res)
    },
  })
  route('/v1/admin/users/:user_id/status', {
    PUT: async (req, res) => {
      const [userId] = pathUuid(req, 'user_id')
      const request = await jsonBody(req, KB16, SCHEMAS.adminUserStatus)
      requireAdmin(auth, req)
      await auth.adminSetUserDisabled(userId, request().disabled)
      sendNoContent(res)
    },
  })
  route('/v1/admin/users/:user_id/devices/:device_id', {
    DELETE: async (req, res) => {
      const [userId, deviceId] = pathUuid(req, 'user_id', 'device_id')
      requireAdmin(auth, req)
      await auth.adminRevokeDevice(userId, deviceId)
      sendNoContent(res)
    },
  })

  route('/v1/contact-discovery/parameters', {
    GET: async (req, res) => sendJson(res, await auth.contactPsiParameters(bearer(req), peer(req))),
  })
  route('/v1/contact-discovery/query', {
    POST: async (req, res) => {
      const request = await jsonBody(req, KB64, SCHEMAS.contactPsiQuery)
      const token = bearer(req)
      sendJson(res, await auth.contactPsiQuery(token, peer(req), request()))
    },
  })

  route('/v1/privacy-pass/parameters', {
    GET: async (_req, res) => sendJson(res, auth.privacyPassParameters()),
  })
  route('/v1/privacy-pass/challenge', {
    GET: async (req, res) => sendJson(res, await auth.privacyPassChallenge(peer(req))),
  })
  route('/v1/privacy-pass/issue', {
    POST: async (req, res) => {
      const request = await jsonBody(req, KB4, SCHEMAS.privacyPassIssue)
      const token = bearer(req)
      sendJson(res, await auth.privacyPassIssue(token, peer(req), request()))
    },
  })
  route('/v1/privacy-pass/redeem', {
    POST: async (req, res) => {
      const request = await jsonBody(req, KB4, SCHEMAS.privacyPassRedeem)
      sendJson(res, await auth.privacyPassRedeem(peer(req), request()))
    },
  })

  route('/v1/chat-requests/proof-of-work/challenge', {
    GET: async (req, res) => sendJson(res, await auth.chatPowChallenge(bearer(req), peer(req))),
  })
  route('/v1/chat-requests/proof-of-work/verify', {
    POST: async (req, res) => {
      const request = await jsonBody(req, KB4, SCHEMAS.chatPowVerify)
      const token = bearer(req)
      sendJson(res, await auth.chatPowVerify(token, peer(req), request()))
    },
  })

  for (const [path, handlers] of routes) {
    const allowed = methodsWithHead(Object.keys(handlers))
    router.all(path, async (req, res) => {
      const method = req.method === 'HEAD' && !handlers.HEAD ? 'GET' : req.method
      const handler = handlers[method]
      if (!handler) {
        res.status(405).set('allow', allowed.join(',')).end()
        return
      }
      try {
        await handler(req, res)
      } catch (error) {
        if (!res.headersSent) {
          sendError(res, error)
        }
      }
    })
  }

  app.use(router)
  app.use((_req, res) => {
    res.status(404).end()
  })
  app.use((error, _req, res, _next) => {
    if (res.headersSent) {
      return
    }
    const status = error?.status ?? error?.statusCode
    res.status(status >= 400 && status < 500 ? 400 : 500).end()
  })
  return app
}
