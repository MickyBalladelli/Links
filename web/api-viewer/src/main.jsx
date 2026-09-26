import { computed, mount, signal } from '@mickyballadelli/matrix'
import {
  Alert,
  Badge,
  Button,
  CheckIcon,
  ClockIcon,
  CodeIcon,
  CodeViewer,
  CopyIcon,
  EmptyState,
  FormField,
  LinkIcon,
  LockIcon,
  Select,
  SendIcon,
  Spinner,
  TextField,
  CloseIcon,
  prismTheme
} from '@mickyballadelli/prism'
import { endpointGroups } from './endpoints.js'
import './style.css'

const historyStorageKey = 'links-http-client-history-v1'
const methodOptions = ['GET', 'POST', 'PUT', 'PATCH', 'DELETE'].map(value => ({ value, label: value }))
const authOptions = [
  { value: 'none', label: 'No authentication' },
  { value: 'bearer', label: 'Bearer token' },
  { value: 'admin', label: 'Admin key' }
]

const baseURL = signal('/links-api')
const method = signal('GET')
const requestPath = signal('/v1/auth/me')
const authMode = signal('bearer')
const bearerToken = signal('')
const adminKey = signal('')
const headersSource = signal('{}')
const bodySource = signal('')
const response = signal(null)
const requestError = signal('')
const isSending = signal(false)
const copied = signal(false)
const history = signal(loadHistory())
let activeController = null

function loadHistory() {
  try {
    const stored = JSON.parse(localStorage.getItem(historyStorageKey) || '[]')
    return Array.isArray(stored) ? stored.slice(0, 20) : []
  } catch {
    return []
  }
}

function saveHistory(nextHistory) {
  history.value = nextHistory.slice(0, 20)
  try {
    localStorage.setItem(historyStorageKey, JSON.stringify(history.value))
  } catch {
    // History remains available for this tab when storage is unavailable.
  }
}

function prettyJSON(value) {
  return JSON.stringify(value, null, 2)
}

function parseHeaders() {
  if (!headersSource.value.trim()) return {}
  const parsed = JSON.parse(headersSource.value)
  if (!parsed || Array.isArray(parsed) || typeof parsed !== 'object') {
    throw new Error('Headers must be a JSON object.')
  }
  return Object.fromEntries(Object.entries(parsed).map(([key, value]) => [key, String(value)]))
}

function resolveURL() {
  const path = requestPath.value.trim()
  if (/^https?:\/\//i.test(path)) return path
  const base = baseURL.value.trim().replace(/\/$/, '')
  const suffix = path.startsWith('/') ? path : `/${path}`
  return `${base}${suffix}`
}

function applyEndpoint(endpoint) {
  method.value = endpoint.method
  requestPath.value = endpoint.path
  authMode.value = endpoint.auth
  bodySource.value = endpoint.body ? prettyJSON(endpoint.body) : ''
  response.value = null
  requestError.value = ''
}

async function sendRequest(event) {
  event?.preventDefault()
  if (isSending.value) return

  requestError.value = ''
  copied.value = false
  let headers
  try {
    headers = parseHeaders()
  } catch (error) {
    requestError.value = error.message
    return
  }

  if (authMode.value === 'bearer' && bearerToken.value.trim()) {
    headers.Authorization = `Bearer ${bearerToken.value.trim()}`
  }
  if (authMode.value === 'admin' && adminKey.value.trim()) {
    headers['X-Links-Admin-Key'] = adminKey.value.trim()
  }

  const sendsBody = !['GET', 'HEAD'].includes(method.value) && bodySource.value.trim().length > 0
  if (sendsBody && !Object.keys(headers).some(key => key.toLowerCase() === 'content-type')) {
    headers['Content-Type'] = 'application/json'
  }

  activeController = new AbortController()
  isSending.value = true
  const startedAt = performance.now()
  const url = resolveURL()

  try {
    const result = await fetch(url, {
      method: method.value,
      headers,
      body: sendsBody ? bodySource.value : undefined,
      signal: activeController.signal
    })
    const elapsedMs = Math.round(performance.now() - startedAt)
    const rawBody = await result.text()
    const contentType = result.headers.get('content-type') || ''
    let formattedBody = rawBody
    let language = 'text'
    if (contentType.includes('json') || /^[\s]*[\[{]/.test(rawBody)) {
      try {
        formattedBody = prettyJSON(JSON.parse(rawBody))
        language = 'json'
      } catch {
        language = 'text'
      }
    }

    response.value = {
      ok: result.ok,
      status: result.status,
      statusText: result.statusText,
      elapsedMs,
      size: new Blob([rawBody]).size,
      body: formattedBody || '(empty response body)',
      language,
      headers: prettyJSON(Object.fromEntries(result.headers.entries())),
      url
    }
    saveHistory([
      {
        id: crypto.randomUUID(),
        method: method.value,
        path: requestPath.value,
        auth: authMode.value,
        status: result.status,
        elapsedMs,
        requestedAt: new Date().toISOString()
      },
      ...history.value
    ])
  } catch (error) {
    const elapsedMs = Math.round(performance.now() - startedAt)
    const message = error.name === 'AbortError'
      ? 'Request cancelled.'
      : `${error.message} Use /links-api during local development, or enable CORS on a direct endpoint.`
    requestError.value = message
    saveHistory([
      {
        id: crypto.randomUUID(),
        method: method.value,
        path: requestPath.value,
        auth: authMode.value,
        status: 'ERR',
        elapsedMs,
        requestedAt: new Date().toISOString()
      },
      ...history.value
    ])
  } finally {
    isSending.value = false
    activeController = null
  }
}

function cancelRequest() {
  activeController?.abort()
}

function loadHistoryItem(item) {
  method.value = item.method
  requestPath.value = item.path
  authMode.value = item.auth
  requestError.value = ''
}

function clearHistory() {
  saveHistory([])
}

function shellQuote(value) {
  return `'${String(value).replaceAll("'", "'\\''")}'`
}

async function copyCurl() {
  let headers
  try {
    headers = parseHeaders()
  } catch (error) {
    requestError.value = error.message
    return
  }
  if (authMode.value === 'bearer' && bearerToken.value.trim()) {
    headers.Authorization = `Bearer ${bearerToken.value.trim()}`
  }
  if (authMode.value === 'admin' && adminKey.value.trim()) {
    headers['X-Links-Admin-Key'] = adminKey.value.trim()
  }
  const sendsBody = !['GET', 'HEAD'].includes(method.value) && bodySource.value.trim()
  if (sendsBody && !Object.keys(headers).some(key => key.toLowerCase() === 'content-type')) {
    headers['Content-Type'] = 'application/json'
  }
  const parts = [`curl -i -X ${method.value}`, shellQuote(resolveURL())]
  for (const [key, value] of Object.entries(headers)) {
    parts.push(`-H ${shellQuote(`${key}: ${value}`)}`)
  }
  if (sendsBody) parts.push(`--data-raw ${shellQuote(bodySource.value)}`)
  try {
    await navigator.clipboard.writeText(parts.join(' \\\n  '))
    copied.value = true
    window.setTimeout(() => { copied.value = false }, 1600)
  } catch {
    requestError.value = 'The browser blocked clipboard access.'
  }
}

function methodTone(value) {
  return value === 'GET' ? 'method-get'
    : value === 'POST' ? 'method-post'
      : value === 'DELETE' ? 'method-delete'
        : 'method-write'
}

function formatBytes(bytes) {
  if (bytes < 1024) return `${bytes} B`
  return `${(bytes / 1024).toFixed(1)} KB`
}

const authField = computed(() => {
  if (authMode.value === 'bearer') {
    return (
      <FormField
        label="Bearer token"
        hint="Kept in memory and omitted from request history."
        control={props => (
          <TextField {...props} type="password" value={bearerToken} autocomplete="off" placeholder="Access token" />
        )}
      />
    )
  }
  if (authMode.value === 'admin') {
    return (
      <FormField
        label="Admin key"
        hint="Sent as X-Links-Admin-Key and never persisted."
        control={props => (
          <TextField {...props} type="password" value={adminKey} autocomplete="off" placeholder="Admin API key" />
        )}
      />
    )
  }
  return <p class="quiet-note">This request will not include credentials.</p>
})

const responseSurface = computed(() => {
  if (isSending.value) {
    return (
      <div class="response-empty">
        <Spinner size="medium" ariaLabel="Sending request" />
        <span>Waiting for the Links service…</span>
      </div>
    )
  }
  if (!response.value) {
    return (
      <EmptyState
        icon={<CodeIcon size="1.4rem" />}
        title="No response yet"
        description="Choose an endpoint or compose a request, then send it."
      />
    )
  }
  const result = response.value
  return (
    <section class="response-result" aria-live="polite">
      <div class="response-summary">
        <Badge value={`${result.status} ${result.statusText}`} tone={result.ok ? 'success' : 'error'} />
        <span><ClockIcon size="0.9rem" /> {result.elapsedMs} ms</span>
        <span>{formatBytes(result.size)}</span>
        <span class="response-url" title={result.url}>{result.url}</span>
      </div>
      <CodeViewer
        tabs={[
          { id: 'body', label: 'Body', language: result.language, code: result.body },
          { id: 'headers', label: 'Headers', language: 'json', code: result.headers }
        ]}
        defaultTab="body"
        lineNumbers
        copyable
        minHeight="18rem"
        maxHeight="32rem"
        ariaLabel="HTTP response"
      />
    </section>
  )
})

const historyItems = computed(() => {
  if (history.value.length === 0) {
    return <p class="history-empty">Sent requests appear here.</p>
  }
  return history.value.map(item => (
    <button class="history-item" type="button" onClick={() => loadHistoryItem(item)}>
      <span class={`method-dot ${methodTone(item.method)}`}>{item.method}</span>
      <span class="history-path">{item.path}</span>
      <span class={item.status === 'ERR' || Number(item.status) >= 400 ? 'history-status is-error' : 'history-status'}>
        {item.status}
      </span>
    </button>
  ))
})

function App() {
  return (
    <div class="app" use:style={prismTheme}>
      <header class="topbar">
        <div class="brand">
          <span class="brand-mark"><LinkIcon size="1.05rem" /></span>
          <div>
            <strong>Links HTTP</strong>
            <span>Protocol workbench</span>
          </div>
        </div>
        <div class="endpoint-control">
          <span class="endpoint-light" aria-hidden="true"></span>
          <TextField value={baseURL} ariaLabel="Base URL" size="small" />
          <span class="proxy-label">Vite proxy</span>
        </div>
      </header>

      <div class="workspace">
        <aside class="catalog-pane">
          <div class="pane-heading">
            <div>
              <h2>Endpoints</h2>
              <p>Account service</p>
            </div>
            <Badge value={endpointGroups.reduce((total, group) => total + group.endpoints.length, 0)} />
          </div>
          <nav aria-label="Links endpoints" class="endpoint-list">
            {endpointGroups.map(group => (
              <section class="endpoint-group">
                <h3>{group.name}</h3>
                {group.endpoints.map(endpoint => (
                  <button type="button" class="endpoint-item" onClick={() => applyEndpoint(endpoint)}>
                    <span class={`method-label ${methodTone(endpoint.method)}`}>{endpoint.method}</span>
                    <span>{endpoint.name}</span>
                    {endpoint.auth !== 'none' ? <LockIcon size="0.75rem" /> : null}
                  </button>
                ))}
              </section>
            ))}
          </nav>
        </aside>

        <main class="request-pane">
          <form onSubmit={sendRequest}>
            <div class="request-line">
              <Select value={method} options={methodOptions} ariaLabel="HTTP method" class="method-select" />
              <TextField value={requestPath} ariaLabel="Request path" class="path-field" autocomplete="off" />
              <Button
                type="submit"
                label="Send"
                icon={<SendIcon />}
                iconPosition="end"
                variant="primary"
                loading={isSending}
                loadingLabel="Sending"
              />
              {computed(() => isSending.value ? (
                <Button
                  type="button"
                  label="Cancel"
                  variant="secondary"
                  onClick={cancelRequest}
                />
              ) : null)}
            </div>

            <div class="request-grid">
              <section class="request-section">
                <div class="section-heading">
                  <div>
                    <h2>Authentication</h2>
                    <p>Credentials stay in this browser tab.</p>
                  </div>
                  <Select value={authMode} options={authOptions} ariaLabel="Authentication mode" size="small" />
                </div>
                <div class="auth-field">{authField}</div>
              </section>

              <section class="request-section">
                <div class="section-heading">
                  <div>
                    <h2>Headers</h2>
                    <p>JSON object; generated auth headers are merged in.</p>
                  </div>
                </div>
                <CodeViewer
                  code={headersSource}
                  language="json"
                  filename="headers.json"
                  editable
                  lineNumbers
                  minHeight="8rem"
                  maxHeight="13rem"
                  ariaLabel="Request headers"
                />
              </section>
            </div>

            <section class="request-section body-section">
              <div class="section-heading">
                <div>
                  <h2>Request body</h2>
                  <p>Sent exactly as written for methods that accept a body.</p>
                </div>
                <Button
                  type="button"
                  label={computed(() => copied.value ? 'Copied' : 'Copy cURL')}
                  icon={computed(() => copied.value ? <CheckIcon /> : <CopyIcon />)}
                  variant="tertiary"
                  size="small"
                  onClick={copyCurl}
                />
              </div>
              <CodeViewer
                code={bodySource}
                language="json"
                filename="body.json"
                editable
                lineNumbers
                minHeight="12rem"
                maxHeight="24rem"
                ariaLabel="Request body"
              />
            </section>
          </form>

          {computed(() => requestError.value ? (
            <Alert tone="error" title="Request failed">{requestError}</Alert>
          ) : null)}

          <section class="response-section">
            <div class="pane-heading response-heading">
              <div>
                <h2>Response</h2>
                <p>Status, headers and payload</p>
              </div>
            </div>
            {responseSurface}
          </section>
        </main>

        <aside class="history-pane">
          <div class="pane-heading">
            <div>
              <h2>History</h2>
              <p>Metadata only</p>
            </div>
            <Button
              type="button"
              label="Clear"
              showLabel={false}
              icon={<CloseIcon />}
              ariaLabel="Clear request history"
              variant="tertiary"
              size="small"
              onClick={clearHistory}
            />
          </div>
          <div class="history-list">{historyItems}</div>
          <div class="privacy-note">
            <LockIcon size="0.85rem" />
            <span>Tokens, keys, headers and bodies are never stored in history.</span>
          </div>
        </aside>
      </div>
    </div>
  )
}

mount(<App />, document.querySelector('#app'))
