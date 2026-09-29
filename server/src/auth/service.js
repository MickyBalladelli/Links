// Account service for the loopback username-auth composition.
// Ported from crates/account-auth/src/service.rs. SQL, rate-limit domains,
// HMAC lookup digests, and error precedence match the Rust service so the two
// implementations can share one database.
import { createHash, createHmac, randomBytes, randomUUID, timingSafeEqual } from 'node:crypto'

import * as contactPsi from '../crypto/contactPsi.js'
import * as privacyPass from '../crypto/privacyPass.js'
import * as proofOfWork from '../crypto/proofOfWork.js'
import { AuthError, StoreError, authErrorFrom } from '../errors.js'
import * as identity from '../identity.js'
import { decode } from '../proto.js'
import {
  MAX_FRAME_BYTES,
  MAX_MESSAGE_BYTES,
  isNilUuid,
  parseUuid,
  uuidBytes,
  validateDeviceSubcertificate,
  validateHandle,
  validatePrekeyUpload,
} from '../protocol.js'
import { begin } from '../store/db.js'
import { RelationalStore, parseRole } from '../store/relational.js'
import { decodeBlob, decodeFixed, encode, rustTrim } from './encoding.js'

export const CHALLENGE_TTL_MS = 10 * 60 * 1000
export const SESSION_TTL_MS = 15 * 60 * 1000
export const PASSKEY_CHALLENGE_TTL_MS = 10 * 60 * 1000
export const PRIVACY_PASS_CHALLENGE_TTL_MS = 10 * 60 * 1000
export const PROOF_OF_WORK_CHALLENGE_TTL_MS = 5 * 60 * 1000
const MAX_PROFILE_PICTURE_BYTES = 131_072
export const MAX_BLOB_BYTES = 32 * 1024 * 1024 + 16

const RATE_LIMIT_SQL =
  'INSERT INTO auth_rate_limits (key_hash,window_start_ms,attempts) VALUES ($1,$2,1) ON CONFLICT (key_hash) DO UPDATE SET attempts=CASE WHEN auth_rate_limits.window_start_ms <= $2-$3 THEN 1 ELSE auth_rate_limits.attempts+1 END, window_start_ms=CASE WHEN auth_rate_limits.window_start_ms <= $2-$3 THEN $2 ELSE auth_rate_limits.window_start_ms END WHERE auth_rate_limits.window_start_ms <= $2-$3 OR auth_rate_limits.attempts < $4 RETURNING attempts'
const WEIGHTED_RATE_LIMIT_SQL =
  'INSERT INTO auth_rate_limits (key_hash,window_start_ms,attempts) VALUES ($1,$2,$5) ON CONFLICT (key_hash) DO UPDATE SET attempts=CASE WHEN auth_rate_limits.window_start_ms <= $2-$3 THEN $5 ELSE auth_rate_limits.attempts+$5 END, window_start_ms=CASE WHEN auth_rate_limits.window_start_ms <= $2-$3 THEN $2 ELSE auth_rate_limits.window_start_ms END WHERE auth_rate_limits.window_start_ms <= $2-$3 OR auth_rate_limits.attempts+$5 <= $4 RETURNING attempts'
const PROFILE_SYNC_RATE_LIMIT_SQL =
  'INSERT INTO auth_rate_limits (key_hash,window_start_ms,attempts) VALUES ($1,$2,1) ON CONFLICT (key_hash) DO UPDATE SET attempts=CASE WHEN auth_rate_limits.window_start_ms <= $2-$3 THEN 1 ELSE auth_rate_limits.attempts+1 END, window_start_ms=CASE WHEN auth_rate_limits.window_start_ms <= $2-$3 THEN $2 ELSE auth_rate_limits.window_start_ms END WHERE auth_rate_limits.window_start_ms <= $2-$3 OR auth_rate_limits.attempts < $4 RETURNING attempts'

const USERNAME_RATE_LIMITS = {
  'registration:start': ['links/username-register-start-ip-minute/v2\0', 'links/username-register-start-ip-hour/v2\0', 'links/username-register-start-global-hour/v2\0', 20, 200],
  'login:start': ['links/username-login-start-ip-minute/v2\0', 'links/username-login-start-ip-hour/v2\0', 'links/username-login-start-global-hour/v2\0', 20, 200],
  'registration:finish': ['links/username-register-finish-ip-minute/v2\0', 'links/username-register-finish-ip-hour/v2\0', 'links/username-register-finish-global-hour/v2\0', 40, 400],
  'login:finish': ['links/username-login-finish-ip-minute/v2\0', 'links/username-login-finish-ip-hour/v2\0', 'links/username-login-finish-global-hour/v2\0', 40, 400],
}

const denied = () => new AuthError('Denied')
const invalid = () => new AuthError('Invalid')
const unavailable = () => new AuthError('Unavailable')

function isProfileJpeg(jpeg) {
  return (
    jpeg.length >= 3 &&
    jpeg.length <= MAX_PROFILE_PICTURE_BYTES &&
    jpeg[0] === 0xff &&
    jpeg[1] === 0xd8 &&
    jpeg[2] === 0xff
  )
}

function sha256(bytes) {
  return createHash('sha256').update(bytes).digest()
}

function mapUsernameRegistrationWriteError(error) {
  if (error?.code === '23505') {
    switch (error.constraint) {
      case 'handles_pkey':
        return new AuthError('UsernameConflict')
      case 'devices_pkey':
      case 'devices_mls_node_id_key':
        return new AuthError('DeviceConflict')
      default:
        return new AuthError('Conflict')
    }
  }
  return unavailable()
}

function mapUsernameChangeWriteError(error) {
  if (error?.code === '23505') {
    return error.constraint === 'handles_pkey' ? new AuthError('UsernameConflict') : new AuthError('Conflict')
  }
  return unavailable()
}

async function query(tx, text, values, mapError) {
  try {
    return await tx.query(text, values)
  } catch (error) {
    throw mapError(error)
  }
}

function validateEncryptedBackupEnvelope(backupId, deviceId, credentialId, envelope) {
  // Keep this structural check on the server so a plaintext seed or an
  // unrelated blob cannot be registered as a backup. AEAD verification stays
  // on the device because the passkey PRF output never reaches this service.
  if (
    isNilUuid(backupId) ||
    isNilUuid(deviceId) ||
    credentialId.length < 1 ||
    credentialId.length > 1024 ||
    envelope.length < 128 ||
    envelope.length > 8192 ||
    envelope[0] !== 1 ||
    envelope[1] !== 1 ||
    !envelope.subarray(2, 18).equals(uuidBytes(backupId)) ||
    !envelope.subarray(18, 34).equals(uuidBytes(deviceId))
  ) {
    throw invalid()
  }
  const credentialLength = envelope.readUInt16BE(34)
  if (
    credentialLength !== credentialId.length ||
    envelope.length !== 128 + credentialLength ||
    !envelope.subarray(36, 36 + credentialLength).equals(credentialId)
  ) {
    throw invalid()
  }
}

function verifyKemPrekey(signingKey, dhKey, key) {
  identity.verify(signingKey, identity.pqxdhKemPrekeyTranscript(dhKey, key.id, key.one_time, key.public_key), key.signature)
}

function verifyPrekeyUpload(upload) {
  try {
    validatePrekeyUpload(upload)
  } catch {
    throw invalid()
  }
  const profile = upload.profile
  const id = profile?.identity
  if (!id || id.signing_key.length !== 32 || id.dh_key.length !== 32) {
    throw invalid()
  }
  identity.verify(id.signing_key, identity.pqxdhIdentityBindingTranscript(id.dh_key), id.binding_signature)
  const signed = profile.signed_prekey
  const signedKey = signed?.prekey
  if (!signedKey || signedKey.public_key.length !== 32) {
    throw invalid()
  }
  identity.verify(
    id.signing_key,
    identity.pqxdhSignedPrekeyTranscript(id.dh_key, signedKey.id, signedKey.public_key),
    signed.signature
  )
  if (!profile.last_resort_kem_prekey) {
    throw invalid()
  }
  verifyKemPrekey(id.signing_key, id.dh_key, profile.last_resort_kem_prekey)
  for (const key of upload.one_time_kem_prekeys) {
    verifyKemPrekey(id.signing_key, id.dh_key, key)
  }
}

function derivePrivacyPassKey(seed) {
  return createHash('sha256').update('links/privacy-pass/issuer-seed/v1\0').update(seed).digest()
}

export class AccountAuth {
  /**
   * @param {import('pg').Pool} pool
   * @param {Buffer} phoneLookupKey 32-byte AUTH_LOOKUP_KEY
   * @param {{ mode: 'production' | 'loopback-username-dev', passkey?: null, clock?: () => number, adminKey?: string }} options
   */
  constructor(pool, phoneLookupKey, options) {
    if (phoneLookupKey.length !== 32 || phoneLookupKey.every(byte => byte === 0)) {
      throw invalid()
    }
    const adminKey = options.adminKey
    if (adminKey !== undefined && Buffer.byteLength(adminKey, 'utf8') < 32) {
      throw invalid()
    }
    this.pool = pool
    this.store = new RelationalStore(pool)
    this.phoneLookupKey = Buffer.from(phoneLookupKey)
    this.privacyPassKey = derivePrivacyPassKey(this.phoneLookupKey)
    this.clock = options.clock ?? (() => Date.now())
    this.passkey = options.passkey ?? null
    this.mode = options.mode
    this.adminKey = adminKey === undefined ? null : Buffer.from(adminKey, 'utf8')
  }

  isLoopbackUsernameDev() {
    return this.mode === 'loopback-username-dev'
  }

  authorizeAdminKey(provided) {
    if (!this.adminKey) {
      throw unavailable()
    }
    const candidate = Buffer.from(provided, 'latin1')
    if (candidate.length !== this.adminKey.length || !timingSafeEqual(candidate, this.adminKey)) {
      throw denied()
    }
  }

  now() {
    const now = this.clock()
    if (!Number.isSafeInteger(now) || now <= 0) {
      throw unavailable()
    }
    return now
  }

  digest(domain, value) {
    return createHmac('sha256', this.phoneLookupKey).update(domain).update(value).digest()
  }

  async runRateLimits(limits, now) {
    const tx = await begin(this.pool)
    let limited = false
    try {
      for (const [key, window, limit, cost] of limits) {
        const nowMs = BigInt(now ?? this.now())
        const result =
          cost === undefined
            ? await tx.query(RATE_LIMIT_SQL, [key, nowMs, BigInt(window), limit])
            : await tx.query(WEIGHTED_RATE_LIMIT_SQL, [key, nowMs, BigInt(window), limit, cost])
        if (!result.rows.length) {
          limited = true
          break
        }
      }
      await tx.commit()
    } finally {
      await tx.release()
    }
    if (limited) {
      throw new AuthError('RateLimited')
    }
  }

  async enforceUsernameSourceRateLimits(purpose, finish, peerIp, now) {
    // Development username auth is loopback-only. Production uses separate
    // registration/login source and global limits; account handles are
    // deliberately excluded so unauthenticated traffic cannot lock out a name.
    if (this.isLoopbackUsernameDev()) {
      return
    }
    const [minuteDomain, hourDomain, globalDomain, minuteLimit, hourLimit] =
      USERNAME_RATE_LIMITS[`${purpose}:${finish ? 'finish' : 'start'}`]
    await this.runRateLimits(
      [
        [this.digest(minuteDomain, peerIp), 60_000, minuteLimit],
        [this.digest(hourDomain, peerIp), 3_600_000, hourLimit],
        [this.digest(globalDomain, 'global'), 3_600_000, 10_000],
      ],
      now
    )
  }

  async enforceDirectoryRateLimits(handle, peerIp, now) {
    await this.runRateLimits(
      [
        [this.digest('links/directory-handle-minute/v1\0', handle), 60_000, 60],
        [this.digest('links/directory-handle-hour/v1\0', handle), 3_600_000, 1_000],
        [this.digest('links/directory-ip/v1\0', peerIp), 3_600_000, 300],
      ],
      now
    )
  }

  async enforceProfileSyncRateLimit(userId) {
    const key = this.digest('links/directory-profile-sync-user-minute/v1\0', uuidBytes(userId))
    const now = this.now()
    const result = await this.pool.query(PROFILE_SYNC_RATE_LIMIT_SQL, [key, BigInt(now), 60_000n, 120])
    if (!result.rows.length) {
      throw new AuthError('RateLimited')
    }
  }

  async enforceContactPsiRateLimits(userId, peerIp, now, batchSize) {
    const user = uuidBytes(userId)
    await this.runRateLimits(
      [
        [this.digest('links/contact-psi-user-minute/v1\0', user), 60_000, 10, 1],
        [this.digest('links/contact-psi-user-hour/v1\0', user), 3_600_000, 60, 1],
        [this.digest('links/contact-psi-user-items-hour/v1\0', user), 3_600_000, 10_000, batchSize],
        [this.digest('links/contact-psi-ip-hour/v1\0', peerIp), 3_600_000, 100, 1],
        [this.digest('links/contact-psi-ip-items-hour/v1\0', peerIp), 3_600_000, 20_000, batchSize],
      ],
      now
    )
  }

  async enforceRateLimitSet(limits) {
    await this.runRateLimits(limits)
  }

  async requireUnverifiedAccount(account) {
    const { rows } = await this.pool.query(
      "SELECT account_kind='pseudonymous' AS unverified FROM accounts WHERE user_id=$1 AND disabled_at IS NULL",
      [account.user_id]
    )
    if (rows[0]?.unverified !== true) {
      throw denied()
    }
  }

  async issueSession(tx, userId, deviceId) {
    const tokenBytes = randomBytes(32)
    const tokenHash = sha256(tokenBytes)
    const expiresAtMs = this.now() + SESSION_TTL_MS
    await tx.query('INSERT INTO auth_sessions (token_hash,user_id,device_id,expires_at_ms) VALUES ($1,$2,$3,$4)', [
      tokenHash,
      userId,
      deviceId,
      BigInt(expiresAtMs),
    ])
    const accessToken = encode(tokenBytes)
    tokenBytes.fill(0)
    return { access_token: accessToken, expires_at_ms: expiresAtMs, user_id: userId, device_id: deviceId }
  }

  // --- Admin -----------------------------------------------------------------

  async adminUsers(search, limit) {
    const normalized = rustTrim(search).toLowerCase()
    const clamped = limit < 1n ? 1n : limit > 200n ? 200n : limit
    const { rows } = await this.pool.query(
      `SELECT a.user_id,a.account_kind,a.created_at::text AS created_at,a.disabled_at::text AS disabled_at,h.handle
             FROM accounts a
             LEFT JOIN handles h USING (user_id)
             WHERE $1 = '' OR h.handle ILIKE '%' || $1 || '%' OR a.user_id::text = $1
             ORDER BY a.created_at DESC
             LIMIT $2`,
      [normalized, clamped]
    )
    const users = []
    for (const row of rows) {
      const devices = await this.pool.query(
        `SELECT device_id,registered_at::text AS registered_at,revoked_at::text AS revoked_at
                 FROM devices WHERE user_id=$1 ORDER BY registered_at`,
        [row.user_id]
      )
      users.push({
        user_id: row.user_id,
        handle: row.handle,
        account_kind: row.account_kind,
        created_at: row.created_at,
        disabled_at: row.disabled_at,
        devices: devices.rows.map(device => ({
          device_id: device.device_id,
          registered_at: device.registered_at,
          revoked_at: device.revoked_at,
        })),
      })
    }
    return users
  }

  async adminSetUserDisabled(userId, disabled) {
    if (isNilUuid(userId)) {
      throw invalid()
    }
    const tx = await begin(this.pool)
    try {
      const updated = await tx.query(
        `UPDATE accounts
             SET disabled_at = CASE WHEN $2 THEN COALESCE(disabled_at, now()) ELSE NULL END
             WHERE user_id=$1`,
        [userId, disabled]
      )
      if (updated.rowCount !== 1) {
        throw denied()
      }
      if (disabled) {
        await tx.query('DELETE FROM auth_sessions WHERE user_id=$1', [userId])
      }
      await tx.commit()
    } finally {
      await tx.release()
    }
  }

  async adminDeleteUser(userId) {
    if (isNilUuid(userId)) {
      throw invalid()
    }
    const tx = await begin(this.pool)
    try {
      const deviceIds = (await tx.query('SELECT device_id FROM devices WHERE user_id=$1', [userId])).rows.map(
        row => row.device_id
      )
      const handle = (await tx.query('SELECT handle FROM handles WHERE user_id=$1', [userId])).rows[0]?.handle ?? null
      await tx.query(
        `DELETE FROM encrypted_payloads
             WHERE recipient_device_id = ANY($1)`,
        [deviceIds]
      )
      await tx.query(
        `DELETE FROM encrypted_payload_cursors
             WHERE recipient_device_id = ANY($1)`,
        [deviceIds]
      )
      await tx.query(
        `DELETE FROM auth_challenges
             WHERE device_id = ANY($1)`,
        [deviceIds]
      )
      if (handle !== null) {
        await tx.query('DELETE FROM username_auth_challenges WHERE handle=$1', [handle])
      }
      await tx.query(
        `DELETE FROM username_auth_challenges
             WHERE device_id = ANY($1)`,
        [deviceIds]
      )
      await tx.query(
        `UPDATE devices
             SET delegation_role='owner',
                 delegated_by_device_id=NULL,
                 delegation_certificate=NULL
             WHERE user_id=$1`,
        [userId]
      )
      await tx.query('DELETE FROM devices WHERE user_id=$1', [userId])
      await tx.query(
        `DELETE FROM groups g
             WHERE EXISTS (
                 SELECT 1 FROM group_memberships owner_membership
                 WHERE owner_membership.group_id=g.group_id
                   AND owner_membership.user_id=$1
                   AND owner_membership.role='owner'
             )
             AND NOT EXISTS (
                 SELECT 1 FROM group_memberships other_membership
                 WHERE other_membership.group_id=g.group_id
                   AND other_membership.user_id<>$1
             )`,
        [userId]
      )
      await tx.query(
        `WITH ownerless_groups AS (
                 SELECT DISTINCT owner_membership.group_id
                 FROM group_memberships owner_membership
                 WHERE owner_membership.user_id=$1
                   AND owner_membership.role='owner'
                   AND NOT EXISTS (
                       SELECT 1 FROM group_memberships other_owner
                       WHERE other_owner.group_id=owner_membership.group_id
                         AND other_owner.role='owner'
                         AND other_owner.user_id<>$1
                   )
             ), replacements AS (
                 SELECT DISTINCT ON (membership.group_id)
                     membership.group_id, membership.user_id
                 FROM group_memberships membership
                 JOIN ownerless_groups
                   ON ownerless_groups.group_id=membership.group_id
                 WHERE membership.user_id<>$1
                 ORDER BY membership.group_id, membership.joined_at, membership.user_id
             )
             UPDATE group_memberships membership
             SET role='owner'
             FROM replacements
             WHERE membership.group_id=replacements.group_id
               AND membership.user_id=replacements.user_id`,
        [userId]
      )
      const deleted = await tx.query('DELETE FROM accounts WHERE user_id=$1', [userId])
      if (deleted.rowCount !== 1) {
        throw denied()
      }
      await tx.commit()
    } finally {
      await tx.release()
    }
  }

  async adminRevokeDevice(userId, deviceId) {
    if (isNilUuid(userId) || isNilUuid(deviceId)) {
      throw invalid()
    }
    const result = await this.pool.query(
      `WITH RECURSIVE descendants AS (
                 SELECT device_id FROM devices WHERE device_id=$1 AND user_id=$2
                 UNION ALL
                 SELECT child.device_id
                 FROM devices child
                 JOIN descendants parent ON child.delegated_by_device_id=parent.device_id
                 WHERE child.user_id=$2
             )
             UPDATE devices SET revoked_at=COALESCE(revoked_at,now())
             WHERE device_id IN (SELECT device_id FROM descendants)
               AND user_id=$2`,
      [deviceId, userId]
    )
    if (result.rowCount === 0) {
      throw denied()
    }
    await this.pool.query('DELETE FROM auth_sessions WHERE user_id=$1 AND device_id=$2', [userId, deviceId])
  }

  // --- Username registration and login --------------------------------------

  /**
   * Issue a one-time server challenge for username registration or login.
   * The challenge is bound to the complete requested device identity.
   */
  async startUsernameChallenge(request, peerIp) {
    try {
      validateHandle(request.handle)
    } catch {
      throw invalid()
    }
    if (isNilUuid(request.device_id) || isNilUuid(request.mls_node_id)) {
      throw invalid()
    }
    const now = this.now()
    await this.enforceUsernameSourceRateLimits(request.purpose, false, peerIp, now)
    const publicKey = decodeFixed(request.public_key, 32)
    try {
      identity.validatePublicKey(publicKey)
    } catch {
      throw invalid()
    }
    const challengeId = randomUUID()
    const challenge = randomBytes(32)
    const expiresAtMs = now + CHALLENGE_TTL_MS
    const tx = await begin(this.pool)
    try {
      // Multiple pending challenges may coexist. Invalidating an earlier
      // challenge here would let anyone who knows public directory fields lock
      // out the legitimate device by repeatedly starting a newer challenge.
      await tx.query(
        "INSERT INTO username_auth_challenges (challenge_id,purpose,handle,device_id,mls_node_id,public_key,challenge,state,expires_at_ms) VALUES ($1,$2,$3,$4,$5,$6,$7,'pending',$8)",
        [
          challengeId,
          request.purpose,
          request.handle,
          request.device_id,
          request.mls_node_id,
          publicKey,
          challenge,
          BigInt(expiresAtMs),
        ]
      )
      await tx.commit()
    } finally {
      await tx.release()
    }
    return {
      challenge_id: challengeId,
      handle: request.handle,
      purpose: request.purpose,
      device_id: request.device_id,
      mls_node_id: request.mls_node_id,
      public_key: encode(publicKey),
      challenge: encode(challenge),
      expires_at_ms: expiresAtMs,
    }
  }

  async failUsernameChallenge(tx, challengeId) {
    await tx.query(
      "UPDATE username_auth_challenges SET state='failed',attempts=attempts+1 WHERE challenge_id=$1 AND state='pending'",
      [challengeId]
    )
    await tx.commit()
    return denied()
  }

  /** Create a pseudonymous account from a one-time challenge and device-key proof. */
  async registerUsername(request, peerIp) {
    await this.enforceUsernameSourceRateLimits('registration', true, peerIp, this.now())
    const signature = decodeFixed(request.signature, 64)
    const tx = await begin(this.pool)
    try {
      const { rows } = await tx.query(
        "SELECT handle,device_id,mls_node_id,public_key,challenge,state,expires_at_ms FROM username_auth_challenges WHERE challenge_id=$1 AND purpose='registration' FOR UPDATE",
        [request.challenge_id]
      )
      const row = rows[0]
      if (!row) {
        throw denied()
      }
      const expiresAtMs = row.expires_at_ms
      if (row.state !== 'pending' || expiresAtMs <= BigInt(this.now())) {
        throw denied()
      }
      const { handle, device_id: deviceId, mls_node_id: mlsNodeId } = row
      if (row.public_key.length !== 32 || row.challenge.length !== 32) {
        throw unavailable()
      }
      const publicKey = row.public_key
      const transcript = identity.usernameRegistrationTranscript(
        request.challenge_id,
        handle,
        deviceId,
        mlsNodeId,
        publicKey,
        row.challenge,
        expiresAtMs
      )
      try {
        identity.verify(publicKey, transcript, signature)
      } catch {
        throw await this.failUsernameChallenge(tx, request.challenge_id)
      }
      const userId = randomUUID()
      const credential = identity.mlsCredential({ userId, deviceId, mlsNodeId, publicKey })
      await query(
        tx,
        "INSERT INTO accounts (user_id,auth_subject_hash,account_kind) VALUES ($1,$2,'pseudonymous')",
        [userId, null],
        mapUsernameRegistrationWriteError
      )
      await query(tx, 'INSERT INTO handles (handle,user_id) VALUES ($1,$2)', [handle, userId], mapUsernameRegistrationWriteError)
      await query(
        tx,
        'INSERT INTO devices (device_id,user_id,mls_node_id,identity_public_key,mls_credential) VALUES ($1,$2,$3,$4,$5)',
        [deviceId, userId, mlsNodeId, publicKey, credential],
        mapUsernameRegistrationWriteError
      )
      const session = await this.issueSession(tx, userId, deviceId)
      await tx.query(
        "UPDATE username_auth_challenges SET state='consumed' WHERE challenge_id=$1 AND state='pending'",
        [request.challenge_id]
      )
      await tx.commit()
      return { session, handle, mls_credential: encode(credential) }
    } finally {
      await tx.release()
    }
  }

  /** Log in to a username account with a one-time challenge and the registered device key. */
  async loginUsername(request, peerIp) {
    await this.enforceUsernameSourceRateLimits('login', true, peerIp, this.now())
    const signature = decodeFixed(request.signature, 64)
    const tx = await begin(this.pool)
    try {
      const challengeRow = (
        await tx.query(
          "SELECT handle,device_id,mls_node_id,public_key,challenge,state,expires_at_ms FROM username_auth_challenges WHERE challenge_id=$1 AND purpose='login' FOR UPDATE",
          [request.challenge_id]
        )
      ).rows[0]
      if (!challengeRow) {
        throw denied()
      }
      const expiresAtMs = challengeRow.expires_at_ms
      if (challengeRow.state !== 'pending' || expiresAtMs <= BigInt(this.now())) {
        throw denied()
      }
      const { handle, device_id: deviceId, mls_node_id: mlsNodeId } = challengeRow
      if (challengeRow.public_key.length !== 32 || challengeRow.challenge.length !== 32) {
        throw unavailable()
      }
      const publicKey = challengeRow.public_key
      const transcript = identity.usernameLoginTranscript(
        request.challenge_id,
        handle,
        deviceId,
        mlsNodeId,
        publicKey,
        challengeRow.challenge,
        expiresAtMs
      )
      try {
        identity.verify(publicKey, transcript, signature)
      } catch {
        throw await this.failUsernameChallenge(tx, request.challenge_id)
      }
      const row = (
        await tx.query(
          "SELECT a.user_id,d.mls_node_id,d.identity_public_key,d.mls_credential FROM handles h JOIN accounts a USING (user_id) JOIN devices d USING (user_id) WHERE h.handle=$1 AND d.device_id=$2 AND a.account_kind IN ('pseudonymous','consumer') AND a.disabled_at IS NULL AND d.revoked_at IS NULL FOR SHARE OF a,d",
          [handle, deviceId]
        )
      ).rows[0]
      if (!row) {
        throw await this.failUsernameChallenge(tx, request.challenge_id)
      }
      const userId = row.user_id
      if (row.identity_public_key.length !== 32) {
        throw unavailable()
      }
      if (row.mls_node_id !== mlsNodeId || !row.identity_public_key.equals(publicKey)) {
        throw await this.failUsernameChallenge(tx, request.challenge_id)
      }
      const credential = identity.mlsCredential({ userId, deviceId, mlsNodeId, publicKey })
      if (!row.mls_credential.equals(credential)) {
        throw unavailable()
      }
      const authenticatedKey = this.digest(
        'links/username-authenticated-device-hour/v2\0',
        Buffer.concat([uuidBytes(userId), uuidBytes(deviceId)])
      )
      const allowed = await tx.query(RATE_LIMIT_SQL, [authenticatedKey, BigInt(this.now()), 3_600_000n, 120])
      if (!allowed.rows.length) {
        throw new AuthError('RateLimited')
      }
      const session = await this.issueSession(tx, userId, deviceId)
      await tx.query(
        "UPDATE username_auth_challenges SET state='consumed' WHERE challenge_id=$1 AND state='pending'",
        [request.challenge_id]
      )
      await tx.commit()
      return { session, handle, mls_credential: encode(credential) }
    } finally {
      await tx.release()
    }
  }

  // --- Account profile -------------------------------------------------------

  async currentUsername(token) {
    const account = await this.authenticate(token)
    const { rows } = await this.pool.query('SELECT handle FROM handles WHERE user_id=$1', [account.user_id])
    return { handle: rows[0]?.handle ?? null }
  }

  /** Change the public username while keeping account and device identities intact. */
  async changeUsername(token, request) {
    const account = await this.authenticate(token)
    try {
      validateHandle(request.handle)
    } catch {
      throw invalid()
    }
    const tx = await begin(this.pool)
    try {
      const kind = (
        await tx.query(
          `SELECT account_kind FROM accounts
             WHERE user_id=$1 AND disabled_at IS NULL FOR UPDATE`,
          [account.user_id]
        )
      ).rows[0]?.account_kind
      if (kind !== 'consumer' && kind !== 'pseudonymous') {
        throw denied()
      }
      const currentHandle =
        (await tx.query('SELECT handle FROM handles WHERE user_id=$1 FOR UPDATE', [account.user_id])).rows[0]?.handle ??
        null
      if (currentHandle === request.handle) {
        await tx.commit()
        return { handle: request.handle }
      }
      if (currentHandle !== null) {
        await query(
          tx,
          'UPDATE handles SET handle=$1, claimed_at=now() WHERE user_id=$2',
          [request.handle, account.user_id],
          mapUsernameChangeWriteError
        )
      } else {
        await query(
          tx,
          'INSERT INTO handles (handle,user_id) VALUES ($1,$2)',
          [request.handle, account.user_id],
          mapUsernameChangeWriteError
        )
      }
      await tx.commit()
      return { handle: request.handle }
    } finally {
      await tx.release()
    }
  }

  async currentDisplayName(token) {
    const account = await this.authenticate(token)
    const { rows } = await this.pool.query(
      `SELECT display_name, display_name_set FROM accounts
             WHERE user_id=$1 AND disabled_at IS NULL`,
      [account.user_id]
    )
    if (!rows.length) {
      throw denied()
    }
    return { display_name: rows[0].display_name, display_name_set: rows[0].display_name_set }
  }

  /** Every device registered to the account, including revoked devices. */
  async accountDevices(token) {
    const account = await this.authenticate(token)
    const { rows } = await this.pool.query(
      `SELECT device_id,registered_at::text AS registered_at,
                    revoked_at::text AS revoked_at,delegation_role
             FROM devices
             WHERE user_id=$1
             ORDER BY registered_at DESC, device_id`,
      [account.user_id]
    )
    return {
      devices: rows.map(row => ({
        device_id: row.device_id,
        registered_at: row.registered_at,
        revoked_at: row.revoked_at,
        delegation_role: row.delegation_role,
        current: row.device_id === account.device_id,
      })),
      current_device_id: account.device_id,
    }
  }

  /** Set the account-wide display name. An empty string clears it. */
  async changeDisplayName(token, request) {
    const account = await this.authenticate(token)
    const cleanName = rustTrim(request.display_name)
    if (Buffer.byteLength(cleanName, 'utf8') > 80 || /\p{Cc}/u.test(cleanName)) {
      throw invalid()
    }
    const displayName = cleanName === '' ? null : cleanName
    const updated = await this.pool.query(
      `UPDATE accounts SET display_name=$1, display_name_set=TRUE
             WHERE user_id=$2 AND disabled_at IS NULL
               AND account_kind IN ('consumer', 'pseudonymous')`,
      [displayName, account.user_id]
    )
    if (updated.rowCount !== 1) {
      throw denied()
    }
    return { display_name: displayName, display_name_set: true }
  }

  /** Replace the public profile picture. Callers must send a JPEG no larger than 128 KiB. */
  async putProfilePicture(token, jpeg) {
    if (!isProfileJpeg(jpeg)) {
      throw invalid()
    }
    const account = await this.authenticate(token)
    await this.pool.query(
      `INSERT INTO account_profile_pictures (user_id, jpeg) VALUES ($1, $2)
             ON CONFLICT (user_id) DO UPDATE SET jpeg = EXCLUDED.jpeg, updated_at = now()`,
      [account.user_id, jpeg]
    )
  }

  async deleteProfilePicture(token) {
    const account = await this.authenticate(token)
    await this.pool.query('DELETE FROM account_profile_pictures WHERE user_id = $1', [account.user_id])
  }

  async lookupProfilePicture(handle, peerIp) {
    try {
      validateHandle(handle)
    } catch {
      throw invalid()
    }
    if (!this.isLoopbackUsernameDev()) {
      await this.enforceDirectoryRateLimits(handle, peerIp, this.now())
    }
    const { rows } = await this.pool.query(
      `SELECT p.jpeg FROM account_profile_pictures p
             JOIN handles h ON h.user_id = p.user_id
             JOIN accounts a ON a.user_id = p.user_id
             WHERE h.handle = $1 AND a.disabled_at IS NULL AND a.account_kind = 'pseudonymous'`,
      [handle]
    )
    const jpeg = rows[0]?.jpeg
    return jpeg && isProfileJpeg(jpeg) ? jpeg : null
  }

  // --- Directory -------------------------------------------------------------

  /**
   * Active public device directory for a canonical username. Pre-key bundles
   * stay behind the authenticated claim endpoint.
   */
  async lookupUsernameDirectory(handle, peerIp) {
    try {
      validateHandle(handle)
    } catch {
      throw invalid()
    }
    const now = this.now()
    await this.enforceDirectoryRateLimits(handle, peerIp, now)
    const directory = await this.store.lookupHandleDirectory(handle)
    if (!directory) {
      return null
    }
    const displayRows = await this.pool.query(
      'SELECT display_name FROM accounts WHERE user_id=$1 AND disabled_at IS NULL',
      [directory.user_id]
    )
    const displayName = displayRows.rows[0]?.display_name ?? null
    const devices = []
    for (const device of directory.devices) {
      if (device.identity_public_key.length !== 32) {
        throw unavailable()
      }
      const credential = identity.mlsCredential({
        userId: directory.user_id,
        deviceId: device.device_id,
        mlsNodeId: device.mls_node_id,
        publicKey: device.identity_public_key,
      })
      if (!credential.equals(device.mls_credential)) {
        throw unavailable()
      }
      devices.push({
        device_id: device.device_id,
        mls_node_id: device.mls_node_id,
        did: device.did,
        identity_public_key: encode(device.identity_public_key),
        mls_credential: encode(device.mls_credential),
        delegation_role: device.delegation_role,
        delegation_certificate: device.delegation_certificate ? encode(device.delegation_certificate) : null,
      })
    }
    return {
      handle,
      user_id: directory.user_id,
      display_name: displayName,
      devices,
      verification_badge: directory.verification_badge ? encode(directory.verification_badge) : null,
    }
  }

  /** Resolve a sender user ID to its active public username directory. */
  async lookupUsernameDirectoryByUserId(token, userId, peerIp) {
    await this.authenticate(token)
    if (isNilUuid(userId)) {
      throw invalid()
    }
    const { rows } = await this.pool.query(
      'SELECT h.handle FROM handles h JOIN accounts a USING (user_id) WHERE h.user_id=$1 AND a.disabled_at IS NULL',
      [userId]
    )
    if (!rows.length) {
      return null
    }
    return this.lookupUsernameDirectory(rows[0].handle, peerIp)
  }

  /** Current public names for a bounded set of saved contact IDs. */
  async syncDirectoryProfiles(token, userIds) {
    const account = await this.authenticate(token)
    if (userIds.length === 0 || userIds.length > 256 || userIds.some(isNilUuid)) {
      throw invalid()
    }
    await this.enforceProfileSyncRateLimit(account.user_id)
    const { rows } = await this.pool.query(
      'SELECT a.user_id, h.handle, a.display_name, COUNT(d.device_id)::INT AS device_count FROM accounts a JOIN handles h USING (user_id) JOIN devices d ON d.user_id = a.user_id AND d.revoked_at IS NULL WHERE a.user_id = ANY($1) AND a.disabled_at IS NULL GROUP BY a.user_id, h.handle, a.display_name ORDER BY h.handle',
      [userIds]
    )
    return {
      profiles: rows.map(row => ({
        user_id: row.user_id,
        handle: row.handle,
        display_name: row.display_name,
        device_count: row.device_count,
      })),
    }
  }

  // --- Sessions --------------------------------------------------------------

  async authenticate(token) {
    let bytes
    try {
      bytes = decodeFixed(token, 32)
    } catch {
      throw denied()
    }
    const hash = sha256(bytes)
    bytes.fill(0)
    const { rows } = await this.pool.query(
      'SELECT s.user_id,s.device_id FROM auth_sessions s JOIN accounts a ON a.user_id=s.user_id JOIN devices d ON d.device_id=s.device_id AND d.user_id=s.user_id WHERE s.token_hash=$1 AND s.expires_at_ms>$2 AND a.disabled_at IS NULL AND d.revoked_at IS NULL',
      [hash, BigInt(this.now())]
    )
    if (!rows.length) {
      throw denied()
    }
    return { user_id: rows[0].user_id, device_id: rows[0].device_id }
  }

  /**
   * Store opaque attachment ciphertext. Reusing an ID is idempotent only for
   * identical ciphertext.
   */
  async putEncryptedAttachment(token, attachmentId, ciphertext) {
    await this.authenticate(token)
    const id = parseUuid(attachmentId)
    if (!id) {
      throw invalid()
    }
    if (isNilUuid(id) || id !== attachmentId || ciphertext.length < 17 || ciphertext.length > MAX_BLOB_BYTES) {
      throw invalid()
    }
    const size = ciphertext.length
    const digest = sha256(ciphertext)
    await this.pool.query(
      `WITH expired AS (
                 SELECT attachment_id FROM encrypted_attachments
                 WHERE created_at <= now() - interval '30 days'
                 ORDER BY created_at LIMIT 100 FOR UPDATE SKIP LOCKED
             )
             DELETE FROM encrypted_attachments a USING expired e
             WHERE a.attachment_id=e.attachment_id`
    )
    await this.pool.query(
      `INSERT INTO encrypted_attachments
                 (attachment_id,ciphertext,ciphertext_size_bytes,ciphertext_sha256)
             VALUES ($1,$2,$3,$4) ON CONFLICT (attachment_id) DO NOTHING`,
      [id, ciphertext, BigInt(size), digest]
    )
    const { rows } = await this.pool.query(
      `SELECT ciphertext_size_bytes,ciphertext_sha256
             FROM encrypted_attachments WHERE attachment_id=$1`,
      [id]
    )
    if (!rows.length) {
      throw unavailable()
    }
    if (rows[0].ciphertext_size_bytes !== BigInt(size) || !rows[0].ciphertext_sha256.equals(digest)) {
      throw new AuthError('Conflict')
    }
    return { size, digest }
  }

  async getEncryptedAttachment(token, attachmentId) {
    await this.authenticate(token)
    const id = parseUuid(attachmentId)
    if (!id || isNilUuid(id) || id !== attachmentId) {
      throw invalid()
    }
    const { rows } = await this.pool.query(
      `SELECT ciphertext,ciphertext_sha256 FROM encrypted_attachments
             WHERE attachment_id=$1 AND created_at > now() - interval '30 days'`,
      [id]
    )
    if (!rows.length) {
      throw new AuthError('NotFound')
    }
    return { ciphertext: rows[0].ciphertext, digest: rows[0].ciphertext_sha256 }
  }

  /** Revoke the presented session. An absent well-formed token is treated as logged out. */
  async logout(token) {
    let bytes
    try {
      bytes = decodeFixed(token, 32)
    } catch {
      throw denied()
    }
    await this.pool.query('DELETE FROM auth_sessions WHERE token_hash=$1', [sha256(bytes)])
  }

  /** Revoke every other session for the account, keeping the presented one. */
  async revokeOtherSessions(token) {
    const account = await this.authenticate(token)
    const bytes = decodeFixed(token, 32)
    await this.pool.query('DELETE FROM auth_sessions WHERE user_id=$1 AND token_hash<>$2', [
      account.user_id,
      sha256(bytes),
    ])
  }

  // --- Groups and organizations ----------------------------------------------

  async createGroup(token, request) {
    if (isNilUuid(request.group_id)) {
      throw invalid()
    }
    const kind = request.kind ?? 'group'
    if (!['direct', 'group', 'channel'].includes(kind)) {
      throw invalid()
    }
    const account = await this.authenticate(token)
    await this.store.createGroup(request.group_id, account.user_id, kind)
    return { group_id: request.group_id, owner_id: account.user_id, kind }
  }

  async organizationControls(token) {
    const account = await this.authenticate(token)
    return organizationControlsResponse(await this.store.organizationControls(account.user_id, account.device_id))
  }

  async setOrganizationControls(token, request) {
    const account = await this.authenticate(token)
    return organizationControlsResponse(
      await this.store.setOrganizationControls(
        account.user_id,
        account.device_id,
        request.mini_apps_enabled,
        request.bots_enabled
      )
    )
  }

  async groupMembers(token, groupId) {
    if (isNilUuid(groupId)) {
      throw invalid()
    }
    const account = await this.authenticate(token)
    const members = await this.store.groupMembers(groupId, account.user_id)
    return { group_id: groupId, members: members.map(member => ({ user_id: member.user_id, role: member.role })) }
  }

  async setGroupRole(token, groupId, targetUserId, request) {
    if (isNilUuid(groupId) || isNilUuid(targetUserId)) {
      throw invalid()
    }
    let role
    try {
      role = parseRole(request.role)
    } catch {
      throw invalid()
    }
    const account = await this.authenticate(token)
    await this.store.setRole(groupId, account.user_id, targetUserId, role)
  }

  async removeGroupMember(token, groupId, targetUserId) {
    if (isNilUuid(groupId) || isNilUuid(targetUserId)) {
      throw invalid()
    }
    const account = await this.authenticate(token)
    await this.store.removeMember(groupId, account.user_id, targetUserId)
  }

  async deleteGroup(token, groupId) {
    if (isNilUuid(groupId)) {
      throw invalid()
    }
    const account = await this.authenticate(token)
    await this.store.deleteGroup(groupId, account.user_id)
  }

  // --- Devices ---------------------------------------------------------------

  async revokeDevice(token, deviceId) {
    if (isNilUuid(deviceId)) {
      throw invalid()
    }
    const account = await this.authenticate(token)
    await this.store.revokeDevice(account.user_id, deviceId)
    return { device_id: deviceId, revoked: true }
  }

  /**
   * Register an additional physical client under the authenticated account.
   * The new device proves possession of its own Ed25519 identity key.
   */
  async registerDevice(token, request) {
    if (isNilUuid(request.device_id) || isNilUuid(request.mls_node_id)) {
      throw invalid()
    }
    const account = await this.authenticate(token)
    const publicKey = decodeFixed(request.public_key, 32)
    const nonce = decodeFixed(request.nonce, 32)
    const signature = decodeFixed(request.signature, 64)
    const binding = { userId: account.user_id, deviceId: request.device_id, mlsNodeId: request.mls_node_id, publicKey }
    identity.verify(
      publicKey,
      identity.devicePairingTranscript(account.user_id, request.device_id, request.mls_node_id, publicKey, nonce),
      signature
    )
    const credential = identity.mlsCredential(binding)
    const response = {
      user_id: account.user_id,
      device_id: request.device_id,
      mls_node_id: request.mls_node_id,
      public_key: encode(publicKey),
      mls_credential: encode(credential),
      delegation_certificate: null,
    }
    const tx = await begin(this.pool)
    try {
      const active = await tx.query(
        'SELECT user_id FROM accounts WHERE user_id=$1 AND disabled_at IS NULL FOR UPDATE',
        [account.user_id]
      )
      if (!active.rows.length) {
        throw denied()
      }
      const existing = (
        await tx.query(
          'SELECT user_id,mls_node_id,identity_public_key,mls_credential,revoked_at IS NULL AS active FROM devices WHERE device_id=$1 FOR UPDATE',
          [request.device_id]
        )
      ).rows[0]
      if (existing) {
        const same =
          existing.user_id === account.user_id &&
          existing.mls_node_id === request.mls_node_id &&
          existing.identity_public_key.equals(publicKey) &&
          existing.mls_credential.equals(credential) &&
          existing.active
        if (same) {
          await tx.commit()
          return response
        }
        throw new AuthError('Conflict')
      }
      const nodeTaken = await tx.query('SELECT EXISTS(SELECT 1 FROM devices WHERE mls_node_id=$1)', [
        request.mls_node_id,
      ])
      if (nodeTaken.rows[0].exists) {
        throw new AuthError('Conflict')
      }
      await tx.query(
        'INSERT INTO devices (device_id,user_id,mls_node_id,identity_public_key,mls_credential) VALUES ($1,$2,$3,$4,$5)',
        [request.device_id, account.user_id, request.mls_node_id, publicKey, credential]
      )
      await tx.commit()
      return response
    } finally {
      await tx.release()
    }
  }

  /**
   * Register a device authorized by an active owner/admin key. An admin may
   * not create another admin. The new device also proves possession of its key.
   */
  async registerDelegatedDevice(token, request) {
    const account = await this.authenticate(token)
    const certificateBytes = decodeBlob(request.certificate, MAX_MESSAGE_BYTES)
    let certificate
    try {
      certificate = decode('DeviceSubCertificate', certificateBytes)
      validateDeviceSubcertificate(certificate)
    } catch {
      throw invalid()
    }
    if (certificate.user_id !== account.user_id || certificate.issuer_device_id !== account.device_id) {
      throw denied()
    }
    const now = BigInt(this.now())
    identity.verifyDeviceSubcertificate(certificate, now)
    const subjectDeviceId = parseUuid(certificate.subject_device_id)
    const subjectMlsNodeId = parseUuid(certificate.subject_mls_node_id)
    if (!subjectDeviceId || !subjectMlsNodeId || certificate.subject_public_key.length !== 32) {
      throw invalid()
    }
    const subjectPublicKey = certificate.subject_public_key
    const nonce = decodeFixed(request.nonce, 32)
    const subjectSignature = decodeFixed(request.signature, 64)
    identity.verify(
      subjectPublicKey,
      identity.devicePairingTranscript(account.user_id, subjectDeviceId, subjectMlsNodeId, subjectPublicKey, nonce),
      subjectSignature
    )
    const issuer = (
      await this.pool.query(
        'SELECT delegation_role,delegation_certificate FROM devices WHERE user_id=$1 AND device_id=$2 AND revoked_at IS NULL',
        [account.user_id, account.device_id]
      )
    ).rows[0]
    if (!issuer) {
      throw denied()
    }
    const issuerRole = issuer.delegation_role
    const role = certificate.delegation_role
    if (!((issuerRole === 'owner' && (role === 1 || role === 2)) || (issuerRole === 'admin' && role === 1))) {
      throw denied()
    }
    if (issuerRole === 'admin') {
      if (!issuer.delegation_certificate) {
        throw unavailable()
      }
      let issuerCertificate
      try {
        issuerCertificate = decode('DeviceSubCertificate', issuer.delegation_certificate)
      } catch {
        throw unavailable()
      }
      identity.verifyDeviceSubcertificate(issuerCertificate, now)
      if (
        issuerCertificate.user_id !== account.user_id ||
        issuerCertificate.subject_device_id !== account.device_id ||
        issuerCertificate.delegation_role !== 2
      ) {
        throw unavailable()
      }
    }
    const credential = identity.mlsCredential({
      userId: account.user_id,
      deviceId: subjectDeviceId,
      mlsNodeId: subjectMlsNodeId,
      publicKey: subjectPublicKey,
    })
    await this.store.registerDelegatedDevice(account.user_id, certificate, credential)
    return {
      user_id: account.user_id,
      device_id: subjectDeviceId,
      mls_node_id: subjectMlsNodeId,
      public_key: encode(subjectPublicKey),
      mls_credential: encode(credential),
      delegation_certificate: encode(certificateBytes),
    }
  }

  // --- Passkeys --------------------------------------------------------------

  passkeyConfig() {
    if (!this.passkey) {
      throw unavailable()
    }
    return this.passkey
  }

  async passkeyRegistrationStart(token) {
    await this.authenticate(token)
    this.passkeyConfig()
  }

  async passkeyRegistrationFinish(token) {
    await this.authenticate(token)
    this.passkeyConfig()
  }

  async passkeyAssertionStart(token) {
    await this.authenticate(token)
    this.passkeyConfig()
  }

  async passkeyAssertionFinish(token) {
    await this.authenticate(token)
    this.passkeyConfig()
  }

  async putEncryptedKeyBackup(token, request) {
    const account = await this.authenticate(token)
    if (request.device_id !== account.device_id) {
      throw denied()
    }
    const credentialId = decodeBlob(request.credential_id, 1024)
    const encryptedEnvelope = decodeBlob(request.encrypted_envelope, 8192)
    validateEncryptedBackupEnvelope(request.backup_id, request.device_id, credentialId, encryptedEnvelope)
    await this.store.putEncryptedKeyBackup(
      account.user_id,
      account.device_id,
      request.backup_id,
      credentialId,
      encryptedEnvelope
    )
  }

  async getEncryptedKeyBackup(token, backupId) {
    const account = await this.authenticate(token)
    const record = await this.store.encryptedKeyBackup(account.user_id, backupId)
    return {
      backup_id: record.backup_id,
      device_id: record.device_id,
      credential_id: encode(record.credential_id),
      encrypted_envelope: encode(record.encrypted_envelope),
    }
  }

  // --- Pre-keys and MLS key packages -----------------------------------------

  async prekeyInventory(token) {
    const account = await this.authenticate(token)
    return this.store.prekeyInventory(account.user_id, account.device_id)
  }

  async uploadPrekeys(token, upload) {
    const account = await this.authenticate(token)
    if (upload.device_id !== account.device_id) {
      throw denied()
    }
    verifyPrekeyUpload(upload)
    return this.store.uploadPrekeys(account.user_id, account.device_id, upload)
  }

  async claimPrekeyBundle(token, targetDeviceId) {
    await this.authenticate(token)
    try {
      return await this.store.claimPrekeyBundle(targetDeviceId)
    } catch (error) {
      // The directory already lists this device. A missing profile means
      // it has not opened a client, not that the caller's token failed.
      if (error instanceof StoreError && error.kind === 'NotFound') {
        throw new AuthError('NotFound')
      }
      throw error
    }
  }

  async putMlsKeyPackage(token, keyPackage) {
    const account = await this.authenticate(token)
    if (keyPackage.length === 0 || keyPackage.length > MAX_FRAME_BYTES) {
      throw invalid()
    }
    await this.store.putMlsKeyPackage(account.device_id, keyPackage)
  }

  async getMlsKeyPackage(token, targetDeviceId) {
    await this.authenticate(token)
    const keyPackage = await this.store.mlsKeyPackage(targetDeviceId)
    if (!keyPackage) {
      throw new AuthError('NotFound')
    }
    return keyPackage
  }

  // --- Contact discovery -----------------------------------------------------

  async contactPsiParameters(token, peerIp) {
    const account = await this.authenticate(token)
    await this.enforceContactPsiRateLimits(account.user_id, peerIp, this.now(), 1)
    const { rows } = await this.pool.query(
      'SELECT a.contact_directory_token FROM accounts a WHERE a.disabled_at IS NULL AND a.contact_directory_token IS NOT NULL AND EXISTS (SELECT 1 FROM devices d WHERE d.user_id=a.user_id AND d.revoked_at IS NULL) ORDER BY a.contact_directory_token'
    )
    if (rows.length > contactPsi.MAX_DIRECTORY_TOKENS) {
      throw unavailable()
    }
    const tokens = rows.map(row => {
      if (row.contact_directory_token.length !== contactPsi.TOKEN_BYTES) {
        throw unavailable()
      }
      return row.contact_directory_token
    })
    let filter
    try {
      filter = contactPsi.filterFromTokens(tokens)
    } catch {
      throw unavailable()
    }
    return {
      protocol_version: contactPsi.VERSION,
      server_public_key: encode(contactPsi.serverPublicKey(this.phoneLookupKey)),
      filter: encode(filter.bits),
      filter_hash_count: filter.hashCount,
      filter_item_count: filter.itemCount,
    }
  }

  async contactPsiQuery(token, peerIp, request) {
    if (
      request.protocol_version !== contactPsi.VERSION ||
      request.blinded_inputs.length === 0 ||
      request.blinded_inputs.length > contactPsi.MAX_QUERY_ITEMS
    ) {
      throw invalid()
    }
    const account = await this.authenticate(token)
    await this.enforceContactPsiRateLimits(account.user_id, peerIp, this.now(), request.blinded_inputs.length)
    const evaluations = []
    for (const encoded of request.blinded_inputs) {
      const blinded = decodeFixed(encoded, contactPsi.POINT_BYTES)
      let evaluation
      try {
        evaluation = contactPsi.evaluateBlinded(this.phoneLookupKey, blinded, randomBytes(64))
      } catch {
        throw invalid()
      }
      evaluations.push({ evaluated_point: encode(evaluation.evaluatedPoint), proof: encode(evaluation.proof) })
    }
    return { protocol_version: contactPsi.VERSION, evaluations }
  }

  // --- Privacy Pass ----------------------------------------------------------

  privacyPassParameters() {
    let parameters
    try {
      parameters = privacyPass.issuerParameters(this.privacyPassKey)
    } catch {
      throw unavailable()
    }
    return {
      protocol_version: privacyPass.VERSION,
      token_type: privacyPass.TOKEN_TYPE,
      public_key: encode(parameters.publicKey),
      token_key_id: encode(parameters.tokenKeyId),
    }
  }

  /** Issue a blind signature after authenticating the client. */
  async privacyPassIssue(token, peerIp, request) {
    if (request.protocol_version !== privacyPass.VERSION || request.token_type !== privacyPass.TOKEN_TYPE) {
      throw invalid()
    }
    const account = await this.authenticate(token)
    const user = uuidBytes(account.user_id)
    await this.enforceRateLimitSet([
      [this.digest('links/privacy-pass-issue-user-minute/v1\0', user), 60_000, 5],
      [this.digest('links/privacy-pass-issue-user-hour/v1\0', user), 3_600_000, 30],
      [this.digest('links/privacy-pass-issue-ip-hour/v1\0', peerIp), 3_600_000, 100],
    ])
    let parameters
    try {
      parameters = privacyPass.issuerParameters(this.privacyPassKey)
    } catch {
      throw unavailable()
    }
    if (request.truncated_token_key_id !== parameters.tokenKeyId[privacyPass.TOKEN_KEY_ID_BYTES - 1]) {
      throw invalid()
    }
    const blindedMessage = decodeFixed(request.blinded_message, privacyPass.POINT_BYTES)
    let response
    try {
      response = privacyPass.evaluate(
        this.privacyPassKey,
        {
          tokenType: request.token_type,
          truncatedTokenKeyId: request.truncated_token_key_id,
          blindedMessage,
        },
        randomBytes(privacyPass.SCALAR_BYTES)
      )
    } catch {
      throw invalid()
    }
    return {
      protocol_version: privacyPass.VERSION,
      token_type: privacyPass.TOKEN_TYPE,
      evaluated_message: encode(response.evaluatedMessage),
      proof: encode(response.proof),
    }
  }

  /** Create an origin challenge without creating an identity-bound record. */
  async privacyPassChallenge(peerIp) {
    const now = this.now()
    await this.enforceRateLimitSet([
      [this.digest('links/privacy-pass-challenge-ip-hour/v1\0', peerIp), 3_600_000, 120],
    ])
    const expiresAtMs = now + PRIVACY_PASS_CHALLENGE_TTL_MS
    const challenge = Buffer.alloc(privacyPass.CHALLENGE_BYTES)
    randomBytes(32).copy(challenge, 0)
    challenge.writeBigUInt64BE(BigInt(expiresAtMs), 32)
    return {
      protocol_version: privacyPass.VERSION,
      token_type: privacyPass.TOKEN_TYPE,
      challenge: encode(challenge),
      expires_at_ms: expiresAtMs,
    }
  }

  /** Redeem a token without bearer authentication. Only a token digest is stored. */
  async privacyPassRedeem(peerIp, request) {
    if (request.protocol_version !== privacyPass.VERSION) {
      throw invalid()
    }
    await this.enforceRateLimitSet([[this.digest('links/privacy-pass-redeem-ip-hour/v1\0', peerIp), 3_600_000, 200]])
    const tokenBytes = decodeFixed(request.token, privacyPass.TOKEN_BYTES)
    const challenge = decodeFixed(request.challenge, privacyPass.CHALLENGE_BYTES)
    let token
    try {
      token = privacyPass.tokenFromBytes(tokenBytes)
      privacyPass.verifyToken(token, challenge, this.now(), this.privacyPassKey)
    } catch {
      throw denied()
    }
    const expiresAtMs = privacyPass.challengeExpiryMs(challenge)
    if (expiresAtMs > 2n ** 63n - 1n) {
      throw denied()
    }
    const inserted = await this.pool.query(
      'INSERT INTO privacy_pass_redeemed (token_hash,expires_at_ms) VALUES ($1,$2) ON CONFLICT (token_hash) DO NOTHING',
      [sha256(tokenBytes), expiresAtMs]
    )
    if (inserted.rowCount !== 1) {
      throw denied()
    }
    return { protocol_version: privacyPass.VERSION, accepted: true }
  }

  // --- Chat proof of work ----------------------------------------------------

  async enforceChatPowRateLimits(domain, userId, peerIp, userLimit, ipLimit) {
    await this.enforceRateLimitSet([
      [this.digest(domain, uuidBytes(userId)), 3_600_000, userLimit],
      [this.digest('links/chat-request-pow-ip-hour/v1\0', peerIp), 3_600_000, ipLimit],
    ])
  }

  /** Short-lived hashcash challenge for a pseudonymous account. */
  async chatPowChallenge(token, peerIp) {
    const account = await this.authenticate(token)
    await this.requireUnverifiedAccount(account)
    await this.enforceChatPowRateLimits('links/chat-request-pow-challenge-user-hour/v1\0', account.user_id, peerIp, 20, 200)
    const now = this.now()
    const expiresAtMs = now + PROOF_OF_WORK_CHALLENGE_TTL_MS
    const challenge = randomBytes(32)
    const challengeHash = this.digest('links/chat-request-pow-challenge/v1\0', challenge)
    await this.pool.query(
      "INSERT INTO proof_of_work_challenges (challenge_hash,user_id,device_id,difficulty_bits,expires_at_ms,state) VALUES ($1,$2,$3,$4,$5,'issued')",
      [challengeHash, account.user_id, account.device_id, proofOfWork.DEFAULT_DIFFICULTY_BITS, BigInt(expiresAtMs)]
    )
    return {
      protocol_version: proofOfWork.VERSION,
      challenge: encode(challenge),
      difficulty_bits: proofOfWork.DEFAULT_DIFFICULTY_BITS,
      expires_at_ms: expiresAtMs,
    }
  }

  /** Verify and consume one proof-of-work challenge bound to the account and device. */
  async chatPowVerify(token, peerIp, request) {
    if (request.protocol_version !== proofOfWork.VERSION) {
      throw invalid()
    }
    const account = await this.authenticate(token)
    await this.requireUnverifiedAccount(account)
    await this.enforceChatPowRateLimits('links/chat-request-pow-verify-user-hour/v1\0', account.user_id, peerIp, 60, 300)
    const challenge = decodeFixed(request.challenge, proofOfWork.CHALLENGE_BYTES)
    const challengeHash = this.digest('links/chat-request-pow-challenge/v1\0', challenge)
    const now = this.now()
    const tx = await begin(this.pool)
    try {
      const row = (
        await tx.query(
          'SELECT user_id,device_id,difficulty_bits,expires_at_ms,state FROM proof_of_work_challenges WHERE challenge_hash=$1 FOR UPDATE',
          [challengeHash]
        )
      ).rows[0]
      if (!row) {
        throw denied()
      }
      const difficultyBits = row.difficulty_bits
      if (difficultyBits < 0 || difficultyBits > 255) {
        throw unavailable()
      }
      if (
        row.state !== 'issued' ||
        row.expires_at_ms <= BigInt(now) ||
        row.user_id !== account.user_id ||
        row.device_id !== account.device_id
      ) {
        throw denied()
      }
      if (!proofOfWork.verify(challenge, difficultyBits, request.nonce)) {
        throw denied()
      }
      const updated = await tx.query(
        "UPDATE proof_of_work_challenges SET state='consumed' WHERE challenge_hash=$1 AND state='issued'",
        [challengeHash]
      )
      if (updated.rowCount !== 1) {
        throw denied()
      }
      await tx.commit()
      return { protocol_version: proofOfWork.VERSION, accepted: true }
    } finally {
      await tx.release()
    }
  }

  async purgeExpired() {
    const now = BigInt(this.now())
    await this.pool.query('DELETE FROM auth_sessions WHERE expires_at_ms <= $1', [now])
    // Keep consumed provider SIDs for another 10 minutes after local expiry.
    await this.pool.query('DELETE FROM auth_challenges WHERE expires_at_ms <= $1', [now - BigInt(CHALLENGE_TTL_MS)])
    await this.pool.query('DELETE FROM passkey_challenges WHERE expires_at_ms <= $1', [
      now - BigInt(PASSKEY_CHALLENGE_TTL_MS),
    ])
    await this.pool.query('DELETE FROM username_auth_challenges WHERE expires_at_ms <= $1', [
      now - BigInt(CHALLENGE_TTL_MS),
    ])
    await this.pool.query('DELETE FROM auth_rate_limits WHERE window_start_ms <= $1', [now - 3_600_000n])
    await this.pool.query('DELETE FROM privacy_pass_redeemed WHERE expires_at_ms <= $1', [now])
    await this.pool.query('DELETE FROM proof_of_work_challenges WHERE expires_at_ms <= $1', [now])
  }
}

function organizationControlsResponse(controls) {
  return {
    organization_id: controls.organization_id,
    mini_apps_enabled: controls.mini_apps_enabled,
    bots_enabled: controls.bots_enabled,
    revision: Number(controls.revision),
  }
}

// Every public service method converts store, identity, and database
// failures into the AuthError taxonomy exactly like the Rust `From` impls.
for (const name of Object.getOwnPropertyNames(AccountAuth.prototype)) {
  const method = AccountAuth.prototype[name]
  if (name === 'constructor' || typeof method !== 'function') {
    continue
  }
  AccountAuth.prototype[name] = function (...args) {
    try {
      const result = method.apply(this, args)
      if (result instanceof Promise) {
        return result.catch(error => {
          throw authErrorFrom(error)
        })
      }
      return result
    } catch (error) {
      throw authErrorFrom(error)
    }
  }
}
