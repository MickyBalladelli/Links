import { WebTextMessaging } from '../../src/WebTextMessaging.ts'
import { WebWasmMessagingCore } from '../../src/WebWasmMessaging.ts'

let wasmModulePromise

function decodeBase64URL(value) {
  const normalized = String(value || '').replace(/-/g, '+').replace(/_/g, '/')
  const padded = normalized + '='.repeat((4 - normalized.length % 4) % 4)
  const binary = atob(padded)
  return Uint8Array.from(binary, character => character.charCodeAt(0))
}

function websocketEndpoint() {
  const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:'
  return `${protocol}//${window.location.host}/v1/connect`
}

async function loadWasmModule() {
  if (!wasmModulePromise) {
    wasmModulePromise = import(/* @vite-ignore */ '/web-wasm/links_web_client.js')
      .then(async module => {
        if (typeof module.default === 'function') await module.default()
        return module
      })
  }
  return wasmModulePromise
}

/** Create the real Rust/WASM core plus reconnecting binary WebSocket host. */
export async function createWebMessagingSession({
  userID,
  deviceID,
  mlsCredential,
  contacts = [],
  accessToken,
  onState,
  onTextMessage,
  onFailure
}) {
  const wasm = await loadWasmModule()
  if (typeof wasm.WebMessagingCore !== 'function') throw new Error('Web messaging WASM is unavailable')
  const rustCore = new wasm.WebMessagingCore(
    userID,
    deviceID,
    decodeBase64URL(mlsCredential)
  )
  const core = new WebWasmMessagingCore(rustCore)
  const token = () => accessToken()
  await publishLocalKeys(rustCore, token)
  await loadRecipients(core, contacts, token)
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
    sendText: (conversationID, recipientUserID, text) => session.sendText(conversationID, recipientUserID, text),
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
    body: rustCore.profile_upload(),
    cache: 'no-store'
  })
  if (!profile.ok) throw new Error('Could not publish browser pre-keys')
  const keyPackage = await fetch('/links-api/v1/mls/key-package', {
    method: 'PUT',
    headers,
    body: rustCore.key_package(),
    cache: 'no-store'
  })
  if (!keyPackage.ok) throw new Error('Could not publish browser MLS key package')
}

async function loadRecipients(core, contacts, accessToken) {
  const headers = { Authorization: `Bearer ${accessToken()}`, Accept: 'application/json' }
  for (const contact of contacts) {
    if (!contact?.userID) continue
    try {
      const directoryResponse = await fetch(`/links-api/v1/directory/users/${encodeURIComponent(contact.userID)}`, {
        headers,
        cache: 'no-store'
      })
      if (!directoryResponse.ok) continue
      const directory = await directoryResponse.json()
      for (const device of directory.devices || []) {
        const [prekeyResponse, keyPackageResponse] = await Promise.all([
          fetch(`/links-api/v1/prekeys/${encodeURIComponent(device.device_id)}/claim`, {
            method: 'POST', headers: { ...headers, Accept: 'application/octet-stream' }, cache: 'no-store'
          }),
          fetch(`/links-api/v1/mls/key-package/${encodeURIComponent(device.device_id)}`, {
            headers: { Authorization: headers.Authorization, Accept: 'application/octet-stream' }, cache: 'no-store'
          })
        ])
        if (!prekeyResponse.ok || !keyPackageResponse.ok) continue
        core.setRecipient(
          directory.user_id,
          device.device_id,
          decodeBase64URL(device.identity_public_key),
          new Uint8Array(await prekeyResponse.arrayBuffer()),
          decodeBase64URL(device.mls_credential),
          new Uint8Array(await keyPackageResponse.arrayBuffer())
        )
      }
    } catch {
      // A contact may not have a usable pre-key package yet.
    }
  }
}
