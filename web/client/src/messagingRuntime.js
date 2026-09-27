import { WebTextMessaging } from '../../src/WebTextMessaging.ts'
import { WebWasmMessagingCore } from '../../src/WebWasmMessaging.ts'
import { WebFileSession } from '../../src/WebFiles.ts'

let wasmModulePromise

function decodeBase64URL(value) {
  const normalized = String(value || '').replace(/-/g, '+').replace(/_/g, '/')
  const padded = normalized + '='.repeat((4 - normalized.length % 4) % 4)
  const binary = atob(padded)
  return Uint8Array.from(binary, character => character.charCodeAt(0))
}

function websocketEndpoint() {
  if (typeof __LINKS_GATEWAY_ENDPOINT__ === 'string' && __LINKS_GATEWAY_ENDPOINT__) {
    return __LINKS_GATEWAY_ENDPOINT__
  }
  const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:'
  return `${protocol}//${window.location.host}/v1/connect`
}

async function loadWasmModule() {
  // Keep the generated Rust/WASM asset outside Vite's module graph so the
  // browser can load it as a normal public resource. Do not reuse the first
  // import: a later session would keep an older core.
  const wasmURL = new URL('/web-wasm/links_web_client.js', window.location.origin)
  wasmURL.searchParams.set('v', String(Date.now()))
  const pending = import(/* @vite-ignore */ wasmURL.href)
    .then(async module => {
      if (typeof module.default === 'function') {
        const binaryURL = new URL('/web-wasm/links_web_client_bg.wasm', window.location.origin)
        binaryURL.searchParams.set('v', wasmURL.searchParams.get('v'))
        await module.default(binaryURL)
      }
      return module
    })
  wasmModulePromise = pending
  try {
    return await pending
  } catch (error) {
    if (wasmModulePromise === pending) wasmModulePromise = null
    throw error
  }
}

/** Create the real Rust/WASM core plus reconnecting binary WebSocket host. */
export async function createWebMessagingSession({
  userID,
  deviceID,
  mlsCredential,
  identitySeed,
  contacts = [],
  accessToken,
  onState,
  onTextMessage,
  onFailure
}) {
  const wasm = await loadWasmModule()
  if (typeof wasm.WebMessagingCore !== 'function' ||
      typeof wasm.WebMessagingCore.from_identity_seed !== 'function') {
    throw new Error('Web messaging WASM is unavailable')
  }
  let rustCore
  try {
    rustCore = wasm.WebMessagingCore.from_identity_seed(
      userID,
      deviceID,
      decodeBase64URL(mlsCredential),
      new Uint8Array(identitySeed || [])
    )
  } catch (error) {
    throw new Error(`WASM identity: ${error instanceof Error ? error.message : String(error || '')}`)
  }
  const core = new WebWasmMessagingCore(rustCore)
  const files = new WebFileSession(wasm)
  const token = () => accessToken()
  try {
    await publishLocalKeys(rustCore, token)
  } catch (error) {
    throw new Error(`Browser keys: ${error instanceof Error ? error.message : String(error || '')}`)
  }
  const loadedRecipientIDs = await loadRecipients(core, contacts, token, deviceID)
  const session = new WebTextMessaging({
    endpoint: websocketEndpoint(),
    core,
    accessToken,
    onState,
    onTextMessage,
    onFailure
  })
  return {
    core,
    start: () => session.start(),
    stop: () => session.stop(),
    shutdown: () => session.shutdown(),
    waitUntilConnected: timeoutMs => session.waitUntilConnected(timeoutMs),
    sendText: (conversationID, recipientUserID, text) => session.sendText(conversationID, recipientUserID, text),
    sendFile: async (conversationID, recipientUserID, file) => {
      const encrypted = await files.encrypt(file.blob, file.name, file.mimeType)
      try {
        const receipt = await files.upload(accessToken(), encrypted)
        session.sendFile(conversationID, recipientUserID, encrypted.metadata, receipt)
      } finally {
        encrypted.ciphertext.fill(0)
      }
    },
    refreshRecipient: async recipientUserID => {
      // Recipient device keys are one-time/prekey material. A browser session
      // can outlive a recipient reconnect, so the cached entry may no longer
      // decrypt on the other device. Re-read and claim fresh keys for every
      // send instead of trusting the initial session snapshot.
      await loadRecipient(core, { userID: recipientUserID }, token, deviceID)
      loadedRecipientIDs.add(recipientUserID)
    },
    get state() { return session.state },
    get isConnected() { return session.isConnected }
  }
}

async function publishLocalKeys(rustCore, accessToken) {
  const headers = {
    Authorization: `Bearer ${accessToken()}`,
    'Content-Type': 'application/x-protobuf'
  }
  const profile = await fetch('/links-api/v1/prekeys', {
    method: 'PUT',
    headers,
    // Keep a small one-time pool so another device can start a session with
    // this browser without waiting for a refill request.
    body: rustCore.prekey_upload(16, 16),
    cache: 'no-store'
  })
  if (!profile.ok) throw new Error(`Could not publish browser pre-keys (${profile.status})`)
  const keyPackage = await fetch('/links-api/v1/mls/key-package', {
    method: 'PUT',
    headers,
    body: rustCore.key_package(),
    cache: 'no-store'
  })
  if (!keyPackage.ok) throw new Error(`Could not publish browser MLS key package (${keyPackage.status})`)
}

async function loadRecipients(core, contacts, accessToken, localDeviceID) {
  const loadedRecipientIDs = new Set()
  for (const contact of contacts) {
    if (!contact?.userID) continue
    try {
      await loadRecipient(core, contact, accessToken, localDeviceID)
      loadedRecipientIDs.add(contact.userID)
    } catch {
      // A contact may not have a usable pre-key package yet.
    }
  }
  return loadedRecipientIDs
}

function httpFailure(message, status) {
  const error = new Error(`${message} (${status})`)
  error.status = status
  return error
}

async function loadRecipient(core, contact, accessToken, localDeviceID) {
  const headers = { Authorization: `Bearer ${accessToken()}`, Accept: 'application/json' }
  const directoryResponse = await fetch(`/links-api/v1/directory/users/${encodeURIComponent(contact.userID)}`, {
    headers,
    cache: 'no-store'
  })
  if (!directoryResponse.ok) throw httpFailure('Recipient directory lookup failed', directoryResponse.status)
  const directory = await directoryResponse.json()
  let installed = 0
  for (const device of directory.devices || []) {
    if (!device?.device_id || device.device_id === localDeviceID) continue
    const [prekeyResponse, keyPackageResponse] = await Promise.all([
      fetch(`/links-api/v1/prekeys/${encodeURIComponent(device.device_id)}/claim`, {
        method: 'POST', headers: { ...headers, Accept: 'application/octet-stream' }, cache: 'no-store'
      }),
      fetch(`/links-api/v1/mls/key-package/${encodeURIComponent(device.device_id)}`, {
        headers: { Authorization: headers.Authorization, Accept: 'application/octet-stream' }, cache: 'no-store'
      })
    ])
    if (prekeyResponse.status === 401 || keyPackageResponse.status === 401) {
      throw httpFailure('Recipient key lookup failed', 401)
    }
    if (!prekeyResponse.ok || !keyPackageResponse.ok) continue
    core.setRecipient(
      directory.user_id,
      device.device_id,
      decodeBase64URL(device.identity_public_key),
      new Uint8Array(await prekeyResponse.arrayBuffer()),
      decodeBase64URL(device.mls_credential),
      new Uint8Array(await keyPackageResponse.arrayBuffer())
    )
    installed += 1
  }
  if (installed === 0) throw new Error('Recipient has no usable device keys')
}
