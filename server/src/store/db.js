// PostgreSQL pool and explicit transactions with sqlx-like semantics: a
// transaction that is released without commit() is rolled back.
import pg from 'pg'

const INT8_OID = 20

const types = {
  getTypeParser(oid, format) {
    if (oid === INT8_OID && format !== 'binary') {
      return value => BigInt(value)
    }
    return pg.types.getTypeParser(oid, format)
  },
}

export function createPool(connectionString) {
  const pool = new pg.Pool({ connectionString, max: 10, types })
  // Idle clients can fail when the database restarts. Queries on a fresh
  // client surface the error to the caller; the pool itself must not crash.
  pool.on('error', () => {})
  return pool
}

export class Transaction {
  constructor(client) {
    this.client = client
    this.done = false
  }

  query(text, values) {
    return this.client.query(text, values)
  }

  async commit() {
    await this.client.query('COMMIT')
    this.done = true
  }

  async release() {
    if (this.done) {
      this.client.release()
      return
    }
    this.done = true
    try {
      await this.client.query('ROLLBACK')
      this.client.release()
    } catch (error) {
      this.client.release(error)
    }
  }
}

export async function begin(pool) {
  const client = await pool.connect()
  try {
    await client.query('BEGIN')
  } catch (error) {
    client.release(error)
    throw error
  }
  return new Transaction(client)
}

/** Run `work(tx)` in a transaction that rolls back unless `work` commits. */
export async function transaction(pool, work) {
  const tx = await begin(pool)
  try {
    return await work(tx)
  } finally {
    await tx.release()
  }
}
