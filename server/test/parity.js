#!/usr/bin/env node
// Differential test: run one scenario against the Rust and Node servers and
// compare every observable result (status, headers, bodies, frames).
//
//   node test/parity.js RUST_AUTH RUST_GATEWAY NODE_AUTH NODE_GATEWAY
//
// Both servers must use the same AUTH_LOOKUP_KEY and LINKS_ADMIN_KEY. Values
// that are random per run (IDs, tokens, timestamps, nonces) are normalized;
// deterministic crypto outputs are compared byte-for-byte across servers.
import { createHash, randomBytes, randomUUID } from 'node:crypto'
import { zstdDecompressSync } from 'node:zlib'
import { RistrettoPoint, ed25519 } from '@noble/curves/ed25519'
import { hashToCurve, p384 } from '@noble/curves/p384'

import * as pp from '../src/crypto/privacyPass.js'
import { devicePairingTranscript, pqxdhIdentityBindingTranscript, pqxdhKemPrekeyTranscript, pqxdhSignedPrekeyTranscript } from '../src/identity.js'
import { create, decode, encode } from '../src/proto.js'
import { SYNC_ZSTD_DICTIONARY_V1, uuidBytes } from '../src/protocol.js'
import { FrameSocket, b64, http, loginAccount, registerAccount, testHandle } from './client.js'

const [rustAuth, rustGateway, nodeAuth, nodeGateway] = process.argv.slice(2)
if (!nodeGateway) {
  console.error('usage: parity.js RUST_AUTH RUST_GATEWAY NODE_AUTH NODE_GATEWAY')
  process.exit(2)
}
const adminKey = process.env.LINKS_ADMIN_KEY

const UUID = /[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}/g

function jsonSafe(value) {
  return JSON.parse(
    JSON.stringify(value, (_key, item) => {
      if (typeof item === 'bigint') {
        return `${item}n`
      }
      if (item && item.type === 'Buffer' && Array.isArray(item.data)) {
        return `bytes(${item.data.length})`
      }
      return item
    })
  )
}

class Recorder {
  constructor(name) {
    this.name = name
    this.entries = []
    this.exact = {}
  }

  record(label, value) {
    this.entries.push([label, normalize(jsonSafe(value))])
  }

  http(label, response, { keepBody = false } = {}) {
    const headers = {}
    for (const name of ['content-type', 'cache-control', 'allow', 'retry-after', 'x-links-ciphertext-size', 'x-links-ciphertext-sha256']) {
      const value = response.headers.get(name)
      if (value !== null) {
        headers[name] = name === 'allow' ? value.split(',').sort().join(',') : value
      }
    }
    let body
    if (response.json !== null && response.json !== undefined) {
      body = response.json
    } else if (response.bytes.length) {
      const text = keepBody || (response.headers.get('content-type') ?? '').startsWith('text/plain')
      body = text ? response.bytes.toString('utf8') : `bytes(${response.bytes.length})`
    } else {
      body = ''
    }
    this.record(label, { status: response.status, headers, body })
    return response
  }
}

// Replace per-run values with stable placeholders.
function normalize(value, key = '') {
  if (Array.isArray(value)) {
    return value.map(item => normalize(item, key))
  }
  if (value && typeof value === 'object') {
    const out = {}
    for (const [name, item] of Object.entries(value)) {
      out[name] = normalize(item, name)
    }
    return out
  }
  if (typeof value === 'number' && /(_at_ms|expires|_ms)$/.test(key) && value > 1e12) {
    return '<time>'
  }
  if (typeof value === 'string') {
    if (/_at$/.test(key) && value !== null) {
      return '<timestamp>'
    }
    if (['access_token', 'challenge', 'nonce', 'proof', 'public_key', 'mls_credential', 'identity_public_key', 'did', 'delegation_certificate', 'evaluated_message', 'evaluated_point', 'handle', 'x-links-ciphertext-sha256', 'credential_id', 'encrypted_envelope'].includes(key)) {
      return `<${key}>`
    }
    if (/^\d+n$/.test(value) && /(_at_ms|expires)/.test(key)) {
      return '<time>'
    }
    return value
      .replace(UUID, '<uuid>')
      .replace(/(alice|bob|carol|dan)_[0-9a-f]{24}/g, '<handle>')
      .replace(/with value `[^`]*[0-9a-f]{6}[^`]*`/g, 'with value `<random>`')
  }
  return value
}

// --- client-side crypto helpers (VOPRF clients) ------------------------------

function sha512(...parts) {
  const hash = createHash('sha512')
  parts.forEach(part => hash.update(part))
  return hash.digest()
}

function psiBlind(phone) {
  const input = createHash('sha256').update('links/contact-psi/input/v1\0').update(phone).digest()
  const point = RistrettoPoint.hashToCurve(Uint8Array.from(sha512(Buffer.from('links/contact-psi/hash-to-group/v1\0'), input)))
  const blind = (BigInt(`0x${Buffer.from(randomBytes(64)).reverse().toString('hex')}`) % ed25519.CURVE.n) || 1n
  return { blinded: Buffer.from(point.multiply(blind).toRawBytes()), blind }
}

function psiUnblind(evaluated, blind) {
  const inverse = modInverse(blind, ed25519.CURVE.n)
  return Buffer.from(RistrettoPoint.fromHex(Uint8Array.from(evaluated)).multiply(inverse).toRawBytes())
}

function psiVerifyProof(serverPublicKey, blinded, evaluated, proof) {
  const n = ed25519.CURVE.n
  const le = bytes => BigInt(`0x${Buffer.from(bytes).reverse().toString('hex')}`)
  const c = le(proof.subarray(0, 32))
  const s = le(proof.subarray(32))
  const G = RistrettoPoint.BASE
  const K = RistrettoPoint.fromHex(Uint8Array.from(serverPublicKey))
  const B = RistrettoPoint.fromHex(Uint8Array.from(blinded))
  const E = RistrettoPoint.fromHex(Uint8Array.from(evaluated))
  const mul = (P, k) => (k % n === 0n ? RistrettoPoint.ZERO : P.multiply(k % n))
  const a = mul(G, s).subtract(mul(K, c))
  const b = mul(B, s).subtract(mul(E, c))
  const expected =
    le(sha512(Buffer.from('links/contact-psi/dleq/v1\0'), serverPublicKey, blinded, evaluated, Buffer.from(a.toRawBytes()), Buffer.from(b.toRawBytes()))) % n
  return expected === c
}

function modInverse(value, modulus) {
  let [a, b, x, y] = [value % modulus, modulus, 1n, 0n]
  while (b) {
    const q = a / b
    ;[a, b] = [b, a - q * b]
    ;[x, y] = [y, x - q * y]
  }
  return ((x % modulus) + modulus) % modulus
}

function lengthPrefixed(bytes) {
  const prefix = Buffer.alloc(2)
  prefix.writeUInt16BE(bytes.length)
  return Buffer.concat([prefix, Buffer.from(bytes)])
}

function privacyPassBlind(challenge, tokenKeyId) {
  const nonce = randomBytes(32)
  const challengeDigest = createHash('sha256').update(challenge).digest()
  const tokenType = Buffer.from([0x00, 0x01])
  const tokenInput = Buffer.concat([tokenType, nonce, challengeDigest, tokenKeyId])
  const element = p384.ProjectivePoint.fromAffine(
    hashToCurve(Uint8Array.from(tokenInput), { DST: Buffer.from('HashToGroup-OPRFV1-\x01-P384-SHA384', 'latin1') }).toAffine()
  )
  const blind = (BigInt(`0x${randomBytes(48).toString('hex')}`) % (p384.CURVE.n - 1n)) + 1n
  return { nonce, challengeDigest, tokenInput, blind, blinded: Buffer.from(element.multiply(blind).toRawBytes(true)) }
}

function privacyPassFinalize(state, evaluatedMessage, tokenKeyId) {
  const evaluated = p384.ProjectivePoint.fromHex(Uint8Array.from(evaluatedMessage))
  const unblinded = Buffer.from(evaluated.multiply(modInverse(state.blind, p384.CURVE.n)).toRawBytes(true))
  const authenticator = createHash('sha384')
    .update(Buffer.concat([lengthPrefixed(state.tokenInput), lengthPrefixed(unblinded), Buffer.from('Finalize')]))
    .digest()
  return Buffer.concat([Buffer.from([0x00, 0x01]), state.nonce, state.challengeDigest, tokenKeyId, authenticator])
}

function solvePow(challenge, bits) {
  for (let nonce = 0n; ; nonce += 1n) {
    const nonceBytes = Buffer.alloc(8)
    nonceBytes.writeBigUInt64BE(nonce)
    const digest = createHash('sha256').update('links/chat-request-pow/v1\0').update(challenge).update(nonceBytes).digest()
    let zeros = 0
    for (const byte of digest) {
      const z = byte === 0 ? 8 : Math.clz32(byte) - 24
      zeros += z
      if (z < 8) {
        break
      }
    }
    if (zeros >= bits) {
      return nonce
    }
  }
}

function sign(account, transcript) {
  return Buffer.from(ed25519.sign(transcript, account.secret))
}

function prekeyUpload(account, { revision = 1n, uploadId = randomUUID(), dh, oneTimeCurve = 3, oneTimeKem = 2 } = {}) {
  const dhKey = dh ?? randomBytes(32)
  const signedPrekey = randomBytes(32)
  const kem = (id, oneTime) => {
    const publicKey = randomBytes(1184)
    return create('KemPreKey', {
      id,
      public_key: publicKey,
      one_time: oneTime,
      signature: sign(account, pqxdhKemPrekeyTranscript(dhKey, id, oneTime, publicKey)),
    })
  }
  return create('PreKeyUpload', {
    protocol_version: 1,
    device_id: account.deviceId,
    profile_revision: revision,
    profile: create('PreKeyProfile', {
      identity: create('PqxdhPublicIdentity', {
        signing_key: account.publicKey,
        dh_key: dhKey,
        binding_signature: sign(account, pqxdhIdentityBindingTranscript(dhKey)),
      }),
      signed_prekey: create('SignedCurvePreKey', {
        prekey: create('CurvePreKey', { id: 1n, public_key: signedPrekey }),
        signature: sign(account, pqxdhSignedPrekeyTranscript(dhKey, 1n, signedPrekey)),
      }),
      last_resort_kem_prekey: kem(9_000_000_000_000_000_001n, false),
    }),
    one_time_curve_prekeys: Array.from({ length: oneTimeCurve }, (_, index) =>
      create('CurvePreKey', { id: 9_007_199_254_740_993n + BigInt(index), public_key: randomBytes(32) })
    ),
    one_time_kem_prekeys: Array.from({ length: oneTimeKem }, (_, index) => kem(100n + BigInt(index), true)),
    upload_id: uploadId,
  })
}

function subcertificate(issuer, subject, role, now) {
  const certificate = create('DeviceSubCertificate', {
    protocol_version: 1,
    user_id: issuer.userId,
    issuer_device_id: issuer.deviceId,
    issuer_mls_node_id: issuer.mlsNodeId,
    issuer_public_key: issuer.publicKey,
    subject_device_id: subject.deviceId,
    subject_mls_node_id: subject.mlsNodeId,
    subject_public_key: subject.publicKey,
    delegation_role: role,
    issued_at_ms: BigInt(now - 1000),
    expires_at_ms: BigInt(now + 86_400_000),
    signature: Buffer.alloc(64),
  })
  const u32 = Buffer.alloc(4)
  u32.writeUInt32BE(role)
  const u64 = value => {
    const bytes = Buffer.alloc(8)
    bytes.writeBigUInt64BE(value)
    return bytes
  }
  const transcript = Buffer.concat([
    Buffer.from('links/device-subcertificate/v1\0'),
    uuidBytes(issuer.userId),
    uuidBytes(issuer.deviceId),
    uuidBytes(issuer.mlsNodeId),
    issuer.publicKey,
    uuidBytes(subject.deviceId),
    uuidBytes(subject.mlsNodeId),
    subject.publicKey,
    u32,
    u64(certificate.issued_at_ms),
    u64(certificate.expires_at_ms),
  ])
  certificate.signature = sign(issuer, transcript)
  return encode('DeviceSubCertificate', certificate)
}

function newDevice() {
  const secret = randomBytes(32)
  return { secret, publicKey: Buffer.from(ed25519.getPublicKey(secret)), deviceId: randomUUID(), mlsNodeId: randomUUID() }
}

function jpeg(size = 64) {
  const bytes = randomBytes(size)
  bytes[0] = 0xff
  bytes[1] = 0xd8
  bytes[2] = 0xff
  return bytes
}

function summarizeFrame(frame) {
  if (!frame) {
    return 'closed'
  }
  const summary = { body: frame.body }
  if (frame.error) {
    summary.error = frame.error
  }
  if (frame.accepted) {
    summary.accepted = frame.accepted.envelope_id
  }
  if (frame.batch) {
    summary.batch = batchSummary(frame.batch)
  }
  if (frame.compressed_batch) {
    const bytes = zstdDecompressSync(frame.compressed_batch.compressed_payload, { dictionary: SYNC_ZSTD_DICTIONARY_V1 })
    summary.compressed = {
      compression: frame.compressed_batch.compression,
      dictionary_id: frame.compressed_batch.dictionary_id,
      size_matches: bytes.length === frame.compressed_batch.uncompressed_size,
      batch: batchSummary(decode('SyncBatch', bytes)),
    }
  }
  if (frame.web_rtc_signal) {
    summary.signal = frame.web_rtc_signal
  }
  if (frame.mls_bootstrap) {
    summary.mls = frame.mls_bootstrap
  }
  return summary
}

function batchSummary(batch) {
  return {
    after: batch.after_cursor,
    next: batch.next_cursor,
    high: batch.high_watermark,
    items: batch.items.map(item => ({
      cursor: item.cursor,
      entry: item.entry,
      reason: item.tombstone?.reason,
      payload: item.envelope ? item.envelope.sealed_payload.toString('hex') : undefined,
    })),
  }
}

function envelope(recipient, payload = randomBytes(24)) {
  return create('Envelope', {
    protocol_version: 1,
    envelope_id: randomUUID(),
    recipient_device_id: recipient.deviceId,
    expires_at_ms: BigInt(Date.now() + 86_400_000),
    sealed_payload: payload,
  })
}

async function scenario(name, base, gateway) {
  const r = new Recorder(name)
  const suffix = randomUUID().replaceAll('-', '').slice(0, 24)
  const handles = { alice: `alice_${suffix}`, bob: `bob_${suffix}`, carol: `carol_${suffix}` }

  // Routing and rejections.
  r.http('unknown path', await http(base, 'GET', '/v1/nope'))
  r.http('trailing slash', await http(base, 'GET', '/v1/auth/me/'))
  r.http('wrong case', await http(base, 'GET', '/v1/AUTH/me'))
  r.http('wrong method', await http(base, 'POST', '/v1/auth/me'))
  r.http('wrong method multi', await http(base, 'DELETE', '/v1/account/username'))
  r.http('static beats param', await http(base, 'GET', '/v1/devices/delegated'))
  r.http('me without token', await http(base, 'GET', '/v1/auth/me'))
  r.http('me with bad token', await http(base, 'GET', '/v1/auth/me', { token: 'not-a-token' }))
  r.http('me with unknown token', await http(base, 'GET', '/v1/auth/me', { token: b64(randomBytes(32)) }))
  r.http('admin probe', await http(base, 'DELETE', '/v1/admin/users/00000000-0000-0000-0000-000000000000'))
  r.http('admin bad path', await http(base, 'DELETE', '/v1/admin/users/not-a-uuid', { headers: { 'x-links-admin-key': adminKey } }))
  const sampleId = randomUUID()
  for (const bad of ['x', '1234', `URN:UUID:${sampleId}`, `urn:uuid:${sampleId.toUpperCase()}`, `{${sampleId}}`, `${sampleId.slice(0, 7)}-${sampleId.slice(7)}`.slice(0, 36), `${sampleId}${sampleId}`, 'é'.repeat(16), `${sampleId.replaceAll('-', '')}0`, sampleId.replaceAll('-', ':'), `{${sampleId}`, `${sampleId.slice(0, 35)}g`]) {
    r.http(`path uuid ${bad}`, await http(base, 'GET', `/v1/groups/${encodeURIComponent(bad)}/members`, { token: 'x' }))
  }
  for (const query of ['limit=', 'limit=%2B5', 'limit=99999999999999999999', 'limit=-99999999999999999999', 'limit=1&limit=2', 'limit=-', 'search=a&search=b', 'limit=5&other=1']) {
    // Each database has its own account history, so compare only the row limit.
    const listed = await http(base, 'GET', `/v1/admin/users?${query}`, { headers: { 'x-links-admin-key': adminKey } })
    r.record(`admin query ${query}`, { status: listed.status, text: listed.json ? `rows<=${Math.min(listed.json.length, 5)}` : listed.bytes.toString() })
  }
  r.http('admin nil user', await http(base, 'DELETE', '/v1/admin/users/00000000-0000-0000-0000-000000000000', { headers: { 'x-links-admin-key': adminKey } }))
  r.http('admin wrong key', await http(base, 'GET', '/v1/admin/users', { headers: { 'x-links-admin-key': 'x'.repeat(64) } }))
  r.http('otp start absent', await http(base, 'POST', '/v1/auth/start', { json: {} }))

  const device = newDevice()
  const challengeBody = { handle: handles.alice, purpose: 'registration', device_id: device.deviceId, mls_node_id: device.mlsNodeId, public_key: b64(device.publicKey) }
  r.http('challenge no content-type', await http(base, 'POST', '/v1/auth/username/challenge', { body: JSON.stringify(challengeBody) }))
  r.http('challenge bad json', await http(base, 'POST', '/v1/auth/username/challenge', { json: '{"handle":' }))
  r.http('challenge unknown field', await http(base, 'POST', '/v1/auth/username/challenge', { json: { ...challengeBody, extra: 1 } }))
  r.http('challenge missing field', await http(base, 'POST', '/v1/auth/username/challenge', { json: { handle: handles.alice } }))
  r.http('challenge bad handle', await http(base, 'POST', '/v1/auth/username/challenge', { json: { ...challengeBody, handle: 'Bad Handle' } }))
  r.http('challenge bad purpose', await http(base, 'POST', '/v1/auth/username/challenge', { json: { ...challengeBody, purpose: 'Login' } }))
  r.http('challenge nil device', await http(base, 'POST', '/v1/auth/username/challenge', { json: { ...challengeBody, device_id: '00000000-0000-0000-0000-000000000000' } }))
  r.http('challenge weak key', await http(base, 'POST', '/v1/auth/username/challenge', { json: { ...challengeBody, public_key: b64(Buffer.alloc(32)) } }))
  r.http('challenge padded key', await http(base, 'POST', '/v1/auth/username/challenge', { json: { ...challengeBody, public_key: `${b64(device.publicKey)}=` } }))
  r.http('challenge simple uuid', await http(base, 'POST', '/v1/auth/username/challenge', { json: { ...challengeBody, device_id: device.deviceId.replaceAll('-', '').toUpperCase() } }))
  r.http('challenge oversize', await http(base, 'POST', '/v1/auth/username/challenge', { json: { ...challengeBody, public_key: 'A'.repeat(5000) } }))
  r.http('register bad signature', await (async () => {
    const challenge = await http(base, 'POST', '/v1/auth/username/challenge', { json: challengeBody })
    return http(base, 'POST', '/v1/auth/username/register', { json: { challenge_id: challenge.json.challenge_id, signature: b64(randomBytes(64)) } })
  })())
  r.http('register unknown challenge', await http(base, 'POST', '/v1/auth/username/register', { json: { challenge_id: randomUUID(), signature: b64(randomBytes(64)) } }))

  const alice = await registerAccount(base, handles.alice)
  r.record('register alice', alice.registration)
  const bob = await registerAccount(base, handles.bob)
  r.record('bob credential prefix', Buffer.from(bob.registration.mls_credential, 'base64url').subarray(0, 4).toString('hex'))
  r.record('duplicate handle', await registerAccount(base, handles.alice).catch(error => error.message.replace(UUID, '<uuid>')))
  r.record('duplicate device', await (async () => {
    const challenge = await http(base, 'POST', '/v1/auth/username/challenge', {
      json: { handle: `dan_${suffix}`, purpose: 'registration', device_id: alice.deviceId, mls_node_id: randomUUID(), public_key: b64(alice.publicKey) },
    })
    const { usernameRegistrationTranscript } = await import('../src/identity.js')
    const transcript = usernameRegistrationTranscript(challenge.json.challenge_id, `dan_${suffix}`, alice.deviceId, challenge.json.mls_node_id, alice.publicKey, Buffer.from(challenge.json.challenge, 'base64url'), BigInt(challenge.json.expires_at_ms))
    const response = await http(base, 'POST', '/v1/auth/username/register', { json: { challenge_id: challenge.json.challenge_id, signature: b64(sign(alice, transcript)) } })
    return { status: response.status, body: response.json }
  })())
  r.record('login alice', await loginAccount(base, alice))
  r.http('me', await http(base, 'GET', '/v1/auth/me', { token: alice.token }))

  // Account profile.
  r.http('username get', await http(base, 'GET', '/v1/account/username', { token: alice.token }))
  r.http('username put bad json first', await http(base, 'PUT', '/v1/account/username', { json: '{', token: 'bad' }))
  r.http('username put taken', await http(base, 'PUT', '/v1/account/username', { json: { handle: handles.bob }, token: alice.token }))
  r.http('username put same', await http(base, 'PUT', '/v1/account/username', { json: { handle: handles.alice }, token: alice.token }))
  r.http('display get', await http(base, 'GET', '/v1/account/display-name', { token: alice.token }))
  r.http('display control char', await http(base, 'PUT', '/v1/account/display-name', { json: { display_name: 'a\u0007b' }, token: alice.token }))
  r.http('display too long', await http(base, 'PUT', '/v1/account/display-name', { json: { display_name: 'é'.repeat(41) }, token: alice.token }))
  r.http('display set', await http(base, 'PUT', '/v1/account/display-name', { json: { display_name: '  Alice Ünïcode  ' }, token: alice.token }))
  r.http('display clear', await http(base, 'PUT', '/v1/account/display-name', { json: { display_name: '   ' }, token: bob.token }))
  r.http('devices', await http(base, 'GET', '/v1/account/devices', { token: alice.token }))

  // Directory.
  r.http('directory handle', await http(base, 'GET', `/v1/directory/@${handles.alice}`))
  r.http('directory double at', await http(base, 'GET', `/v1/directory/@@${handles.alice}`))
  r.http('directory missing', await http(base, 'GET', `/v1/directory/carol_${suffix}`))
  r.http('directory invalid', await http(base, 'GET', '/v1/directory/AB'))
  r.http('directory users word', await http(base, 'GET', '/v1/directory/users'))
  r.http('directory by user', await http(base, 'GET', `/v1/directory/users/${bob.userId}`, { token: alice.token }))
  r.http('directory by user bad path', await http(base, 'GET', '/v1/directory/users/picture', { token: alice.token }))
  r.http('directory by unknown user', await http(base, 'GET', `/v1/directory/users/${randomUUID()}`, { token: alice.token }))
  r.http('profiles sync', await http(base, 'POST', '/v1/directory/profiles/sync', { json: { user_ids: [alice.userId, bob.userId, randomUUID()] }, token: alice.token }))
  r.http('profiles sync empty', await http(base, 'POST', '/v1/directory/profiles/sync', { json: { user_ids: [] }, token: alice.token }))
  r.http('profiles sync get', await http(base, 'GET', '/v1/directory/profiles/sync'))

  // Profile pictures.
  r.http('picture not jpeg', await http(base, 'PUT', '/v1/profile/picture', { body: randomBytes(10), token: alice.token }))
  r.http('picture too big', await http(base, 'PUT', '/v1/profile/picture', { body: jpeg(131_073), token: alice.token }))
  r.http('picture put', await http(base, 'PUT', '/v1/profile/picture', { body: jpeg(), token: alice.token }))
  r.http('picture get', await http(base, 'GET', `/v1/directory/${handles.alice}/picture`))
  r.http('picture get at', await http(base, 'GET', `/v1/directory/@${handles.alice}/picture`))
  r.http('picture delete', await http(base, 'DELETE', '/v1/profile/picture', { token: alice.token }))
  r.http('picture gone', await http(base, 'GET', `/v1/directory/${handles.alice}/picture`))

  // Encrypted attachments.
  const attachmentId = randomUUID()
  const ciphertext = randomBytes(1000)
  const octet = { 'content-type': 'application/octet-stream' }
  r.http('blob wrong type', await http(base, 'PUT', `/v1/blobs/${attachmentId}`, { body: ciphertext, token: alice.token }))
  r.http('blob no token', await http(base, 'PUT', `/v1/blobs/${attachmentId}`, { body: ciphertext, headers: octet }))
  r.http('blob short', await http(base, 'PUT', `/v1/blobs/${attachmentId}`, { body: randomBytes(16), headers: octet, token: alice.token }))
  r.http('blob uppercase id', await http(base, 'PUT', `/v1/blobs/${attachmentId.toUpperCase()}`, { body: ciphertext, headers: octet, token: alice.token }))
  const putBlob = r.http('blob put', await http(base, 'PUT', `/v1/blobs/${attachmentId}`, { body: ciphertext, headers: octet, token: alice.token }))
  r.record('blob digest correct', putBlob.headers.get('x-links-ciphertext-sha256') === createHash('sha256').update(ciphertext).digest('hex'))
  r.http('blob put again', await http(base, 'PUT', `/v1/blobs/${attachmentId}`, { body: ciphertext, headers: octet, token: alice.token }))
  r.http('blob conflict', await http(base, 'PUT', `/v1/blobs/${attachmentId}`, { body: randomBytes(1000), headers: octet, token: alice.token }))
  const gotBlob = r.http('blob get', await http(base, 'GET', `/v1/blobs/${attachmentId}`, { token: bob.token }))
  r.record('blob roundtrip', gotBlob.bytes.equals(ciphertext))
  r.http('blob missing', await http(base, 'GET', `/v1/blobs/${randomUUID()}`, { token: bob.token }))

  // Groups.
  const groupId = randomUUID()
  r.http('group bad kind', await http(base, 'POST', '/v1/groups', { json: { group_id: groupId, kind: 'party' }, token: alice.token }))
  r.http('group create', await http(base, 'POST', '/v1/groups', { json: { group_id: groupId }, token: alice.token }))
  r.http('group create again', await http(base, 'POST', '/v1/groups', { json: { group_id: groupId, kind: null }, token: alice.token }))
  r.http('group add bob', await http(base, 'PUT', `/v1/groups/${groupId}/members/${bob.userId}/role`, { json: { role: 'admin' }, token: alice.token }))
  r.http('group bad role', await http(base, 'PUT', `/v1/groups/${groupId}/members/${bob.userId}/role`, { json: { role: 'king' }, token: alice.token }))
  r.http('group members', await http(base, 'GET', `/v1/groups/${groupId}/members`, { token: bob.token }))
  r.http('group bob removes owner', await http(base, 'DELETE', `/v1/groups/${groupId}/members/${alice.userId}`, { token: bob.token }))
  r.http('group owner leaves alone', await http(base, 'DELETE', `/v1/groups/${groupId}/members/${alice.userId}`, { token: alice.token }))
  r.http('group remove bob', await http(base, 'DELETE', `/v1/groups/${groupId}/members/${bob.userId}`, { token: alice.token }))
  r.http('group members non-member', await http(base, 'GET', `/v1/groups/${groupId}/members`, { token: bob.token }))
  r.http('group unknown', await http(base, 'GET', `/v1/groups/${randomUUID()}/members`, { token: bob.token }))
  const directId = randomUUID()
  r.http('direct create', await http(base, 'POST', '/v1/groups', { json: { group_id: directId, kind: 'direct' }, token: alice.token }))
  r.http('direct delete', await http(base, 'DELETE', `/v1/groups/${directId}`, { token: alice.token }))
  r.http('group delete', await http(base, 'DELETE', `/v1/groups/${groupId}`, { token: alice.token }))
  r.http('org controls', await http(base, 'GET', '/v1/organization/controls', { token: alice.token }))
  r.http('org controls put', await http(base, 'PUT', '/v1/organization/controls', { json: { mini_apps_enabled: true, bots_enabled: 1 }, token: alice.token }))

  // Additional and delegated devices.
  const second = newDevice()
  const nonce = randomBytes(32)
  const pairing = devicePairingTranscript(alice.userId, second.deviceId, second.mlsNodeId, second.publicKey, nonce)
  const deviceRequest = { device_id: second.deviceId, mls_node_id: second.mlsNodeId, public_key: b64(second.publicKey), nonce: b64(nonce), signature: b64(sign(second, pairing)) }
  r.http('device bad signature', await http(base, 'POST', '/v1/devices', { json: { ...deviceRequest, signature: b64(randomBytes(64)) }, token: alice.token }))
  r.http('device register', await http(base, 'POST', '/v1/devices', { json: deviceRequest, token: alice.token }))
  r.http('device register again', await http(base, 'POST', '/v1/devices', { json: deviceRequest, token: alice.token }))
  r.http('device register other account', await http(base, 'POST', '/v1/devices', { json: deviceRequest, token: bob.token }))
  const delegated = newDevice()
  const certificate = subcertificate(alice, delegated, 2, Date.now())
  const delegatedNonce = randomBytes(32)
  const delegatedRequest = {
    certificate: b64(certificate),
    nonce: b64(delegatedNonce),
    signature: b64(sign(delegated, devicePairingTranscript(alice.userId, delegated.deviceId, delegated.mlsNodeId, delegated.publicKey, delegatedNonce))),
  }
  r.http('delegated wrong issuer', await http(base, 'POST', '/v1/devices/delegated', { json: delegatedRequest, token: bob.token }))
  const delegatedResponse = r.http('delegated register', await http(base, 'POST', '/v1/devices/delegated', { json: delegatedRequest, token: alice.token }))
  r.record('delegated certificate echoed', delegatedResponse.json?.delegation_certificate === b64(certificate))
  r.http('delegated again', await http(base, 'POST', '/v1/devices/delegated', { json: delegatedRequest, token: alice.token }))
  const directory = await http(base, 'GET', `/v1/directory/${handles.alice}`)
  // Devices are ordered by random device IDs; compare them in role order.
  directory.json.devices.sort((left, right) => left.delegation_role.localeCompare(right.delegation_role))
  r.http('directory with delegation', directory)
  r.record('directory certificate roundtrip', directory.json.devices.find(device => device.delegation_certificate)?.delegation_certificate === b64(certificate))
  r.http('revoke nil', await http(base, 'DELETE', '/v1/devices/00000000-0000-0000-0000-000000000000', { token: alice.token }))
  r.http('revoke unknown', await http(base, 'DELETE', `/v1/devices/${randomUUID()}`, { token: alice.token }))
  r.http('revoke second', await http(base, 'DELETE', `/v1/devices/${second.deviceId}`, { token: alice.token }))
  r.http('devices after revoke', await http(base, 'GET', '/v1/account/devices', { token: alice.token }))

  // Passkeys are not configured in the loopback composition.
  r.http('passkey start', await http(base, 'POST', '/v1/passkeys/register/start', { token: alice.token }))
  r.http('passkey finish bad json', await http(base, 'POST', '/v1/passkeys/register/finish', { json: '{}', token: alice.token }))
  r.http('passkey assert finish', await http(base, 'POST', '/v1/passkeys/assert/finish', { json: { challenge_id: randomUUID(), credential_id: 'AA', client_data_json: 'AA', authenticator_data: 'AA', signature: 'AA' }, token: alice.token }))
  const backupId = randomUUID()
  const credentialId = randomBytes(20)
  const envelopeBytes = Buffer.alloc(128 + credentialId.length)
  envelopeBytes[0] = 1
  envelopeBytes[1] = 1
  uuidBytes(backupId).copy(envelopeBytes, 2)
  uuidBytes(alice.deviceId).copy(envelopeBytes, 18)
  envelopeBytes.writeUInt16BE(credentialId.length, 34)
  credentialId.copy(envelopeBytes, 36)
  const backup = { backup_id: backupId, device_id: alice.deviceId, credential_id: b64(credentialId), encrypted_envelope: b64(envelopeBytes) }
  r.http('backup malformed', await http(base, 'PUT', '/v1/passkey-backups', { json: { ...backup, encrypted_envelope: b64(randomBytes(200)) }, token: alice.token }))
  r.http('backup put', await http(base, 'PUT', '/v1/passkey-backups', { json: backup, token: alice.token }))
  r.http('backup put again', await http(base, 'PUT', '/v1/passkey-backups', { json: backup, token: alice.token }))
  r.http('backup get', await http(base, 'GET', `/v1/passkey-backups/${backupId}`, { token: alice.token }))
  r.http('backup get other', await http(base, 'GET', `/v1/passkey-backups/${backupId}`, { token: bob.token }))

  // Pre-keys.
  const protobuf = { 'content-type': 'application/x-protobuf' }
  const upload = prekeyUpload(bob)
  r.http('prekeys status empty', await http(base, 'GET', '/v1/prekeys/status', { token: bob.token }))
  r.http('prekeys wrong type', await http(base, 'PUT', '/v1/prekeys', { body: encode('PreKeyUpload', upload), token: bob.token }))
  r.http('prekeys garbage', await http(base, 'PUT', '/v1/prekeys', { body: Buffer.from([0xff, 0xff]), headers: protobuf, token: bob.token }))
  r.http('prekeys other device', await http(base, 'PUT', '/v1/prekeys', { body: encode('PreKeyUpload', upload), headers: protobuf, token: alice.token }))
  const tampered = structuredClone(upload)
  tampered.profile.identity.binding_signature = randomBytes(64)
  r.http('prekeys bad signature', await http(base, 'PUT', '/v1/prekeys', { body: encode('PreKeyUpload', tampered), headers: protobuf, token: bob.token }))
  const uploaded = await http(base, 'PUT', '/v1/prekeys', { body: encode('PreKeyUpload', upload), headers: { 'content-type': 'application/x-protobuf; charset=binary' }, token: bob.token })
  r.http('prekeys upload', uploaded)
  r.record('prekeys inventory', decode('PreKeyInventory', uploaded.bytes))
  const retry = await http(base, 'PUT', '/v1/prekeys', { body: encode('PreKeyUpload', upload), headers: protobuf, token: bob.token })
  r.record('prekeys retry', { status: retry.status, inventory: decode('PreKeyInventory', retry.bytes) })
  const stale = prekeyUpload(bob, { revision: 1n })
  r.http('prekeys conflicting revision', await http(base, 'PUT', '/v1/prekeys', { body: encode('PreKeyUpload', stale), headers: protobuf, token: bob.token }))
  for (let index = 0; index < 4; index += 1) {
    const claim = await http(base, 'POST', `/v1/prekeys/${bob.deviceId}/claim`, { token: alice.token })
    const bundle = claim.status === 200 ? decode('PreKeyBundle', claim.bytes) : null
    r.record(`prekeys claim ${index}`, {
      status: claim.status,
      type: claim.headers.get('content-type'),
      curve: bundle?.one_time_curve_prekey?.id,
      kem: bundle?.kem_prekey?.id,
      kemOneTime: bundle?.kem_prekey?.one_time,
      signedId: bundle?.profile?.signed_prekey?.prekey?.id,
      reencodesIdentically: bundle ? encode('PreKeyBundle', bundle).equals(claim.bytes) : null,
    })
  }
  r.http('prekeys claim unknown', await http(base, 'POST', `/v1/prekeys/${randomUUID()}/claim`, { token: alice.token }))
  r.http('prekeys status', await http(base, 'GET', '/v1/prekeys/status', { token: bob.token }))
  r.record('prekeys status body', decode('PreKeyInventory', (await http(base, 'GET', '/v1/prekeys/status', { token: bob.token })).bytes))

  // MLS key packages.
  const keyPackage = randomBytes(300)
  r.http('mls put empty', await http(base, 'PUT', '/v1/mls/key-package', { body: Buffer.alloc(0), token: alice.token }))
  r.http('mls put', await http(base, 'PUT', '/v1/mls/key-package', { body: keyPackage, token: alice.token }))
  const gotPackage = r.http('mls get', await http(base, 'GET', `/v1/mls/key-package/${alice.deviceId}`, { token: bob.token }))
  r.record('mls roundtrip', gotPackage.bytes.equals(keyPackage))
  r.http('mls get missing', await http(base, 'GET', `/v1/mls/key-package/${bob.deviceId}`, { token: bob.token }))

  // Contact discovery (deterministic for a fixed lookup key).
  const parameters = r.http('psi parameters', await http(base, 'GET', '/v1/contact-discovery/parameters', { token: alice.token }))
  r.exact.psiParameters = parameters.json
  const blind = psiBlind('+12025550123')
  r.http('psi bad version', await http(base, 'POST', '/v1/contact-discovery/query', { json: { protocol_version: 2, blinded_inputs: [b64(blind.blinded)] }, token: alice.token }))
  r.http('psi bad point', await http(base, 'POST', '/v1/contact-discovery/query', { json: { protocol_version: 1, blinded_inputs: [b64(Buffer.alloc(32, 0xff))] }, token: alice.token }))
  r.http('psi float version', await http(base, 'POST', '/v1/contact-discovery/query', { json: '{"protocol_version":1.0,"blinded_inputs":["AA"]}', token: alice.token }))
  const query = r.http('psi query', await http(base, 'POST', '/v1/contact-discovery/query', { json: { protocol_version: 1, blinded_inputs: [b64(blind.blinded)] }, token: alice.token }))
  const evaluation = query.json.evaluations[0]
  const evaluatedPoint = Buffer.from(evaluation.evaluated_point, 'base64url')
  r.record('psi proof verifies', psiVerifyProof(Buffer.from(parameters.json.server_public_key, 'base64url'), blind.blinded, evaluatedPoint, Buffer.from(evaluation.proof, 'base64url')))
  r.exact.psiUnblinded = psiUnblind(evaluatedPoint, blind.blind).toString('hex')

  // Privacy Pass.
  const ppParameters = r.http('pp parameters', await http(base, 'GET', '/v1/privacy-pass/parameters'))
  r.exact.ppParameters = ppParameters.json
  const tokenKeyId = Buffer.from(ppParameters.json.token_key_id, 'base64url')
  const challenge = r.http('pp challenge', await http(base, 'GET', '/v1/privacy-pass/challenge'))
  const challengeBytes = Buffer.from(challenge.json.challenge, 'base64url')
  const state = privacyPassBlind(challengeBytes, tokenKeyId)
  r.http('pp issue bad key id', await http(base, 'POST', '/v1/privacy-pass/issue', { json: { protocol_version: 1, token_type: 1, truncated_token_key_id: (tokenKeyId[31] + 1) % 256, blinded_message: b64(state.blinded) }, token: alice.token }))
  r.http('pp issue u8 overflow', await http(base, 'POST', '/v1/privacy-pass/issue', { json: { protocol_version: 1, token_type: 1, truncated_token_key_id: 256, blinded_message: b64(state.blinded) }, token: alice.token }))
  const issued = r.http('pp issue', await http(base, 'POST', '/v1/privacy-pass/issue', { json: { protocol_version: 1, token_type: 1, truncated_token_key_id: tokenKeyId[31], blinded_message: b64(state.blinded) }, token: alice.token }))
  const token = privacyPassFinalize(state, Buffer.from(issued.json.evaluated_message, 'base64url'), tokenKeyId)
  r.exact.ppToken = { token: b64(token), challenge: challenge.json.challenge }
  r.http('pp redeem wrong challenge', await http(base, 'POST', '/v1/privacy-pass/redeem', { json: { protocol_version: 1, token: b64(token), challenge: b64(randomBytes(40)) } }))
  r.http('pp redeem', await http(base, 'POST', '/v1/privacy-pass/redeem', { json: { protocol_version: 1, token: b64(token), challenge: challenge.json.challenge } }))
  r.http('pp redeem replay', await http(base, 'POST', '/v1/privacy-pass/redeem', { json: { protocol_version: 1, token: b64(token), challenge: challenge.json.challenge } }))

  // Chat proof-of-work.
  const pow = r.http('pow challenge', await http(base, 'GET', '/v1/chat-requests/proof-of-work/challenge', { token: alice.token }))
  const powChallenge = Buffer.from(pow.json.challenge, 'base64url')
  r.http('pow wrong nonce', await http(base, 'POST', '/v1/chat-requests/proof-of-work/verify', { json: `{"protocol_version":1,"challenge":"${pow.json.challenge}","nonce":18446744073709551615}`, token: alice.token }))
  const solved = solvePow(powChallenge, pow.json.difficulty_bits)
  r.http('pow other device', await http(base, 'POST', '/v1/chat-requests/proof-of-work/verify', { json: `{"protocol_version":1,"challenge":"${pow.json.challenge}","nonce":${solved}}`, token: bob.token }))
  r.http('pow verify', await http(base, 'POST', '/v1/chat-requests/proof-of-work/verify', { json: `{"protocol_version":1,"challenge":"${pow.json.challenge}","nonce":${solved}}`, token: alice.token }))
  r.http('pow replay', await http(base, 'POST', '/v1/chat-requests/proof-of-work/verify', { json: `{"protocol_version":1,"challenge":"${pow.json.challenge}","nonce":${solved}}`, token: alice.token }))
  r.http('pow negative nonce', await http(base, 'POST', '/v1/chat-requests/proof-of-work/verify', { json: `{"protocol_version":1,"challenge":"${pow.json.challenge}","nonce":-1}`, token: alice.token }))

  // Gateway.
  r.record('ws no subprotocol', await FrameSocket.open(gateway, []).then(() => 'open', error => error.message))
  r.record('ws wrong path', await FrameSocket.open(gateway.replace('/v1/connect', '/v1/other')).then(() => 'open', error => error.message))
  r.record('ws plain get', await http(gateway.replace('ws://', 'http://'), 'GET', '').then(response => response.status))

  const badHello = await FrameSocket.open(gateway)
  badHello.hello({ ...alice, token: b64(randomBytes(32)) })
  r.record('ws bad token', summarizeFrame(await badHello.next()))
  r.record('ws bad token then', summarizeFrame(await badHello.next()))

  const textFirst = await FrameSocket.open(gateway)
  textFirst.ws.send('hello')
  r.record('ws text first', summarizeFrame(await textFirst.next()))

  // Offline delivery: alice sends to bob before bob connects.
  const aliceSocket = await FrameSocket.open(gateway)
  aliceSocket.hello(alice)
  r.record('ws alice welcome', summarizeFrame(await aliceSocket.next()))
  const duplicate = await FrameSocket.open(gateway)
  duplicate.hello(alice)
  r.record('ws duplicate device', summarizeFrame(await duplicate.next()))

  const offline = envelope(bob, Buffer.from('offline-1'))
  aliceSocket.send({ request_id: randomUUID(), send: offline })
  r.record('ws send offline', summarizeFrame(await aliceSocket.next()))
  aliceSocket.send({ request_id: randomUUID(), send: offline })
  r.record('ws send duplicate', summarizeFrame(await aliceSocket.next()))
  aliceSocket.send({
    request_id: randomUUID(),
    web_rtc_signal: create('WebRtcSignal', { session_id: randomUUID(), target_device_id: bob.deviceId, kind: 1, sdp: 'v=0' }),
  })
  r.record('ws signal offline', summarizeFrame(await aliceSocket.next()))
  aliceSocket.send({
    request_id: randomUUID(),
    mls_bootstrap: create('MlsBootstrap', {
      conversation_id: randomUUID(),
      recipient_device_id: bob.deviceId,
      commit: Buffer.from('commit'),
      welcome: Buffer.from('welcome'),
      sender_mls_credential: Buffer.from('credential'),
      sender_identity_public_key: alice.publicKey,
    }),
  })
  r.record('ws mls offline', summarizeFrame(await aliceSocket.next()))

  const bobSocket = await FrameSocket.open(gateway)
  bobSocket.hello(bob, { compression: [1] })
  r.record('ws bob welcome', summarizeFrame(await bobSocket.next()))
  r.record('ws bob pending mls', summarizeFrame(await bobSocket.next()))
  r.record('ws bob replayed batch', summarizeFrame(await bobSocket.next()))

  const live = envelope(bob, Buffer.from('live-2'))
  aliceSocket.send({ request_id: randomUUID(), send: live })
  r.record('ws send live', summarizeFrame(await aliceSocket.next()))
  r.record('ws bob live batch', summarizeFrame(await bobSocket.next()))

  bobSocket.send({ request_id: randomUUID(), ack: create('QueueAck', { through_cursor: 1n }) })
  bobSocket.send({ request_id: randomUUID(), replay: create('Replay', { after_cursor: 0n, limit: 10 }) })
  r.record('ws replay after ack', summarizeFrame(await bobSocket.next()))

  const signalSession = randomUUID()
  aliceSocket.send({
    request_id: randomUUID(),
    web_rtc_signal: create('WebRtcSignal', { session_id: signalSession, target_device_id: bob.deviceId, kind: 3, sdp: 'candidate:1', sdp_mid: '0', sdp_mline_index: 0 }),
  })
  const signalFrame = await bobSocket.next()
  r.record('ws signal live', summarizeFrame(signalFrame))
  r.record('ws signal sender', signalFrame.web_rtc_signal?.sender_device_id === alice.deviceId)

  const conversation = randomUUID()
  aliceSocket.send({
    request_id: randomUUID(),
    mls_bootstrap: create('MlsBootstrap', {
      conversation_id: conversation,
      recipient_device_id: bob.deviceId,
      commit: Buffer.from('commit-2'),
      welcome: Buffer.from('welcome-2'),
      sender_mls_credential: Buffer.from('credential'),
      sender_identity_public_key: alice.publicKey,
      reset_group: true,
    }),
  })
  r.record('ws mls live accepted', summarizeFrame(await aliceSocket.next()))
  r.record('ws mls live delivered', summarizeFrame(await bobSocket.next()))

  const toAlice = envelope(alice, Buffer.from('reply-3'))
  bobSocket.send({ request_id: randomUUID(), send: toAlice })
  r.record('ws bob sends', summarizeFrame(await bobSocket.next()))
  r.record('ws alice receives', summarizeFrame(await aliceSocket.next()))
  aliceSocket.send({ request_id: randomUUID(), ack: create('QueueAck', { through_cursor: 1n }) })

  bobSocket.send({ request_id: randomUUID(), ack: create('QueueAck', { through_cursor: 99n }) })
  r.record('ws ack beyond watermark', summarizeFrame(await bobSocket.next()))

  aliceSocket.send({ request_id: randomUUID(), ack: create('QueueAck', { through_cursor: 0n }) })
  r.record('ws ack regress', summarizeFrame(await aliceSocket.next()))

  const expired = await FrameSocket.open(gateway)
  expired.hello(bob)
  r.record('ws reconnect after drop', summarizeFrame(await expired.next()))
  r.record('ws reconnect batch', summarizeFrame(await expired.next()))
  expired.close()

  // Sessions and admin.
  const extra = await loginAccount(base, alice).then(() => alice.token)
  r.http('logout', await http(base, 'POST', '/v1/auth/logout', { token: extra }))
  r.http('after logout', await http(base, 'GET', '/v1/auth/me', { token: extra }))
  r.http('logout again', await http(base, 'POST', '/v1/auth/logout', { token: extra }))
  const first = await loginAccount(base, bob).then(() => bob.token)
  const secondToken = await loginAccount(base, bob).then(() => bob.token)
  r.http('revoke others', await http(base, 'DELETE', '/v1/auth/sessions/others', { token: secondToken }))
  r.http('revoked other', await http(base, 'GET', '/v1/auth/me', { token: first }))
  r.http('kept current', await http(base, 'GET', '/v1/auth/me', { token: secondToken }))

  const admin = { 'x-links-admin-key': adminKey }
  const users = r.http('admin users', await http(base, 'GET', `/v1/admin/users?search=%20${handles.bob.toUpperCase()}%20&limit=0`, { headers: admin }))
  r.record('admin users found', users.json.length)
  r.http('admin users bad limit', await http(base, 'GET', '/v1/admin/users?limit=abc', { headers: admin }))
  r.http('admin disable', await http(base, 'PUT', `/v1/admin/users/${bob.userId}/status`, { json: { disabled: true }, headers: admin }))
  r.http('disabled me', await http(base, 'GET', '/v1/auth/me', { token: secondToken }))
  r.http('disabled directory', await http(base, 'GET', `/v1/directory/${handles.bob}`))
  r.http('admin enable', await http(base, 'PUT', `/v1/admin/users/${bob.userId}/status`, { json: { disabled: false }, headers: admin }))
  r.http('admin revoke device', await http(base, 'DELETE', `/v1/admin/users/${alice.userId}/devices/${delegated.deviceId}`, { headers: admin }))
  r.http('admin revoke unknown', await http(base, 'DELETE', `/v1/admin/users/${alice.userId}/devices/${randomUUID()}`, { headers: admin }))
  r.http('admin delete bob', await http(base, 'DELETE', `/v1/admin/users/${bob.userId}`, { headers: admin }))
  r.http('admin delete bob again', await http(base, 'DELETE', `/v1/admin/users/${bob.userId}`, { headers: admin }))
  r.http('deleted directory', await http(base, 'GET', `/v1/directory/${handles.bob}`))

  aliceSocket.close()
  bobSocket.close()
  return r
}

const rust = await scenario('rust', rustAuth, rustGateway)
const node = await scenario('node', nodeAuth, nodeGateway)

let failures = 0
const length = Math.max(rust.entries.length, node.entries.length)
for (let index = 0; index < length; index += 1) {
  const [label, rustValue] = rust.entries[index] ?? ['<missing>', null]
  const [, nodeValue] = node.entries[index] ?? ['<missing>', null]
  if (JSON.stringify(rustValue) !== JSON.stringify(nodeValue)) {
    failures += 1
    console.log(`DIFF ${label}\n  rust: ${JSON.stringify(rustValue)}\n  node: ${JSON.stringify(nodeValue)}`)
  }
}
for (const key of ['psiParameters', 'psiUnblinded', 'ppParameters']) {
  const same = JSON.stringify(rust.exact[key]) === JSON.stringify(node.exact[key])
  if (!same) {
    failures += 1
  }
  console.log(`${same ? 'SAME' : 'DIFF'} ${key}`)
}

// Tokens issued by one server must redeem on the other (same issuer key).
for (const [issuer, redeemer, base] of [['rust', 'node', nodeAuth], ['node', 'rust', rustAuth]]) {
  const { token, challenge } = (issuer === 'rust' ? rust : node).exact.ppToken
  const response = await http(base, 'POST', '/v1/privacy-pass/redeem', { json: { protocol_version: 1, token, challenge } })
  // Each server already redeemed its own token; the other server has not.
  const ok = response.status === 200
  if (!ok) {
    failures += 1
  }
  console.log(`${ok ? 'SAME' : 'DIFF'} privacy pass token issued by ${issuer} redeems on ${redeemer} (${response.status})`)
}

console.log(`${length} observations compared, ${failures} differences`)
process.exit(failures ? 1 : 0)
