const databaseName = 'links-web-client-auth-v1'
const storeName = 'identity'
const legacyIdentityKey = 'current'

function openDatabase() {
  return new Promise((resolve, reject) => {
    if (!globalThis.indexedDB) {
      reject(new Error('Secure browser storage is unavailable.'))
      return
    }
    const request = indexedDB.open(databaseName, 1)
    request.onupgradeneeded = () => {
      if (!request.result.objectStoreNames.contains(storeName)) {
        request.result.createObjectStore(storeName)
      }
    }
    request.onsuccess = () => resolve(request.result)
    request.onerror = () => reject(request.error || new Error('Could not open browser identity storage.'))
  })
}

async function runTransaction(mode, operation) {
  const database = await openDatabase()
  try {
    return await new Promise((resolve, reject) => {
      const transaction = database.transaction(storeName, mode)
      const request = operation(transaction.objectStore(storeName))
      request.onsuccess = () => resolve(request.result)
      request.onerror = () => reject(request.error || new Error('Browser identity storage failed.'))
      transaction.onabort = () => reject(transaction.error || new Error('Browser identity storage was aborted.'))
    })
  } finally {
    database.close()
  }
}

export async function loadBrowserIdentity(handle) {
  const canonicalHandle = String(handle || '').trim().toLowerCase()
  if (!canonicalHandle) return null
  const identity = await runTransaction('readonly', store => store.get(`handle:${canonicalHandle}`))
  if (identity) return identity
  const legacy = await runTransaction('readonly', store => store.get(legacyIdentityKey))
  return legacy?.handle === canonicalHandle ? legacy : null
}

export function saveBrowserIdentity(identity) {
  const handle = String(identity?.handle || '').trim().toLowerCase()
  if (!handle) return Promise.reject(new Error('Cannot save an unnamed browser identity.'))
  return runTransaction('readwrite', store => store.put(identity, `handle:${handle}`))
}

export async function createBrowserIdentity() {
  if (!crypto?.subtle || typeof crypto.randomUUID !== 'function') {
    throw new Error('This browser cannot create a secure Links identity.')
  }
  let keyPair
  try {
    keyPair = await crypto.subtle.generateKey({ name: 'Ed25519' }, false, ['sign', 'verify'])
  } catch {
    throw new Error('This browser does not support Ed25519 account keys.')
  }
  let publicKey
  try {
    publicKey = new Uint8Array(await crypto.subtle.exportKey('raw', keyPair.publicKey))
  } catch {
    throw new Error('The browser could not prepare the public account key.')
  }
  if (publicKey.length !== 32) throw new Error('The browser returned an invalid account key.')
  return {
    deviceID: crypto.randomUUID(),
    mlsNodeID: crypto.randomUUID(),
    privateKey: keyPair.privateKey,
    publicKey,
    handle: '',
    mlsCredential: ''
  }
}
