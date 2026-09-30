// Apply SQL migrations with the exact bookkeeping used by
// `sqlx::migrate!`: the same `_sqlx_migrations` table, SHA-384 checksums,
// advisory lock ID, and dirty/missing/modified checks. The Rust and Node
// servers can therefore run against the same database interchangeably.
import { createHash } from 'node:crypto'
import { readdirSync, readFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { crc32 } from 'node:zlib'

import { StoreError } from '../errors.js'

const repoRoot = join(dirname(fileURLToPath(import.meta.url)), '..', '..', '..')
export const MIGRATIONS_DIR = join(repoRoot, 'SQL')

export function loadMigrations(directory = MIGRATIONS_DIR) {
  return readdirSync(directory)
    .filter(name => name.endsWith('.sql'))
    .map(name => {
      const match = /^(\d+)_(.+)\.sql$/.exec(name)
      if (!match || match[2].endsWith('.up') || match[2].endsWith('.down')) {
        throw new StoreError('Migration', new Error(`unsupported migration file name: ${name}`))
      }
      const sql = readFileSync(join(directory, name), 'utf8')
      return {
        version: BigInt(match[1]),
        description: match[2].replaceAll('_', ' '),
        sql,
        checksum: createHash('sha384').update(sql, 'utf8').digest(),
        noTransaction: sql.startsWith('-- no-transaction'),
      }
    })
    .sort((left, right) => (left.version < right.version ? -1 : left.version > right.version ? 1 : 0))
}

function lockId(databaseName) {
  return 0x3d32ad9en * BigInt(crc32(Buffer.from(databaseName, 'utf8')))
}

function migrationError(message) {
  return new StoreError('Migration', new Error(message))
}

export async function migrate(pool, migrations = loadMigrations()) {
  const client = await pool.connect()
  let lock = null
  try {
    const { rows } = await client.query('SELECT current_database()')
    lock = lockId(rows[0].current_database)
    await client.query('SELECT pg_advisory_lock($1)', [lock])
    await client.query(`CREATE TABLE IF NOT EXISTS _sqlx_migrations (
    version BIGINT PRIMARY KEY,
    description TEXT NOT NULL,
    installed_on TIMESTAMPTZ NOT NULL DEFAULT now(),
    success BOOLEAN NOT NULL,
    checksum BYTEA NOT NULL,
    execution_time BIGINT NOT NULL
);`)
    const dirty = await client.query(
      'SELECT version FROM _sqlx_migrations WHERE success = false ORDER BY version LIMIT 1'
    )
    if (dirty.rows.length) {
      throw migrationError(`migration ${dirty.rows[0].version} is partially applied; fix and remove row from \`_sqlx_migrations\` table`)
    }
    const applied = await client.query('SELECT version, checksum FROM _sqlx_migrations ORDER BY version')
    const appliedByVersion = new Map(applied.rows.map(row => [BigInt(row.version), row.checksum]))
    const known = new Set(migrations.map(migration => migration.version))
    for (const version of appliedByVersion.keys()) {
      if (!known.has(version)) {
        throw migrationError(`migration ${version} was previously applied but is missing in the resolved migrations`)
      }
    }
    for (const migration of migrations) {
      const checksum = appliedByVersion.get(migration.version)
      if (checksum) {
        if (!Buffer.from(checksum).equals(migration.checksum)) {
          throw migrationError(`migration ${migration.version} was previously applied but has been modified`)
        }
        continue
      }
      const started = process.hrtime.bigint()
      if (migration.noTransaction) {
        await client.query(migration.sql)
        await client.query(
          'INSERT INTO _sqlx_migrations ( version, description, success, checksum, execution_time ) VALUES ( $1, $2, TRUE, $3, -1 )',
          [migration.version, migration.description, migration.checksum]
        )
      } else {
        await client.query('BEGIN')
        try {
          await client.query(migration.sql)
          await client.query(
            'INSERT INTO _sqlx_migrations ( version, description, success, checksum, execution_time ) VALUES ( $1, $2, TRUE, $3, -1 )',
            [migration.version, migration.description, migration.checksum]
          )
          await client.query('COMMIT')
        } catch (error) {
          await client.query('ROLLBACK').catch(() => {})
          throw error
        }
      }
      const elapsed = process.hrtime.bigint() - started
      await client.query('UPDATE _sqlx_migrations SET execution_time = $1 WHERE version = $2', [
        elapsed,
        migration.version,
      ])
    }
  } catch (error) {
    throw error instanceof StoreError ? error : new StoreError('Migration', error)
  } finally {
    if (lock !== null) {
      await client.query('SELECT pg_advisory_unlock($1)', [lock]).catch(() => {})
    }
    client.release()
  }
}
