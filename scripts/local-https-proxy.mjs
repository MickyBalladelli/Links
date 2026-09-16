#!/usr/bin/env node

import { execFileSync } from 'node:child_process'
import { request as httpRequest } from 'node:http'
import { createServer as createHttpsServer } from 'node:https'
import { networkInterfaces } from 'node:os'
import { dirname, join } from 'node:path'
import { existsSync, mkdirSync, chmodSync, readFileSync, unlinkSync, writeFileSync } from 'node:fs'
import { connect as tcpConnect } from 'node:net'
import { fileURLToPath } from 'node:url'

const repoRoot = dirname(dirname(fileURLToPath(import.meta.url)))
const listenPort = Number.parseInt(process.env.LINKS_HTTPS_PORT || '8443', 10)
const authHost = process.env.LINKS_AUTH_UPSTREAM_HOST || '127.0.0.1'
const authPort = Number.parseInt(process.env.LINKS_AUTH_UPSTREAM_PORT || '8080', 10)
const gatewayHost = process.env.LINKS_GATEWAY_UPSTREAM_HOST || '127.0.0.1'
const gatewayPort = Number.parseInt(process.env.LINKS_GATEWAY_UPSTREAM_PORT || '8081', 10)
const lanIp = process.env.LINKS_LAN_IP || findLanIPv4()
const listenHost = process.env.LINKS_HTTPS_BIND || lanIp
const tlsDirectory = process.env.LINKS_TLS_DIR || join(repoRoot, 'native/ios/LocalHTTPS')
const customCertificatePath = process.env.LINKS_TLS_CERT_FILE || ''
const customKeyPath = process.env.LINKS_TLS_KEY_FILE || ''
const usingCustomCertificate = Boolean(customCertificatePath || customKeyPath)
const certificatePath = customCertificatePath || join(tlsDirectory, `server-cert-${fileSafe(lanIp)}.pem`)
const keyPath = customKeyPath || join(tlsDirectory, `server-key-${fileSafe(lanIp)}.pem`)
const rootCertificatePath = join(tlsDirectory, `root-cert-${fileSafe(lanIp)}.pem`)
const rootKeyPath = join(tlsDirectory, `root-key-${fileSafe(lanIp)}.pem`)
const mobileCertificatePath = process.env.LINKS_TLS_CERT_DER_FILE || join(
  tlsDirectory,
  `${usingCustomCertificate ? 'server-cert' : 'root-cert'}-${fileSafe(lanIp)}.cer`
)

if (!Number.isInteger(listenPort) || listenPort < 1 || listenPort > 65535) {
  fail('LINKS_HTTPS_PORT must be a valid TCP port')
}

if (!Number.isInteger(authPort) || authPort < 1 || authPort > 65535) {
  fail('LINKS_AUTH_UPSTREAM_PORT must be a valid TCP port')
}

if (!Number.isInteger(gatewayPort) || gatewayPort < 1 || gatewayPort > 65535) {
  fail('LINKS_GATEWAY_UPSTREAM_PORT must be a valid TCP port')
}

if (!lanIp) {
  fail('Could not determine a LAN IPv4 address. Set LINKS_LAN_IP explicitly.')
}

ensureCertificate()

const server = createHttpsServer(
  {
    key: readFileSync(keyPath),
    cert: readFileSync(certificatePath)
  },
  proxyHttpRequest
)

server.on('upgrade', proxyWebSocket)
server.on('clientError', (_error, socket) => {
  socket.destroy()
})

server.on('error', error => {
  if (error.code === 'EADDRINUSE') {
    fail(`HTTPS port ${listenPort} is already in use`)
  }
  fail('HTTPS proxy stopped because the listener failed')
})

server.listen(listenPort, listenHost, () => {
  console.log(`Links HTTPS proxy listening on https://${lanIp}:${listenPort}`)
  console.log(`Auth upstream: http://${authHost}:${authPort}`)
  console.log(`Gateway upstream: ws://${gatewayHost}:${gatewayPort}/v1/connect`)
  console.log(`TLS certificate: ${certificatePath}`)
  console.log(`iPhone trust certificate: ${mobileCertificatePath}`)
  console.log('Install and trust this certificate on the iPhone before using OTP.')
})

for (const signal of ['SIGINT', 'SIGTERM']) {
  process.once(signal, () => {
    server.close(() => process.exit(0))
    setTimeout(() => process.exit(0), 1000).unref()
  })
}

function proxyHttpRequest(request, response) {
  const requestPath = pathname(request.url)

  if (requestPath === '/healthz') {
    checkAuthUpstream(response)
    return
  }

  const headers = { ...request.headers }
  headers.host = `${authHost}:${authPort}`
  delete headers.connection
  delete headers['keep-alive']
  delete headers['proxy-connection']
  delete headers.te
  delete headers.trailer
  delete headers.upgrade

  const upstream = httpRequest(
    {
      hostname: authHost,
      port: authPort,
      method: request.method,
      path: request.url || '/',
      headers,
      timeout: 15000
    },
    upstreamResponse => {
      const responseHeaders = { ...upstreamResponse.headers }
      delete responseHeaders.connection
      delete responseHeaders['keep-alive']
      delete responseHeaders['proxy-authenticate']
      delete responseHeaders['proxy-authorization']
      delete responseHeaders.te
      delete responseHeaders.trailer
      delete responseHeaders.transfer-encoding
      delete responseHeaders.upgrade
      response.writeHead(upstreamResponse.statusCode || 502, responseHeaders)
      upstreamResponse.pipe(response)
      upstreamResponse.once('end', () => logRequest(request.method, requestPath, upstreamResponse.statusCode || 502))
    }
  )

  upstream.on('timeout', () => upstream.destroy(new Error('auth upstream timeout')))
  upstream.on('error', () => {
    if (!response.headersSent) {
      response.writeHead(502, { 'content-type': 'application/json', 'cache-control': 'no-store' })
    }
    if (!response.writableEnded) {
      response.end('{"error":"auth upstream unavailable"}\n')
    }
    logRequest(request.method, requestPath, 502)
  })
  request.pipe(upstream)
}

function checkAuthUpstream(response) {
  let completed = false
  const finish = (status, body) => {
    if (completed || response.writableEnded) {
      return
    }
    completed = true
    response.writeHead(status, { 'content-type': 'application/json', 'cache-control': 'no-store' })
    response.end(body)
  }
  const socket = tcpConnect(authPort, authHost)
  socket.setTimeout(1000, () => {
    socket.destroy()
    finish(503, '{"status":"degraded","auth":"unavailable"}\n')
  })
  socket.once('connect', () => {
    socket.end()
    finish(200, '{"status":"ok"}\n')
  })
  socket.once('error', () => {
    finish(503, '{"status":"degraded","auth":"unavailable"}\n')
  })
}

function proxyWebSocket(request, clientSocket, head) {
  if (pathname(request.url) !== '/v1/connect') {
    clientSocket.write('HTTP/1.1 404 Not Found\r\nConnection: close\r\n\r\n')
    clientSocket.destroy()
    return
  }

  const upstreamSocket = tcpConnect(gatewayPort, gatewayHost)
  let handedOff = false

  upstreamSocket.once('connect', () => {
    const headers = { ...request.headers }
    headers.host = `${gatewayHost}:${gatewayPort}`
    headers.connection = 'Upgrade'
    headers.upgrade = 'websocket'

    const headerLines = Object.entries(headers)
      .filter(([, value]) => value !== undefined)
      .flatMap(([name, value]) => {
        const values = Array.isArray(value) ? value : [value]
        return values.map(item => `${name}: ${item}`)
      })

    const rawRequest = [
      `${request.method || 'GET'} ${request.url || '/'} HTTP/1.1`,
      ...headerLines,
      '',
      ''
    ].join('\r\n')

    upstreamSocket.write(rawRequest)
    if (head.length > 0) {
      upstreamSocket.write(head)
    }
    clientSocket.pipe(upstreamSocket)
    upstreamSocket.pipe(clientSocket)
    handedOff = true
  })

  upstreamSocket.once('error', () => {
    if (!handedOff && !clientSocket.destroyed) {
      clientSocket.write('HTTP/1.1 502 Bad Gateway\r\nConnection: close\r\n\r\n')
    }
    clientSocket.destroy()
  })

  clientSocket.once('error', () => upstreamSocket.destroy())
  clientSocket.once('close', () => upstreamSocket.destroy())
}

function ensureCertificate() {
  const requiredPaths = usingCustomCertificate
    ? [certificatePath, keyPath]
    : [certificatePath, keyPath, rootCertificatePath, rootKeyPath]
  const existingCount = requiredPaths.filter(existsSync).length

  if (existingCount === requiredPaths.length && process.env.LINKS_TLS_REGENERATE !== '1') {
    ensureMobileCertificate()
    return
  }

  if (usingCustomCertificate) {
    fail('LINKS_TLS_CERT_FILE and LINKS_TLS_KEY_FILE must both point to existing files')
  }

  if (existingCount > 0 && process.env.LINKS_TLS_REGENERATE !== '1') {
    fail('TLS files are incomplete; remove the matching local HTTPS files or set LINKS_TLS_REGENERATE=1')
  }

  mkdirSync(dirname(certificatePath), { recursive: true, mode: 0o700 })
  mkdirSync(dirname(rootKeyPath), { recursive: true, mode: 0o700 })
  const rootConfigPath = join(tlsDirectory, `openssl-root-${fileSafe(lanIp)}.cnf`)
  const serverConfigPath = join(tlsDirectory, `openssl-server-${fileSafe(lanIp)}.cnf`)
  const csrPath = join(tlsDirectory, `server-${fileSafe(lanIp)}.csr`)
  const serialPath = `${rootCertificatePath}.srl`
  const rootConfig = `[req]\ndistinguished_name = req_distinguished_name\nx509_extensions = v3_ca\nprompt = no\n\n[req_distinguished_name]\nCN = Links local development CA\n\n[v3_ca]\nsubjectKeyIdentifier = hash\nauthorityKeyIdentifier = keyid:always,issuer\nbasicConstraints = critical,CA:true,pathlen:1\nkeyUsage = critical,keyCertSign,cRLSign\n`
  const serverConfig = `[req]\ndistinguished_name = req_distinguished_name\nprompt = no\n\n[req_distinguished_name]\nCN = links-mac.local\n\n[v3_server]\nsubjectAltName = @alt_names\nbasicConstraints = critical,CA:false\nkeyUsage = critical,digitalSignature,keyEncipherment\nextendedKeyUsage = serverAuth\n\n[alt_names]\nIP.1 = ${lanIp}\nDNS.1 = links-mac.local\n`

  writeFileSync(rootConfigPath, rootConfig, { mode: 0o600 })
  writeFileSync(serverConfigPath, serverConfig, { mode: 0o600 })
  try {
    execFileSync(
      process.env.OPENSSL_BIN || 'openssl',
      [
        'req',
        '-x509',
        '-newkey',
        'rsa:2048',
        '-nodes',
        '-keyout',
        rootKeyPath,
        '-out',
        rootCertificatePath,
        '-days',
        '3650',
        '-config',
        rootConfigPath,
        '-extensions',
        'v3_ca'
      ],
      { stdio: 'ignore' }
    )
    execFileSync(
      process.env.OPENSSL_BIN || 'openssl',
      [
        'req',
        '-new',
        '-newkey',
        'rsa:2048',
        '-nodes',
        '-keyout',
        keyPath,
        '-out',
        csrPath,
        '-config',
        serverConfigPath
      ],
      { stdio: 'ignore' }
    )
    execFileSync(
      process.env.OPENSSL_BIN || 'openssl',
      [
        'x509',
        '-req',
        '-in',
        csrPath,
        '-CA',
        rootCertificatePath,
        '-CAkey',
        rootKeyPath,
        '-CAcreateserial',
        '-out',
        certificatePath,
        '-days',
        '365',
        '-sha256',
        '-extfile',
        serverConfigPath,
        '-extensions',
        'v3_server'
      ],
      { stdio: 'ignore' }
    )
  } catch (_error) {
    fail('Could not create local TLS certificates. Install openssl or set LINKS_TLS_CERT_FILE and LINKS_TLS_KEY_FILE.')
  } finally {
    for (const temporaryPath of [rootConfigPath, serverConfigPath, csrPath, serialPath]) {
      try {
        unlinkSync(temporaryPath)
      } catch (_error) {
        // The certificates remain usable if cleanup is interrupted.
      }
    }
  }

  chmodSync(rootKeyPath, 0o600)
  chmodSync(keyPath, 0o600)
  chmodSync(rootCertificatePath, 0o644)
  chmodSync(certificatePath, 0o644)
  ensureMobileCertificate()
}

function ensureMobileCertificate() {
  if (existsSync(mobileCertificatePath)) {
    return
  }

  try {
    execFileSync(
      process.env.OPENSSL_BIN || 'openssl',
      [
        'x509',
        '-in',
        usingCustomCertificate ? certificatePath : rootCertificatePath,
        '-outform',
        'der',
        '-out',
        mobileCertificatePath
      ],
      { stdio: 'ignore' }
    )
    chmodSync(mobileCertificatePath, 0o644)
  } catch (_error) {
    fail('Could not create the iPhone certificate file. The HTTPS server certificate was created, but iPhone installation needs openssl.')
  }
}

function findLanIPv4() {
  for (const addresses of Object.values(networkInterfaces())) {
    for (const address of addresses || []) {
      if (address.family === 'IPv4' && !address.internal && address.address) {
        return address.address
      }
    }
  }
  return ''
}

function fileSafe(value) {
  return String(value || 'unknown').replace(/[^a-zA-Z0-9.-]/g, '_')
}

function pathname(value) {
  if (!value) {
    return '/'
  }
  try {
    return new URL(value, 'https://links.local').pathname
  } catch (_error) {
    return '/'
  }
}

function logRequest(method, requestPath, status) {
  console.log(`${method || 'GET'} ${requestPath} -> ${status}`)
}

function fail(message) {
  console.error(`Links HTTPS proxy: ${message}`)
  process.exit(1)
}
