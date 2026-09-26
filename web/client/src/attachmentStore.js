const databaseName = 'links-web-client-preview-attachments-v1'
const storeName = 'attachments'

function openDatabase() {
  return new Promise((resolve, reject) => {
    if (!globalThis.indexedDB) {
      reject(new Error('IndexedDB is unavailable'))
      return
    }
    const request = indexedDB.open(databaseName, 1)
    request.onupgradeneeded = () => {
      if (!request.result.objectStoreNames.contains(storeName)) {
        request.result.createObjectStore(storeName)
      }
    }
    request.onsuccess = () => resolve(request.result)
    request.onerror = () => reject(request.error || new Error('Could not open attachment storage'))
  })
}

async function runTransaction(mode, operation) {
  const database = await openDatabase()
  try {
    return await new Promise((resolve, reject) => {
      const transaction = database.transaction(storeName, mode)
      const request = operation(transaction.objectStore(storeName))
      request.onsuccess = () => resolve(request.result)
      request.onerror = () => reject(request.error || new Error('Attachment storage failed'))
      transaction.onabort = () => reject(transaction.error || new Error('Attachment storage was aborted'))
    })
  } finally {
    database.close()
  }
}

export function saveAttachment(id, blob) {
  if (!id || !(blob instanceof Blob)) return Promise.reject(new Error('Invalid attachment'))
  return runTransaction('readwrite', store => store.put(blob, id))
}

export function readAttachment(id) {
  if (!id) return Promise.resolve(null)
  return runTransaction('readonly', store => store.get(id)).then(value => value instanceof Blob ? value : null)
}

export function clearAttachments() {
  return runTransaction('readwrite', store => store.clear())
}
