// Local Links composition: PostgreSQL, username auth, mailbox storage,
// ephemeral sessions, and the loopback WebSocket gateway.
//
// Development only. Username auth disables phone OTP, so this server refuses
// to start unless AUTH_DEV_USERNAME_MODE=1 and refuses non-loopback listeners.
import { createServer } from 'node:http'
import { isIP } from 'node:net'

import { AccountAuth } from './auth/service.js'
import { decodeBase64Url } from './auth/encoding.js'
import { createAuthApp } from './auth/web.js'
import { AccountAuthDeviceAuthenticator, Gateway, gatewayConfig } from './gateway/gateway.js'
import { WEBSOCKET_PATH, WebSocketAdapter } from './gateway/websocket.js'
import { createPool } from './store/db.js'
import { MemoryEphemeralState } from './store/ephemeral.js'
import { migrate } from './store/migrate.js'
import { RelationalStore } from './store/relational.js'

class StartupError extends Error {}

function lookupKey() {
  const encoded = process.env.AUTH_LOOKUP_KEY
  if (encoded === undefined) {
    throw new StartupError('AUTH_LOOKUP_KEY must be a 32-byte base64url secret')
  }
  const decoded = decodeBase64Url(encoded)
  if (!decoded) {
    throw new StartupError('AUTH_LOOKUP_KEY must be base64url without padding')
  }
  if (decoded.length !== 32) {
    throw new StartupError('AUTH_LOOKUP_KEY must decode to exactly 32 bytes')
  }
  return decoded
}

/** Parse `ip:port` or `[ipv6]:port` like Rust `SocketAddr` and require loopback. */
function loopbackAddress(name, fallback) {
  const value = process.env[name] ?? fallback
  const match = /^\[([^\]]+)\]:(\d+)$/.exec(value) ?? /^([^:[\]]+):(\d+)$/.exec(value)
  const host = match?.[1]
  const port = match ? Number(match[2]) : NaN
  if (!match || !isIP(host) || !(port <= 65535)) {
    throw new StartupError('invalid socket address syntax')
  }
  const loopback = isIP(host) === 4 ? host.split('.')[0] === '127' : host === '::1'
  if (!loopback) {
    throw new StartupError(`${name} must bind to loopback`)
  }
  return { host, port, display: isIP(host) === 6 ? `[${host}]:${port}` : `${host}:${port}` }
}

function listen(server, address) {
  return new Promise((resolve, reject) => {
    server.once('error', reject)
    server.listen(address.port, address.host, () => {
      server.off('error', reject)
      resolve()
    })
  })
}

async function main() {
  if (process.env.AUTH_DEV_USERNAME_MODE !== '1') {
    throw new StartupError('set AUTH_DEV_USERNAME_MODE=1 for the loopback composition')
  }
  const databaseUrl = process.env.DATABASE_URL
  if (databaseUrl === undefined) {
    throw new StartupError('environment variable not found: DATABASE_URL')
  }
  const pool = createPool(databaseUrl)
  await migrate(pool)
  const auth = new AccountAuth(pool, lookupKey(), {
    mode: 'loopback-username-dev',
    adminKey: process.env.LINKS_ADMIN_KEY,
  })

  const authAddress = loopbackAddress('AUTH_BIND', '127.0.0.1:8080')
  const gatewayAddress = loopbackAddress('GATEWAY_BIND', '127.0.0.1:8081')
  const config = gatewayConfig(process.env.GATEWAY_ID ?? 'local-gateway', process.env.GATEWAY_REGION ?? 'local')
  const gateway = new Gateway(
    config,
    new MemoryEphemeralState(1_024),
    new RelationalStore(pool),
    new AccountAuthDeviceAuthenticator(auth)
  )
  const adapter = new WebSocketAdapter(gateway)

  const authHttp = createServer(createAuthApp(auth))
  const gatewayHttp = adapter.createServer()
  await listen(authHttp, authAddress)
  await listen(gatewayHttp, gatewayAddress)
  console.log(`Links local auth: http://${authAddress.display}`)
  console.log(`Links local gateway: ws://${gatewayAddress.display}${WEBSOCKET_PATH}`)

  const shutdown = () => {
    authHttp.close()
    gatewayHttp.close()
    for (const client of adapter.wss.clients) {
      client.terminate()
    }
    authHttp.closeAllConnections?.()
    gatewayHttp.closeAllConnections?.()
    pool.end().finally(() => process.exit(0))
  }
  process.once('SIGINT', shutdown)
  process.once('SIGTERM', shutdown)
}

main().catch(error => {
  const message = error instanceof StartupError ? error.message : error?.cause?.message ?? error?.message ?? String(error)
  console.error(`Error: ${message}`)
  process.exit(1)
})
