// PostgreSQL relational store and ciphertext-only mailbox.
// Ported from crates/server-store/src/postgres.rs; SQL is kept verbatim.
import { createHash } from 'node:crypto'

import { ProtocolError, StoreError, storeErrorFromDatabase } from '../errors.js'
import { create, decode, encode } from '../proto.js'
import {
  MAX_CURSOR,
  MAX_FRAME_BYTES,
  MAX_ONE_TIME_PREKEYS,
  MAX_RETENTION_MS,
  VERSION,
  decodeEnvelope,
  didKeyForEd25519,
  parseUuid,
  queueItemFieldLen,
  syncBatchEncodedLen,
  validateDeviceSubcertificate,
  validateHandle,
  validatePrekeyBundle,
  validatePrekeyUpload,
  validateVerificationBadge,
} from '../protocol.js'
import { begin } from './db.js'

export const Role = Object.freeze({ Owner: 'owner', Admin: 'admin', Member: 'member' })
const ROLES = new Set(Object.values(Role))

export function parseRole(value) {
  if (!ROLES.has(value)) {
    throw new StoreError('Invalid')
  }
  return value
}

export const MAX_PURGE_BATCH = 1_000

function sha256(bytes) {
  return createHash('sha256').update(bytes).digest()
}

function signedCursor(value) {
  const big = BigInt(value)
  if (big > MAX_CURSOR) {
    throw new StoreError('Invalid')
  }
  return big
}

function unsignedCursor(value) {
  const big = BigInt(value)
  if (big < 0n) {
    throw new StoreError('Unavailable')
  }
  return big
}

function parseUuidOrInvalid(value) {
  const parsed = parseUuid(value)
  if (!parsed) {
    throw new StoreError('Invalid')
  }
  return parsed
}

function storeCall(target, name) {
  const method = target[name]
  return async function (...args) {
    try {
      return await method.apply(this, args)
    } catch (error) {
      throw storeErrorFromDatabase(error)
    }
  }
}

async function requireActiveAccount(tx, user) {
  // SHARE (not UPDATE) allows independent memberships for the same account.
  const { rows } = await tx.query(
    'SELECT user_id FROM accounts WHERE user_id=$1 AND disabled_at IS NULL FOR SHARE',
    [user]
  )
  if (!rows.length) {
    throw new StoreError('Forbidden')
  }
}

async function lockGroup(tx, group) {
  const { rows } = await tx.query('SELECT group_id FROM groups WHERE group_id=$1 FOR UPDATE', [group])
  if (!rows.length) {
    throw new StoreError('NotFound')
  }
}

async function roleIn(tx, group, user) {
  const { rows } = await tx.query('SELECT role FROM group_memberships WHERE group_id=$1 AND user_id=$2', [
    group,
    user,
  ])
  return rows.length ? parseRole(rows[0].role) : null
}

async function requireOtherOwner(tx, group) {
  const { rows } = await tx.query("SELECT count(*) FROM group_memberships WHERE group_id=$1 AND role='owner'", [
    group,
  ])
  if (rows[0].count < 2n) {
    throw new StoreError('Forbidden')
  }
}

async function withTx(pool, work) {
  const tx = await begin(pool)
  try {
    return await work(tx)
  } finally {
    await tx.release()
  }
}

export class RelationalStore {
  constructor(pool) {
    this.pool = pool
  }

  async lookupHandleDirectory(handle) {
    validateHandle(handle)
    const { rows } = await this.pool.query(
      'SELECT h.user_id,a.verification_badge,d.device_id,d.mls_node_id,d.identity_public_key,d.mls_credential,d.delegation_role,d.delegation_certificate FROM handles h JOIN accounts a USING (user_id) JOIN devices d USING (user_id) WHERE h.handle=$1 AND a.disabled_at IS NULL AND d.revoked_at IS NULL ORDER BY d.device_id',
      [handle]
    )
    if (!rows.length) {
      return null
    }
    const userId = rows[0].user_id
    const verificationBadge = rows[0].verification_badge
    if (verificationBadge) {
      let badge
      try {
        badge = decode('VerificationBadge', verificationBadge)
        validateVerificationBadge(badge)
      } catch {
        throw new StoreError('CorruptObject')
      }
      if (badge.subject_user_id !== userId) {
        throw new StoreError('CorruptObject')
      }
    }
    const devices = []
    for (const row of rows) {
      if (row.user_id !== userId) {
        throw new StoreError('CorruptObject')
      }
      const identityPublicKey = row.identity_public_key
      const mlsCredential = row.mls_credential
      if (identityPublicKey.length !== 32 || mlsCredential.length === 0 || mlsCredential.length > 65_536) {
        throw new StoreError('CorruptObject')
      }
      let did
      try {
        did = didKeyForEd25519(identityPublicKey)
      } catch {
        throw new StoreError('CorruptObject')
      }
      const roleIds = { owner: 0, device: 1, admin: 2 }
      const delegationRoleId = roleIds[row.delegation_role]
      if (delegationRoleId === undefined) {
        throw new StoreError('CorruptObject')
      }
      const delegationCertificate = row.delegation_certificate
      if (delegationCertificate) {
        let certificate
        try {
          certificate = decode('DeviceSubCertificate', delegationCertificate)
          validateDeviceSubcertificate(certificate)
        } catch {
          throw new StoreError('CorruptObject')
        }
        if (
          certificate.user_id !== userId ||
          certificate.subject_device_id !== row.device_id ||
          certificate.subject_mls_node_id !== row.mls_node_id ||
          !certificate.subject_public_key.equals(identityPublicKey) ||
          certificate.delegation_role !== delegationRoleId
        ) {
          throw new StoreError('CorruptObject')
        }
      } else if (delegationRoleId !== 0) {
        throw new StoreError('CorruptObject')
      }
      devices.push({
        device_id: row.device_id,
        mls_node_id: row.mls_node_id,
        did,
        identity_public_key: identityPublicKey,
        mls_credential: mlsCredential,
        delegation_role: row.delegation_role,
        delegation_certificate: delegationCertificate,
      })
    }
    return { user_id: userId, devices, verification_badge: verificationBadge }
  }

  async putMlsKeyPackage(deviceId, keyPackage) {
    if (keyPackage.length === 0 || keyPackage.length > MAX_FRAME_BYTES) {
      throw new StoreError('Invalid')
    }
    await this.pool.query(
      'INSERT INTO device_mls_key_packages (device_id,key_package) VALUES ($1,$2) ON CONFLICT (device_id) DO UPDATE SET key_package=EXCLUDED.key_package,updated_at=now()',
      [deviceId, keyPackage]
    )
  }

  async mlsKeyPackage(deviceId) {
    const { rows } = await this.pool.query(
      'SELECT key_package FROM device_mls_key_packages dkp JOIN devices d USING (device_id) WHERE dkp.device_id=$1 AND d.revoked_at IS NULL',
      [deviceId]
    )
    return rows.length ? rows[0].key_package : null
  }

  /** Atomically register a device authorized by a signed sub-certificate. */
  async registerDelegatedDevice(userId, certificate, credential) {
    try {
      validateDeviceSubcertificate(certificate)
    } catch (error) {
      throw new StoreError('Protocol', error)
    }
    const certificateUser = parseUuidOrInvalid(certificate.user_id)
    const issuerDeviceId = parseUuidOrInvalid(certificate.issuer_device_id)
    const issuerMlsNodeId = parseUuidOrInvalid(certificate.issuer_mls_node_id)
    const subjectDeviceId = parseUuidOrInvalid(certificate.subject_device_id)
    const subjectMlsNodeId = parseUuidOrInvalid(certificate.subject_mls_node_id)
    if (certificateUser !== userId || credential.length === 0 || credential.length > 65_536) {
      throw new StoreError('Invalid')
    }
    const role = { 1: 'device', 2: 'admin' }[certificate.delegation_role]
    if (!role) {
      throw new StoreError('Invalid')
    }
    await withTx(this.pool, async tx => {
      await requireActiveAccount(tx, userId)
      const issuer = await tx.query(
        'SELECT user_id,mls_node_id,identity_public_key,delegation_role,revoked_at IS NULL AS active FROM devices WHERE device_id=$1 FOR UPDATE',
        [issuerDeviceId]
      )
      const row = issuer.rows[0]
      if (!row) {
        throw new StoreError('Forbidden')
      }
      if (
        row.user_id !== userId ||
        row.mls_node_id !== issuerMlsNodeId ||
        !row.identity_public_key.equals(certificate.issuer_public_key) ||
        !row.active
      ) {
        throw new StoreError('Forbidden')
      }
      const allowed =
        (row.delegation_role === 'owner' && (role === 'device' || role === 'admin')) ||
        (row.delegation_role === 'admin' && role === 'device')
      if (!allowed) {
        throw new StoreError('Forbidden')
      }
      const exists = await tx.query(
        'SELECT EXISTS(SELECT 1 FROM devices WHERE device_id=$1 OR mls_node_id=$2)',
        [subjectDeviceId, subjectMlsNodeId]
      )
      if (exists.rows[0].exists) {
        throw new StoreError('Conflict')
      }
      await tx.query(
        'INSERT INTO devices (device_id,user_id,mls_node_id,identity_public_key,mls_credential,delegation_role,delegated_by_device_id,delegation_certificate) VALUES ($1,$2,$3,$4,$5,$6,$7,$8)',
        [
          subjectDeviceId,
          userId,
          subjectMlsNodeId,
          certificate.subject_public_key,
          credential,
          role,
          issuerDeviceId,
          encode('DeviceSubCertificate', certificate),
        ]
      )
      await tx.commit()
    })
  }

  async revokeDevice(actorUserId, deviceId) {
    const result = await this.pool.query(
      'WITH RECURSIVE descendants AS (SELECT device_id FROM devices WHERE device_id=$1 AND user_id=$2 UNION ALL SELECT child.device_id FROM devices child JOIN descendants parent ON child.delegated_by_device_id=parent.device_id WHERE child.user_id=$2) UPDATE devices SET revoked_at=COALESCE(revoked_at,now()) WHERE device_id IN (SELECT device_id FROM descendants) AND user_id=$2',
      [deviceId, actorUserId]
    )
    if (result.rowCount === 0) {
      throw new StoreError('NotFound')
    }
  }

  async putEncryptedKeyBackup(userId, deviceId, backupId, credentialId, encryptedEnvelope) {
    if (
      backupId === '00000000-0000-0000-0000-000000000000' ||
      deviceId === '00000000-0000-0000-0000-000000000000' ||
      credentialId.length === 0 ||
      credentialId.length > 1024 ||
      encryptedEnvelope.length < 128 ||
      encryptedEnvelope.length > 8192
    ) {
      throw new StoreError('Invalid')
    }
    await withTx(this.pool, async tx => {
      await requireActiveAccount(tx, userId)
      const existing = await tx.query(
        'SELECT backup_id,credential_id,encrypted_envelope FROM encrypted_key_backups WHERE user_id=$1 AND device_id=$2 FOR UPDATE',
        [userId, deviceId]
      )
      const row = existing.rows[0]
      if (row) {
        if (
          row.backup_id !== backupId ||
          !row.credential_id.equals(credentialId) ||
          !row.encrypted_envelope.equals(encryptedEnvelope)
        ) {
          throw new StoreError('Conflict')
        }
        await tx.commit()
        return
      }
      await tx.query(
        'INSERT INTO encrypted_key_backups (backup_id,user_id,device_id,credential_id,encrypted_envelope) VALUES ($1,$2,$3,$4,$5)',
        [backupId, userId, deviceId, credentialId, encryptedEnvelope]
      )
      await tx.commit()
    })
  }

  async encryptedKeyBackup(userId, backupId) {
    const { rows } = await this.pool.query(
      'SELECT backup_id,device_id,credential_id,encrypted_envelope FROM encrypted_key_backups WHERE user_id=$1 AND backup_id=$2',
      [userId, backupId]
    )
    if (!rows.length) {
      throw new StoreError('NotFound')
    }
    return rows[0]
  }

  async prekeyInventory(userId, deviceId) {
    const { rows } = await this.pool.query(
      `SELECT p.profile_revision,
                (SELECT count(*) FROM device_curve_one_time_prekeys c WHERE c.device_id=p.device_id) AS curve_count,
                (SELECT count(*) FROM device_kem_one_time_prekeys k WHERE k.device_id=p.device_id) AS kem_count
             FROM device_prekey_profiles p
             JOIN devices d USING (device_id)
             JOIN accounts a USING (user_id)
             WHERE p.device_id=$1 AND d.user_id=$2 AND d.revoked_at IS NULL AND a.disabled_at IS NULL`,
      [deviceId, userId]
    )
    if (!rows.length) {
      const owns = await this.pool.query(
        'SELECT EXISTS(SELECT 1 FROM devices d JOIN accounts a USING (user_id) WHERE d.device_id=$1 AND d.user_id=$2 AND d.revoked_at IS NULL AND a.disabled_at IS NULL)',
        [deviceId, userId]
      )
      if (!owns.rows[0].exists) {
        throw new StoreError('Forbidden')
      }
      return create('PreKeyInventory', {
        protocol_version: VERSION,
        device_id: deviceId,
        profile_revision: 0n,
        one_time_curve_prekeys: 0,
        one_time_kem_prekeys: 0,
      })
    }
    return create('PreKeyInventory', {
      protocol_version: VERSION,
      device_id: deviceId,
      profile_revision: BigInt.asUintN(64, rows[0].profile_revision),
      one_time_curve_prekeys: Number(BigInt.asUintN(32, rows[0].curve_count)),
      one_time_kem_prekeys: Number(BigInt.asUintN(32, rows[0].kem_count)),
    })
  }

  async uploadPrekeys(userId, deviceId, upload) {
    try {
      validatePrekeyUpload(upload)
    } catch (error) {
      throw new StoreError('Protocol', error)
    }
    if (upload.device_id !== deviceId) {
      throw new StoreError('Forbidden')
    }
    const profile = upload.profile
    const identity = profile.identity
    const signed = profile.signed_prekey
    const signedKey = signed.prekey
    const last = profile.last_resort_kem_prekey
    const uploadId = parseUuidOrInvalid(upload.upload_id)
    const uploadDigest = sha256(encode('PreKeyUpload', upload))
    const revision = upload.profile_revision
    return withTx(this.pool, async tx => {
      const enrolled = await tx.query(
        'SELECT d.identity_public_key FROM devices d JOIN accounts a USING (user_id) WHERE d.device_id=$1 AND d.user_id=$2 AND d.revoked_at IS NULL AND a.disabled_at IS NULL FOR UPDATE OF d',
        [deviceId, userId]
      )
      if (!enrolled.rows.length || !enrolled.rows[0].identity_public_key.equals(identity.signing_key)) {
        throw new StoreError('Forbidden')
      }
      const currentResult = await tx.query('SELECT * FROM device_prekey_profiles WHERE device_id=$1', [deviceId])
      const current = currentResult.rows[0]
      if (current) {
        const currentRevision = current.profile_revision
        if (currentRevision > revision) {
          throw new StoreError('Conflict')
        }
        if (
          currentRevision === revision &&
          (!current.identity_dh_key.equals(identity.dh_key) ||
            !current.identity_binding_signature.equals(identity.binding_signature) ||
            current.signed_curve_prekey_id !== signedKey.id ||
            !current.signed_curve_prekey.equals(signedKey.public_key) ||
            !current.signed_curve_signature.equals(signed.signature) ||
            current.last_resort_kem_prekey_id !== last.id ||
            !current.last_resort_kem_prekey.equals(last.public_key) ||
            !current.last_resort_kem_signature.equals(last.signature))
        ) {
          throw new StoreError('Conflict')
        }
        if (currentRevision < revision) {
          await tx.query('DELETE FROM device_prekey_uploads WHERE device_id=$1', [deviceId])
          await tx.query('DELETE FROM device_curve_one_time_prekeys WHERE device_id=$1', [deviceId])
          await tx.query('DELETE FROM device_kem_one_time_prekeys WHERE device_id=$1', [deviceId])
        }
      }
      const prior = await tx.query(
        'SELECT upload_digest FROM device_prekey_uploads WHERE device_id=$1 AND upload_id=$2 AND profile_revision=$3',
        [deviceId, uploadId, revision]
      )
      if (prior.rows.length) {
        if (!prior.rows[0].upload_digest.equals(uploadDigest)) {
          throw new StoreError('Conflict')
        }
        const curve = await tx.query('SELECT count(*) FROM device_curve_one_time_prekeys WHERE device_id=$1', [
          deviceId,
        ])
        const kem = await tx.query('SELECT count(*) FROM device_kem_one_time_prekeys WHERE device_id=$1', [deviceId])
        await tx.commit()
        return create('PreKeyInventory', {
          protocol_version: VERSION,
          device_id: deviceId,
          profile_revision: upload.profile_revision,
          one_time_curve_prekeys: Number(BigInt.asUintN(32, curve.rows[0].count)),
          one_time_kem_prekeys: Number(BigInt.asUintN(32, kem.rows[0].count)),
        })
      }
      await tx.query(
        `INSERT INTO device_prekey_profiles (device_id,profile_revision,identity_dh_key,identity_binding_signature,signed_curve_prekey_id,signed_curve_prekey,signed_curve_signature,last_resort_kem_prekey_id,last_resort_kem_prekey,last_resort_kem_signature)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)
             ON CONFLICT (device_id) DO UPDATE SET profile_revision=EXCLUDED.profile_revision,identity_dh_key=EXCLUDED.identity_dh_key,identity_binding_signature=EXCLUDED.identity_binding_signature,signed_curve_prekey_id=EXCLUDED.signed_curve_prekey_id,signed_curve_prekey=EXCLUDED.signed_curve_prekey,signed_curve_signature=EXCLUDED.signed_curve_signature,last_resort_kem_prekey_id=EXCLUDED.last_resort_kem_prekey_id,last_resort_kem_prekey=EXCLUDED.last_resort_kem_prekey,last_resort_kem_signature=EXCLUDED.last_resort_kem_signature,updated_at=now()`,
        [
          deviceId,
          revision,
          identity.dh_key,
          identity.binding_signature,
          signedKey.id,
          signedKey.public_key,
          signed.signature,
          last.id,
          last.public_key,
          last.signature,
        ]
      )
      // Two replenishments can both observe a pool just below the cap and
      // upload the same shortfall. The device row is locked above, so the
      // second transaction sees the first commit and must keep the stored
      // pools at the cap instead of failing the whole upload.
      let curveCount = (
        await tx.query('SELECT count(*) FROM device_curve_one_time_prekeys WHERE device_id=$1', [deviceId])
      ).rows[0].count
      for (const key of upload.one_time_curve_prekeys) {
        if (curveCount >= BigInt(MAX_ONE_TIME_PREKEYS)) {
          break
        }
        const result = await tx.query(
          'INSERT INTO device_curve_one_time_prekeys (device_id,prekey_id,public_key) VALUES ($1,$2,$3) ON CONFLICT DO NOTHING',
          [deviceId, key.id, key.public_key]
        )
        if (result.rowCount === 0) {
          const same = await tx.query(
            'SELECT public_key=$3 AS same FROM device_curve_one_time_prekeys WHERE device_id=$1 AND prekey_id=$2',
            [deviceId, key.id, key.public_key]
          )
          if (!same.rows.length) {
            throw new StoreError('Database', new Error('no rows returned'))
          }
          if (!same.rows[0].same) {
            throw new StoreError('Conflict')
          }
        } else {
          curveCount += 1n
        }
      }
      let kemCount = (
        await tx.query('SELECT count(*) FROM device_kem_one_time_prekeys WHERE device_id=$1', [deviceId])
      ).rows[0].count
      for (const key of upload.one_time_kem_prekeys) {
        if (kemCount >= BigInt(MAX_ONE_TIME_PREKEYS)) {
          break
        }
        const result = await tx.query(
          'INSERT INTO device_kem_one_time_prekeys (device_id,prekey_id,public_key,signature) VALUES ($1,$2,$3,$4) ON CONFLICT DO NOTHING',
          [deviceId, key.id, key.public_key, key.signature]
        )
        if (result.rowCount === 0) {
          const same = await tx.query(
            'SELECT public_key=$3 AND signature=$4 AS same FROM device_kem_one_time_prekeys WHERE device_id=$1 AND prekey_id=$2',
            [deviceId, key.id, key.public_key, key.signature]
          )
          if (!same.rows.length) {
            throw new StoreError('Database', new Error('no rows returned'))
          }
          if (!same.rows[0].same) {
            throw new StoreError('Conflict')
          }
        } else {
          kemCount += 1n
        }
      }
      await tx.query(
        'INSERT INTO device_prekey_uploads (device_id,upload_id,profile_revision,upload_digest) VALUES ($1,$2,$3,$4)',
        [deviceId, uploadId, revision, uploadDigest]
      )
      await tx.commit()
      return create('PreKeyInventory', {
        protocol_version: VERSION,
        device_id: deviceId,
        profile_revision: upload.profile_revision,
        one_time_curve_prekeys: Number(BigInt.asUintN(32, curveCount)),
        one_time_kem_prekeys: Number(BigInt.asUintN(32, kemCount)),
      })
    })
  }

  async claimPrekeyBundle(targetDeviceId) {
    return withTx(this.pool, async tx => {
      const profileResult = await tx.query(
        'SELECT p.*,d.identity_public_key FROM device_prekey_profiles p JOIN devices d USING (device_id) JOIN accounts a USING (user_id) WHERE p.device_id=$1 AND d.revoked_at IS NULL AND a.disabled_at IS NULL FOR UPDATE OF p',
        [targetDeviceId]
      )
      const row = profileResult.rows[0]
      if (!row) {
        throw new StoreError('NotFound')
      }
      const curve = (
        await tx.query(
          'DELETE FROM device_curve_one_time_prekeys WHERE (device_id,prekey_id) IN (SELECT device_id,prekey_id FROM device_curve_one_time_prekeys WHERE device_id=$1 ORDER BY created_at,prekey_id LIMIT 1 FOR UPDATE) RETURNING prekey_id,public_key',
          [targetDeviceId]
        )
      ).rows[0]
      const kem = (
        await tx.query(
          'DELETE FROM device_kem_one_time_prekeys WHERE (device_id,prekey_id) IN (SELECT device_id,prekey_id FROM device_kem_one_time_prekeys WHERE device_id=$1 ORDER BY created_at,prekey_id LIMIT 1 FOR UPDATE) RETURNING prekey_id,public_key,signature',
          [targetDeviceId]
        )
      ).rows[0]
      const lastResort = create('KemPreKey', {
        id: BigInt.asUintN(64, row.last_resort_kem_prekey_id),
        public_key: row.last_resort_kem_prekey,
        one_time: false,
        signature: row.last_resort_kem_signature,
      })
      const bundle = create('PreKeyBundle', {
        protocol_version: VERSION,
        device_id: targetDeviceId,
        profile_revision: BigInt.asUintN(64, row.profile_revision),
        profile: create('PreKeyProfile', {
          identity: create('PqxdhPublicIdentity', {
            signing_key: row.identity_public_key,
            dh_key: row.identity_dh_key,
            binding_signature: row.identity_binding_signature,
          }),
          signed_prekey: create('SignedCurvePreKey', {
            prekey: create('CurvePreKey', {
              id: BigInt.asUintN(64, row.signed_curve_prekey_id),
              public_key: row.signed_curve_prekey,
            }),
            signature: row.signed_curve_signature,
          }),
          last_resort_kem_prekey: lastResort,
        }),
        one_time_curve_prekey: curve
          ? create('CurvePreKey', { id: BigInt.asUintN(64, curve.prekey_id), public_key: curve.public_key })
          : null,
        kem_prekey: kem
          ? create('KemPreKey', {
              id: BigInt.asUintN(64, kem.prekey_id),
              public_key: kem.public_key,
              one_time: true,
              signature: kem.signature,
            })
          : { ...lastResort },
      })
      try {
        validatePrekeyBundle(bundle)
      } catch (error) {
        throw new StoreError('Protocol', error)
      }
      await tx.commit()
      return bundle
    })
  }

  async createGroup(groupId, owner, kind) {
    await withTx(this.pool, async tx => {
      await requireActiveAccount(tx, owner)
      await tx.query('INSERT INTO groups (group_id, group_kind) VALUES ($1,$2)', [groupId, kind])
      await tx.query("INSERT INTO group_memberships (group_id, user_id, role) VALUES ($1,$2,'owner')", [groupId, owner])
      await tx.commit()
    })
  }

  /** Owner: all roles. Admin: add/update members only. Member: no grants. */
  async setRole(groupId, actor, target, role) {
    await withTx(this.pool, async tx => {
      await lockGroup(tx, groupId)
      await requireActiveAccount(tx, actor)
      await requireActiveAccount(tx, target)
      const actorRole = await roleIn(tx, groupId, actor)
      const current = await roleIn(tx, groupId, target)
      const permitted =
        actorRole === Role.Owner ||
        (actorRole === Role.Admin && role === Role.Member && (current === null || current === Role.Member))
      if (!permitted) {
        throw new StoreError('Forbidden')
      }
      if (current === Role.Owner && role !== Role.Owner) {
        await requireOtherOwner(tx, groupId)
      }
      await tx.query(
        'INSERT INTO group_memberships (group_id,user_id,role) VALUES ($1,$2,$3) ON CONFLICT (group_id,user_id) DO UPDATE SET role = EXCLUDED.role',
        [groupId, target, role]
      )
      await tx.commit()
    })
  }

  async removeMember(groupId, actor, target) {
    await withTx(this.pool, async tx => {
      await lockGroup(tx, groupId)
      await requireActiveAccount(tx, actor)
      const actorRole = await roleIn(tx, groupId, actor)
      const current = await roleIn(tx, groupId, target)
      if (current === null) {
        throw new StoreError('NotFound')
      }
      if (
        actor !== target &&
        actorRole !== Role.Owner &&
        !(actorRole === Role.Admin && current === Role.Member)
      ) {
        throw new StoreError('Forbidden')
      }
      if (current === Role.Owner) {
        await requireOtherOwner(tx, groupId)
      }
      await tx.query('DELETE FROM group_memberships WHERE group_id=$1 AND user_id=$2', [groupId, target])
      await tx.commit()
    })
  }

  /** Delete a many-to-many group and every membership. Only an owner may disband it. */
  async deleteGroup(groupId, actor) {
    await withTx(this.pool, async tx => {
      await lockGroup(tx, groupId)
      await requireActiveAccount(tx, actor)
      if ((await roleIn(tx, groupId, actor)) !== Role.Owner) {
        throw new StoreError('Forbidden')
      }
      const { rows } = await tx.query('SELECT group_kind FROM groups WHERE group_id=$1', [groupId])
      if (!rows.length) {
        throw new StoreError('Database', new Error('no rows returned'))
      }
      if (rows[0].group_kind !== 'group') {
        throw new StoreError('Forbidden')
      }
      await tx.query('DELETE FROM groups WHERE group_id=$1', [groupId])
      await tx.commit()
    })
  }

  async groupMembers(groupId, actor) {
    return withTx(this.pool, async tx => {
      await lockGroup(tx, groupId)
      await requireActiveAccount(tx, actor)
      if ((await roleIn(tx, groupId, actor)) === null) {
        throw new StoreError('Forbidden')
      }
      const { rows } = await tx.query(
        'SELECT user_id,role FROM group_memberships WHERE group_id=$1 ORDER BY joined_at,user_id',
        [groupId]
      )
      const members = rows.map(row => ({ user_id: row.user_id, role: parseRole(row.role) }))
      await tx.commit()
      return members
    })
  }

  /** Read organization feature gates for an active organization device. */
  async organizationControls(organizationId, deviceId) {
    return withTx(this.pool, async tx => {
      const account = await tx.query(
        'SELECT a.account_kind FROM accounts a JOIN devices d ON d.user_id=a.user_id WHERE a.user_id=$1 AND d.device_id=$2 AND a.disabled_at IS NULL AND d.revoked_at IS NULL FOR SHARE',
        [organizationId, deviceId]
      )
      if (!account.rows.length || account.rows[0].account_kind !== 'organization') {
        throw new StoreError('Forbidden')
      }
      await tx.query(
        'INSERT INTO organization_controls (organization_id) VALUES ($1) ON CONFLICT (organization_id) DO NOTHING',
        [organizationId]
      )
      const { rows } = await tx.query(
        'SELECT mini_apps_enabled,bots_enabled,revision FROM organization_controls WHERE organization_id=$1',
        [organizationId]
      )
      if (!rows.length) {
        throw new StoreError('Database', new Error('no rows returned'))
      }
      if (rows[0].revision <= 0n) {
        throw new StoreError('CorruptObject')
      }
      const controls = {
        organization_id: organizationId,
        mini_apps_enabled: rows[0].mini_apps_enabled,
        bots_enabled: rows[0].bots_enabled,
        revision: rows[0].revision,
      }
      await tx.commit()
      return controls
    })
  }

  async setOrganizationControls(organizationId, deviceId, miniAppsEnabled, botsEnabled) {
    return withTx(this.pool, async tx => {
      const account = await tx.query(
        'SELECT a.account_kind,d.delegation_role FROM accounts a JOIN devices d ON d.user_id=a.user_id WHERE a.user_id=$1 AND d.device_id=$2 AND a.disabled_at IS NULL AND d.revoked_at IS NULL FOR UPDATE OF a,d',
        [organizationId, deviceId]
      )
      const row = account.rows[0]
      if (
        !row ||
        row.account_kind !== 'organization' ||
        !(row.delegation_role === 'owner' || row.delegation_role === 'admin')
      ) {
        throw new StoreError('Forbidden')
      }
      await tx.query(
        'INSERT INTO organization_controls (organization_id) VALUES ($1) ON CONFLICT (organization_id) DO NOTHING',
        [organizationId]
      )
      const { rows } = await tx.query(
        'UPDATE organization_controls SET mini_apps_enabled=$2,bots_enabled=$3,revision=revision+1,updated_at=now() WHERE organization_id=$1 RETURNING mini_apps_enabled,bots_enabled,revision',
        [organizationId, miniAppsEnabled, botsEnabled]
      )
      if (!rows.length) {
        throw new StoreError('Database', new Error('no rows returned'))
      }
      if (rows[0].revision <= 0n) {
        throw new StoreError('CorruptObject')
      }
      const controls = {
        organization_id: organizationId,
        mini_apps_enabled: rows[0].mini_apps_enabled,
        bots_enabled: rows[0].bots_enabled,
        revision: rows[0].revision,
      }
      await tx.commit()
      return controls
    })
  }

  // --- EncryptedPayloadStore -------------------------------------------------

  /**
   * Atomic append + contiguous per-device cursor allocation. Identical
   * (device, envelope_id, content) retries return the original cursor.
   * `envelope` must already be validated with validateEnqueue.
   */
  async append(envelope, acceptedAtMs) {
    const recipientDeviceId = parseUuidOrInvalid(envelope.recipient_device_id)
    const envelopeId = parseUuidOrInvalid(envelope.envelope_id)
    const envelopeBytes = encode('Envelope', envelope)
    const fingerprint = sha256(envelopeBytes)
    const acceptedAt = signedCursor(acceptedAtMs)
    const expiresAt = signedCursor(envelope.expires_at_ms)
    return withTx(this.pool, async tx => {
      await tx.query(
        'INSERT INTO encrypted_payload_cursors (recipient_device_id) VALUES ($1) ON CONFLICT DO NOTHING',
        [recipientDeviceId]
      )
      const watermark = await tx.query(
        'SELECT high_watermark FROM encrypted_payload_cursors WHERE recipient_device_id=$1 FOR UPDATE',
        [recipientDeviceId]
      )
      if (!watermark.rows.length) {
        throw new StoreError('Database', new Error('no rows returned'))
      }
      const highWatermark = watermark.rows[0].high_watermark
      const existing = await tx.query(
        'SELECT cursor,envelope_fingerprint FROM encrypted_payloads WHERE recipient_device_id=$1 AND envelope_id=$2 FOR UPDATE',
        [recipientDeviceId, envelopeId]
      )
      if (existing.rows.length) {
        if (!existing.rows[0].envelope_fingerprint.equals(fingerprint)) {
          throw new StoreError('Conflict')
        }
        const cursor = unsignedCursor(existing.rows[0].cursor)
        await tx.commit()
        return { cursor, duplicate: true }
      }
      if (highWatermark < 0n || highWatermark >= MAX_CURSOR) {
        throw new StoreError('Conflict')
      }
      const cursor = highWatermark + 1n
      await tx.query(
        "INSERT INTO encrypted_payloads (recipient_device_id,cursor,envelope_id,envelope_bytes,envelope_fingerprint,accepted_at_ms,expires_at_ms,state) VALUES ($1,$2,$3,$4,$5,$6,$7,'live')",
        [recipientDeviceId, cursor, envelopeId, envelopeBytes, fingerprint, acceptedAt, expiresAt]
      )
      await tx.query('UPDATE encrypted_payload_cursors SET high_watermark=$2 WHERE recipient_device_id=$1', [
        recipientDeviceId,
        cursor,
      ])
      await tx.commit()
      return { cursor: unsignedCursor(cursor), duplicate: false }
    })
  }

  /**
   * Strongly ordered replay. Expired/deleted entries become tombstones.
   * `request` must come from readRequest().
   */
  async read(request) {
    const deviceId = parseUuidOrInvalid(request.deviceId)
    const afterCursor = signedCursor(request.afterCursor)
    const nowMs = signedCursor(request.nowMs)
    return withTx(this.pool, async tx => {
      const watermark = await tx.query(
        'SELECT high_watermark FROM encrypted_payload_cursors WHERE recipient_device_id=$1',
        [deviceId]
      )
      const highWatermark = watermark.rows.length ? watermark.rows[0].high_watermark : 0n
      if (highWatermark < 0n || afterCursor > highWatermark) {
        throw new StoreError('Invalid')
      }
      if (afterCursor < highWatermark) {
        const next = await tx.query(
          'SELECT EXISTS(SELECT 1 FROM encrypted_payloads WHERE recipient_device_id=$1 AND cursor=$2)',
          [deviceId, afterCursor + 1n]
        )
        if (!next.rows[0].exists) {
          throw new StoreError('CursorExpired')
        }
      }
      const { rows } = await tx.query(
        'SELECT cursor,envelope_bytes,state,expires_at_ms FROM encrypted_payloads WHERE recipient_device_id=$1 AND cursor>$2 ORDER BY cursor LIMIT $3',
        [deviceId, afterCursor, BigInt(request.limit)]
      )
      const items = []
      let itemsLen = 0
      let expected = afterCursor + 1n
      for (const row of rows) {
        const cursor = row.cursor
        if (cursor !== expected) {
          throw new StoreError('CursorExpired')
        }
        expected += 1n
        const state = row.state
        let item
        if (state === 'acknowledged' || state === 'expired') {
          item = create('QueueItem', {
            cursor: unsignedCursor(cursor),
            tombstone: create('Tombstone', { reason: state === 'acknowledged' ? 2 : 1 }),
          })
        } else if (state === 'live' && row.expires_at_ms > nowMs) {
          let envelope
          try {
            envelope = decodeEnvelope(row.envelope_bytes ?? Buffer.alloc(0))
          } catch (error) {
            throw new StoreError('Protocol', error)
          }
          if (envelope.recipient_device_id !== request.deviceId) {
            throw new StoreError('Unavailable')
          }
          item = create('QueueItem', { cursor: unsignedCursor(cursor), envelope })
        } else if (state === 'live') {
          item = create('QueueItem', {
            cursor: unsignedCursor(cursor),
            tombstone: create('Tombstone', { reason: 1 }),
          })
        } else {
          throw new StoreError('Unavailable')
        }
        const header = create('SyncBatch', {
          recipient_device_id: request.deviceId,
          after_cursor: request.afterCursor,
          next_cursor: item.cursor,
          high_watermark: unsignedCursor(highWatermark),
        })
        const itemLen = queueItemFieldLen(item)
        const candidateLen = syncBatchEncodedLen(header) + itemsLen + itemLen
        if (candidateLen > MAX_FRAME_BYTES - 128) {
          if (!items.length) {
            throw new StoreError('Invalid')
          }
          break
        }
        items.push(item)
        itemsLen += itemLen
      }
      const nextCursor = items.length ? items[items.length - 1].cursor : request.afterCursor
      const batch = create('SyncBatch', {
        recipient_device_id: request.deviceId,
        after_cursor: request.afterCursor,
        next_cursor: nextCursor,
        high_watermark: unsignedCursor(highWatermark),
        items,
      })
      if (!batch.items.length && batch.next_cursor !== batch.high_watermark) {
        throw new StoreError('CursorExpired')
      }
      await tx.commit()
      return batch
    })
  }

  /** Idempotent cumulative delivery confirmation and purge. */
  async acknowledge(deviceIdText, throughCursor, nowMs) {
    const deviceId = parseUuidOrInvalid(deviceIdText)
    const through = signedCursor(throughCursor)
    const now = signedCursor(nowMs)
    await withTx(this.pool, async tx => {
      const watermark = await tx.query(
        'SELECT high_watermark FROM encrypted_payload_cursors WHERE recipient_device_id=$1 FOR UPDATE',
        [deviceId]
      )
      const highWatermark = watermark.rows.length ? watermark.rows[0].high_watermark : 0n
      if (through > highWatermark) {
        throw new StoreError('Invalid')
      }
      await tx.query(
        "UPDATE encrypted_payloads SET state=CASE WHEN expires_at_ms <= $3 THEN 'expired' ELSE 'acknowledged' END, envelope_bytes=NULL WHERE recipient_device_id=$1 AND cursor <= $2 AND state='live'",
        [deviceId, through, now]
      )
      await tx.query('DELETE FROM device_mls_bootstraps WHERE recipient_device_id=$1', [deviceId])
      await tx.commit()
    })
  }

  async putPendingMlsBootstrap(recipientDeviceId, bootstrap) {
    if (bootstrap.recipient_device_id !== recipientDeviceId) {
      throw new StoreError('Invalid')
    }
    const recipient = parseUuidOrInvalid(recipientDeviceId)
    const conversation = parseUuidOrInvalid(bootstrap.conversation_id)
    const bytes = encode('MlsBootstrap', bootstrap)
    if (bytes.length === 0 || bytes.length > 2 * MAX_FRAME_BYTES) {
      throw new StoreError('Invalid')
    }
    await this.pool.query(
      'INSERT INTO device_mls_bootstraps (recipient_device_id,conversation_id,bootstrap) VALUES ($1,$2,$3) ON CONFLICT (recipient_device_id,conversation_id) DO UPDATE SET bootstrap=EXCLUDED.bootstrap, updated_at=now()',
      [recipient, conversation, bytes]
    )
  }

  async pendingMlsBootstraps(recipientDeviceId) {
    const recipient = parseUuidOrInvalid(recipientDeviceId)
    const { rows } = await this.pool.query(
      'SELECT bootstrap FROM device_mls_bootstraps WHERE recipient_device_id=$1 ORDER BY updated_at',
      [recipient]
    )
    return rows.map(row => {
      try {
        return decode('MlsBootstrap', row.bootstrap)
      } catch {
        throw new StoreError('Invalid')
      }
    })
  }

  /** Enforce the <=30 day payload lifetime. */
  async purgeExpired(nowMs, limit) {
    if (limit === 0 || limit > MAX_PURGE_BATCH) {
      throw new StoreError('Invalid')
    }
    const now = signedCursor(nowMs)
    const cutoff = now - MAX_RETENTION_MS
    return withTx(this.pool, async tx => {
      const expired = (
        await tx.query(
          "WITH candidates AS (SELECT recipient_device_id,cursor FROM encrypted_payloads WHERE state='live' AND expires_at_ms <= $1 ORDER BY expires_at_ms,cursor LIMIT $2 FOR UPDATE SKIP LOCKED) UPDATE encrypted_payloads p SET state='expired',envelope_bytes=NULL FROM candidates c WHERE p.recipient_device_id=c.recipient_device_id AND p.cursor=c.cursor",
          [now, BigInt(limit)]
        )
      ).rowCount
      const remaining = Math.max(0, limit - expired)
      let deleted = 0
      if (remaining > 0) {
        deleted = (
          await tx.query(
            "WITH candidates AS (SELECT recipient_device_id,cursor FROM encrypted_payloads WHERE state <> 'live' AND accepted_at_ms <= $1 ORDER BY accepted_at_ms,cursor LIMIT $2 FOR UPDATE SKIP LOCKED) DELETE FROM encrypted_payloads p USING candidates c WHERE p.recipient_device_id=c.recipient_device_id AND p.cursor=c.cursor",
            [cutoff, BigInt(remaining)]
          )
        ).rowCount
      }
      await tx.commit()
      return expired + deleted
    })
  }
}

for (const name of Object.getOwnPropertyNames(RelationalStore.prototype)) {
  if (name !== 'constructor') {
    RelationalStore.prototype[name] = storeCall(RelationalStore.prototype, name)
  }
}

/** ReadRequest::new validation. */
export function readRequest(deviceId, afterCursor, limit, nowMs) {
  if (!/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/.test(deviceId) || deviceId === '00000000-0000-0000-0000-000000000000') {
    throw new StoreError('Protocol', new ProtocolError('Invalid', 'id'))
  }
  if (afterCursor > MAX_CURSOR || limit === 0 || limit > 100) {
    throw new StoreError('Invalid')
  }
  return { deviceId, afterCursor, limit, nowMs }
}
