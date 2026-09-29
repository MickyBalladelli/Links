#!/usr/bin/env node
// Shared-database interop: the Rust and Node servers point at one database.
// Data written through one implementation must be read, deduplicated, and
// delivered identically through the other.
//
//   node test/interop.js RUST_AUTH RUST_GATEWAY NODE_AUTH NODE_GATEWAY
import { randomBytes, randomUUID } from 'node:crypto'
import { ed25519 } from '@noble/curves/ed25519'

import { pqxdhIdentityBindingTranscript, pqxdhKemPrekeyTranscript, pqxdhSignedPrekeyTranscript } from '../src/identity.js'
import { create, decode, encode } from '../src/proto.js'
import { decompressSyncBatch } from '../src/protocol.js'
import { FrameSocket, http, loginAccount, registerAccount, testHandle } from './client.js'

const [rustAuth, rustGateway, nodeAuth, nodeGateway] = process.argv.slice(2)
let failures = 0

function check(label, condition, detail = '') {
  if (!condition) {
    failures += 1
  }
  console.log(`${condition ? 'PASS' : 'FAIL'} ${label}${detail ? ` (${detail})` : ''}`)
}

function envelope(recipient, payload) {
  return create('Envelope', {
    protocol_version: 1,
    envelope_id: randomUUID(),
    recipient_device_id: recipient.deviceId,
    expires_at_ms: BigInt(Date.now() + 3_600_000),
    sealed_payload: Buffer.from(payload),
  })
}

function batchOf(frame) {
  if (frame?.compressed_batch) {
    return decompressSyncBatch(frame.compressed_batch)
  }
  return frame?.batch ?? null
}

function sign(account, transcript) {
  return Buffer.from(ed25519.sign(transcript, account.secret))
}

function prekeyUpload(account) {
  const dhKey = randomBytes(32)
  const signedKey = randomBytes(32)
  const kem = (id, oneTime) => {
    const publicKey = randomBytes(1184)
    return create('KemPreKey', { id, public_key: publicKey, one_time: oneTime, signature: sign(account, pqxdhKemPrekeyTranscript(dhKey, id, oneTime, publicKey)) })
  }
  return create('PreKeyUpload', {
    protocol_version: 1,
    device_id: account.deviceId,
    profile_revision: 7n,
    profile: create('PreKeyProfile', {
      identity: create('PqxdhPublicIdentity', { signing_key: account.publicKey, dh_key: dhKey, binding_signature: sign(account, pqxdhIdentityBindingTranscript(dhKey)) }),
      signed_prekey: create('SignedCurvePreKey', {
        prekey: create('CurvePreKey', { id: 5n, public_key: signedKey }),
        signature: sign(account, pqxdhSignedPrekeyTranscript(dhKey, 5n, signedKey)),
      }),
      last_resort_kem_prekey: kem(18_000_000_000_000_000_000n / 2n, false),
    }),
    one_time_curve_prekeys: [create('CurvePreKey', { id: 2n ** 62n + 3n, public_key: randomBytes(32) })],
    one_time_kem_prekeys: [kem(2n ** 60n + 1n, true)],
    upload_id: randomUUID(),
  })
}

// Accounts created by Rust log in through Node, and the reverse.
const alice = await registerAccount(rustAuth, testHandle('alice'))
const bob = await registerAccount(nodeAuth, testHandle('bob'))
await loginAccount(nodeAuth, alice)
check('Rust-registered account logs in through Node', Boolean(alice.token))
await loginAccount(rustAuth, bob)
check('Node-registered account logs in through Rust', Boolean(bob.token))
const rustToken = (await loginAccount(rustAuth, alice)).session.access_token
check('Rust session token authenticates on Node', (await http(nodeAuth, 'GET', '/v1/auth/me', { token: rustToken })).status === 200)
check('directory entries agree', JSON.stringify((await http(rustAuth, 'GET', `/v1/directory/${bob.handle}`)).json) === JSON.stringify((await http(nodeAuth, 'GET', `/v1/directory/${bob.handle}`)).json))

// Mailbox: Rust stores the envelope, Node replays it.
const aliceRust = await FrameSocket.open(rustGateway)
alice.token = rustToken
aliceRust.hello(alice)
await aliceRust.next()
const first = envelope(bob, 'from-rust')
aliceRust.send({ request_id: randomUUID(), send: first })
const accepted = await aliceRust.next()
check('Rust accepts envelope for offline recipient', accepted.body === 'accepted')

const bobNode = await FrameSocket.open(nodeGateway)
bobNode.hello(bob, { compression: [1] })
check('Node welcomes recipient', (await bobNode.next()).body === 'welcome')
const replay = await bobNode.next()
const replayed = batchOf(replay)?.items ?? []
check('Node replays Rust-stored envelope', replayed.length === 1 && replayed[0].envelope?.sealed_payload.toString() === 'from-rust', replay.body)

// Retrying the identical envelope through the other implementation must be
// recognized as a duplicate (same prost fingerprint), not a conflict.
const aliceNode = await FrameSocket.open(nodeGateway)
aliceRust.close()
await new Promise(resolve => setTimeout(resolve, 200))
aliceNode.hello(alice)
const aliceWelcome = await aliceNode.next()
check('Node accepts sender whose Rust socket closed', aliceWelcome.body === 'welcome', aliceWelcome.error ? `code ${aliceWelcome.error.code}` : '')
aliceNode.send({ request_id: randomUUID(), send: first })
const duplicate = await aliceNode.next()
check('identical retry via Node is idempotent', duplicate.body === 'accepted' && duplicate.accepted.envelope_id === first.envelope_id)
const redelivery = await bobNode.next()
const redelivered = batchOf(redelivery)
check('duplicate keeps the original cursor', redelivered?.items.length === 1 && redelivered.items[0].cursor === 1n)
const conflicting = create('Envelope', { ...first, sealed_payload: Buffer.from('different') })
aliceNode.send({ request_id: randomUUID(), send: conflicting })
check('different content with same ID is rejected', (await aliceNode.next()) === null)

bobNode.send({ request_id: randomUUID(), ack: create('QueueAck', { through_cursor: 1n }) })
bobNode.close()
await new Promise(resolve => setTimeout(resolve, 200))
const bobRust = await FrameSocket.open(rustGateway)
bob.token = (await loginAccount(rustAuth, bob)).session.access_token
bobRust.hello(bob)
await bobRust.next()
const afterAck = await bobRust.next()
check('Rust sees Node acknowledgement as tombstone', batchOf(afterAck)?.items[0]?.tombstone?.reason === 2)
bobRust.close()

// Pre-keys: upload through Rust, retry the identical bytes through Node.
const upload = prekeyUpload(bob)
const protobuf = { 'content-type': 'application/x-protobuf' }
const rustUpload = await http(rustAuth, 'PUT', '/v1/prekeys', { body: encode('PreKeyUpload', upload), headers: protobuf, token: bob.token })
const nodeRetry = await http(nodeAuth, 'PUT', '/v1/prekeys', { body: encode('PreKeyUpload', upload), headers: protobuf, token: bob.token })
check('pre-key upload retry across servers is idempotent', rustUpload.status === 200 && nodeRetry.status === 200 && rustUpload.bytes.equals(nodeRetry.bytes), `${rustUpload.status}/${nodeRetry.status}`)
const nodeClaim = await http(nodeAuth, 'POST', `/v1/prekeys/${bob.deviceId}/claim`, { token: alice.token })
const bundle = decode('PreKeyBundle', nodeClaim.bytes)
check('Node claims Rust-stored 64-bit pre-key IDs exactly', bundle.one_time_curve_prekey?.id === 2n ** 62n + 3n && bundle.kem_prekey.id === 2n ** 60n + 1n)
const rustClaim = await http(rustAuth, 'POST', `/v1/prekeys/${bob.deviceId}/claim`, { token: alice.token })
check('Rust sees the one-time keys Node consumed', decode('PreKeyBundle', rustClaim.bytes).kem_prekey.one_time === false)

// Proof-of-work challenge issued by Node, verified by Rust (HMAC digests agree).
const pow = await http(nodeAuth, 'GET', '/v1/chat-requests/proof-of-work/challenge', { token: alice.token })
const challenge = Buffer.from(pow.json.challenge, 'base64url')
const { verify } = await import('../src/crypto/proofOfWork.js')
let nonce = 0n
while (!verify(challenge, pow.json.difficulty_bits, nonce)) {
  nonce += 1n
}
const verified = await http(rustAuth, 'POST', '/v1/chat-requests/proof-of-work/verify', { json: `{"protocol_version":1,"challenge":"${pow.json.challenge}","nonce":${nonce}}`, token: alice.token })
check('Rust verifies a Node-issued proof-of-work challenge', verified.status === 200)

aliceNode.close()
console.log(`${failures} failures`)
process.exit(failures ? 1 : 0)
