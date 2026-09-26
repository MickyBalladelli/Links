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
    wasmModulePromise = import(/* @vite-ignore */ '/links-web-client.js')
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
