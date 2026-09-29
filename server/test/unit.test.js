// Fixtures shared with the Rust crates' unit tests.
import assert from 'node:assert/strict'
import test from 'node:test'
import { ed25519 } from '@noble/curves/ed25519'

import { decodeBase64Url } from '../src/auth/encoding.js'
import { leadingZeroBits } from '../src/crypto/proofOfWork.js'
import { mlsCredential, verify } from '../src/identity.js'
import { create, decode, encode } from '../src/proto.js'
import { parseUuid, uuidParseError, validateEnqueue, validateHandle, validateId } from '../src/protocol.js'

test('prost-compatible receipt wire fixture', () => {
  const bytes = encode('Receipts', create('Receipts', { kind: 1, message_ids: ['id'], observed_at_ms: 150n }))
  assert.deepEqual([...bytes], [8, 1, 18, 2, 105, 100, 24, 150, 1])
})

test('default scalars are omitted and unknown fields are skipped', () => {
  const envelope = create('Envelope', {
    protocol_version: 1,
    envelope_id: '00000000-0000-4000-8000-000000000001',
    recipient_device_id: '00000000-0000-4000-8000-000000000002',
    expires_at_ms: 1000n,
    sealed_payload: Buffer.from([7, 8]),
  })
  const wire = Buffer.concat([encode('Envelope', envelope), Buffer.from([0xa0, 0x06, 0x01])])
  assert.deepEqual(decode('Envelope', wire), envelope)
  assert.equal(encode('Hello', create('Hello')).length, 0)
  assert.throws(() => decode('Envelope', Buffer.from([0xff])))
  assert.throws(() => decode('Accepted', Buffer.from([0x0a, 0x01, 0xff])), 'invalid UTF-8 is malformed')
})

test('enqueue retention bounds', () => {
  const envelope = create('Envelope', {
    protocol_version: 1,
    envelope_id: '00000000-0000-4000-8000-000000000001',
    recipient_device_id: '00000000-0000-4000-8000-000000000002',
    expires_at_ms: 1000n,
    sealed_payload: Buffer.from([7]),
  })
  validateEnqueue(envelope, 999n)
  assert.throws(() => validateEnqueue(envelope, 1000n))
})

test('strict IDs and handles', () => {
  for (const id of ['', '00000000-0000-0000-0000-000000000000', '00000000000040008000000000000001']) {
    assert.throws(() => validateId(id))
  }
  for (const handle of ['ab', 'UPPER', '1user', 'with space', 'éclair']) {
    assert.throws(() => validateHandle(handle))
  }
  validateHandle('micky_1')
})

test('UUID parsing follows the uuid crate', () => {
  const id = '67e55044-10b1-426f-9247-bb680e5fe0c8'
  assert.equal(parseUuid(id.toUpperCase()), id)
  assert.equal(parseUuid(id.replaceAll('-', '')), id)
  assert.equal(parseUuid(`{${id}}`), id)
  assert.equal(parseUuid(`urn:uuid:${id}`), id)
  assert.equal(parseUuid(`URN:UUID:${id}`), null)
  assert.equal(uuidParseError('not-a-uuid'), 'invalid character: found `n` at 0')
  assert.equal(uuidParseError('1234'), 'invalid length: found 4')
})

test('base64url decoding is canonical and unpadded', () => {
  assert.deepEqual(decodeBase64Url('AAE'), Buffer.from([0, 1]))
  assert.equal(decodeBase64Url('AAE='), null)
  assert.equal(decodeBase64Url('AAF'), null)
  assert.equal(decodeBase64Url('A'), null)
  assert.equal(decodeBase64Url('AA+/'), null)
})

test('RFC 8032 test vector 1 and strict verification', () => {
  const seed = Buffer.from('9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60', 'hex')
  const publicKey = Buffer.from(ed25519.getPublicKey(seed))
  assert.equal(publicKey.toString('hex'), 'd75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a')
  const signature = Buffer.from(
    'e5564300c360ac729086e2cc806e828a84877f1eb8e5d974d873e065224901555fb8821590a33bacc61e39701cf9b46bd25bf5f0595bbe24655141438e7a100b',
    'hex'
  )
  verify(publicKey, Buffer.alloc(0), signature)
  assert.throws(() => verify(publicKey, Buffer.from('tampered'), signature))
  assert.throws(() => verify(publicKey, Buffer.alloc(0), signature.subarray(0, 63)))
  assert.throws(() => verify(Buffer.alloc(32), Buffer.alloc(0), signature))
  const nonCanonical = Buffer.from(signature)
  nonCanonical[63] |= 0xf0
  assert.throws(() => verify(publicKey, Buffer.alloc(0), nonCanonical))
})

test('MLS basic credential layout', () => {
  const credential = mlsCredential({
    userId: '00000000-0000-0000-0000-000000000001',
    deviceId: '00000000-0000-0000-0000-000000000002',
    mlsNodeId: '00000000-0000-0000-0000-000000000003',
    publicKey: Buffer.from(ed25519.getPublicKey(Buffer.alloc(32, 1))),
  })
  assert.deepEqual([...credential.subarray(0, 4)], [0, 1, 0x40, 96])
  assert.equal(credential.length, 100)
})

test('proof-of-work leading zero bits', () => {
  assert.equal(leadingZeroBits(Buffer.alloc(32)), 256)
  assert.equal(leadingZeroBits(Buffer.alloc(32, 0x0f)), 4)
  assert.equal(leadingZeroBits(Buffer.alloc(32, 0x7f)), 1)
  assert.equal(leadingZeroBits(Buffer.alloc(32, 0x80)), 0)
})
