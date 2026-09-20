import { computed, mount, signal } from '@mickyballadelli/matrix'
import {
  Alert,
  Avatar,
  Badge,
  Button,
  ChatIcon,
  CloseIcon,
  EmptyState,
  LiveStatusIcon,
  LockIcon,
  PlusIcon,
  Popup,
  SearchIcon,
  SendIcon,
  SettingsIcon,
  TextField,
  UserPlusIcon,
  prismTheme
} from '@mickyballadelli/prism'
import './style.css'

const storageKey = 'links-web-client-preview-v1'
const authBaseURL = signal('/links-api')
const accessToken = signal('')
const profileHandle = signal('micky')
const connectionState = signal('preview')
const selectedConversationID = signal('karine')
const composerText = signal('')
const contactQuery = signal('')
const searchQuery = signal('')
const notice = signal('')
const newConversationOpen = signal(false)
const addContactOpen = signal(false)
const profileOpen = signal(false)
const mobileSidebarOpen = signal(false)
const isResolving = signal(false)

const seededState = {
  contacts: [
    { handle: 'karine', userID: '4bc1797c-2dc3-4854-aada-6a52037a35e1', deviceCount: 2 },
    { handle: 'bob', userID: 'c394da90-1982-4541-bc6a-af981bd67978', deviceCount: 1 }
  ],
  conversations: [
    {
      id: 'karine',
      title: '@karine',
      recipientUserID: '4bc1797c-2dc3-4854-aada-6a52037a35e1',
      unreadCount: 0,
      messages: [
        { id: 'm1', text: 'The incoming username now resolves on my side.', outgoing: false, sentAt: '09:41' },
        { id: 'm2', text: 'Perfect. I’m checking the web client shell next.', outgoing: true, sentAt: '09:43' }
      ]
    },
    {
      id: 'bob',
      title: '@bob',
      recipientUserID: 'c394da90-1982-4541-bc6a-af981bd67978',
      unreadCount: 2,
      messages: [
        { id: 'm3', text: 'Can you see this conversation?', outgoing: false, sentAt: 'Yesterday' }
      ]
    }
  ]
}

function loadState() {
  try {
    const stored = JSON.parse(localStorage.getItem(storageKey) || 'null')
    if (stored?.contacts && stored?.conversations) return stored
  } catch {
    // Keep the preview usable when browser storage is unavailable.
  }
  return seededState
}

const initialState = loadState()
const contacts = signal(initialState.contacts)
const conversations = signal(initialState.conversations)

function persistState() {
  try {
    localStorage.setItem(storageKey, JSON.stringify({
      contacts: contacts.value,
      conversations: conversations.value
    }))
  } catch {
    // State remains available for the current tab.
  }
}

function normalizeHandle(value) {
  return String(value || '').trim().toLowerCase().replace(/^@/, '')
}

function avatarName(value) {
  return String(value || '')
    .trim()
    .replace(/^@/, '')
    .replace(/[_-]+/g, ' ')
}

function validHandle(value) {
  return /^[a-z0-9_]{3,32}$/.test(value)
}

function authHeaders() {
  const token = accessToken.value.trim()
  return token ? { Authorization: `Bearer ${token}` } : {}
}

async function lookupHandle(handle) {
  const cleanHandle = normalizeHandle(handle)
  if (!validHandle(cleanHandle)) {
    throw new Error('Use 3–32 lowercase letters, numbers, or underscores.')
  }
  if (!accessToken.value.trim()) {
    const local = contacts.value.find(contact => contact.handle === cleanHandle)
    if (local) return local
    throw new Error('Add a bearer token in Profile to resolve a new username.')
  }

  const base = authBaseURL.value.trim().replace(/\/$/, '')
  const response = await fetch(`${base}/v1/directory/${encodeURIComponent(cleanHandle)}`, {
    headers: authHeaders(),
    cache: 'no-store',
    credentials: 'omit',
    redirect: 'error'
  })
  if (response.status === 404) throw new Error(`No Links account uses @${cleanHandle}.`)
  if (!response.ok) throw new Error('The directory could not resolve that username.')
  const result = await response.json()
  return {
    handle: normalizeHandle(result.handle || cleanHandle),
    userID: result.user_id || result.userID,
    deviceCount: Array.isArray(result.devices) ? result.devices.length : Number(result.device_count || 0)
  }
}

function selectConversation(id) {
  selectedConversationID.value = id
  conversations.value = conversations.value.map(conversation => (
    conversation.id === id ? { ...conversation, unreadCount: 0 } : conversation
  ))
  mobileSidebarOpen.value = false
  persistState()
}

function openConversation(contact) {
  const existing = conversations.value.find(conversation => conversation.recipientUserID === contact.userID)
  if (existing) {
    selectConversation(existing.id)
    return
  }
  const conversation = {
    id: crypto.randomUUID(),
    title: `@${contact.handle}`,
    recipientUserID: contact.userID,
    unreadCount: 0,
    messages: []
  }
  conversations.value = [conversation, ...conversations.value]
  selectedConversationID.value = conversation.id
  persistState()
}

async function addOrOpenContact(event) {
  event?.preventDefault()
  if (isResolving.value) return
  notice.value = ''
  isResolving.value = true
  try {
    const contact = await lookupHandle(contactQuery.value)
    if (!contact.userID) throw new Error('The directory response did not include a user ID.')
    contacts.value = [
      ...contacts.value.filter(item => item.userID !== contact.userID),
      contact
    ].sort((left, right) => left.handle.localeCompare(right.handle))
    openConversation(contact)
    contactQuery.value = ''
    newConversationOpen.value = false
    addContactOpen.value = false
    notice.value = `Opened @${contact.handle}.`
    persistState()
  } catch (error) {
    notice.value = error.message
  } finally {
    isResolving.value = false
  }
}

function sendPreviewMessage(event) {
  event?.preventDefault()
  const text = composerText.value.trim()
  const id = selectedConversationID.value
  if (!text || !id) return
  conversations.value = conversations.value.map(conversation => (
    conversation.id === id
      ? {
          ...conversation,
          messages: [...conversation.messages, {
            id: crypto.randomUUID(),
            text,
            outgoing: true,
            sentAt: new Intl.DateTimeFormat([], { hour: '2-digit', minute: '2-digit' }).format(new Date())
          }]
        }
      : conversation
  ))
  composerText.value = ''
  notice.value = 'Saved in the local preview. Encrypted transport is not connected yet.'
  persistState()
  requestAnimationFrame(() => document.querySelector('.message-list')?.scrollTo({ top: 999999, behavior: 'smooth' }))
}

function resetPreview() {
  contacts.value = seededState.contacts
  conversations.value = seededState.conversations
  selectedConversationID.value = 'karine'
  persistState()
}

const selectedConversation = computed(() => (
  conversations.value.find(conversation => conversation.id === selectedConversationID.value) || null
))

const filteredConversations = computed(() => {
  const query = normalizeHandle(searchQuery.value)
  return conversations.value.filter(conversation => !query || normalizeHandle(conversation.title).includes(query))
})

const statusLabel = computed(() => connectionState.value === 'ready' ? 'Connected' : 'UI preview')

function StatusDot() {
  return <span class={computed(() => `status-dot is-${connectionState.value}`)} aria-hidden="true"></span>
}

function ConversationList() {
  return computed(() => filteredConversations.value.length ? filteredConversations.value.map(conversation => (
    <button
      type="button"
      class={computed(() => `conversation-row ${selectedConversationID.value === conversation.id ? 'is-selected' : ''}`)}
      onClick={() => selectConversation(conversation.id)}
    >
      <Avatar name={avatarName(conversation.title)} size="medium" />
      <span class="conversation-copy">
        <strong>{conversation.title}</strong>
        <span>{conversation.messages.at(-1)?.text || 'No messages yet'}</span>
      </span>
      {conversation.unreadCount > 0 ? <Badge value={conversation.unreadCount > 99 ? '99+' : conversation.unreadCount} tone="info" /> : null}
    </button>
  )) : (
    <p class="sidebar-empty">No matching conversations.</p>
  ))
}

function ContactList() {
  return computed(() => contacts.value.length ? contacts.value.map(contact => (
    <button type="button" class="contact-row" onClick={() => openConversation(contact)}>
      <Avatar name={avatarName(contact.handle)} size="small" />
      <span>
        <strong>@{contact.handle}</strong>
        <small>{contact.deviceCount || 'No'} active {contact.deviceCount === 1 ? 'device' : 'devices'}</small>
      </span>
    </button>
  )) : <p class="sidebar-empty">Add someone by username.</p>)
}

function Sidebar() {
  return (
    <aside class={computed(() => `sidebar ${mobileSidebarOpen.value ? 'is-open' : ''}`)}>
      <div class="sidebar-brand">
        <img class="brand-mark" src="/links-app-icon.png" alt="" aria-hidden="true" />
        <span><strong>Links</strong><small>Private messaging</small></span>
        <Button
          label="Close"
          showLabel={false}
          icon={<CloseIcon />}
          ariaLabel="Close sidebar"
          variant="tertiary"
          size="small"
          class="mobile-close"
          onClick={() => { mobileSidebarOpen.value = false }}
        />
      </div>

      <div class="sidebar-search">
        <SearchIcon size="0.9rem" />
        <input
          value={searchQuery}
          onInput={event => { searchQuery.value = event.currentTarget.value }}
          placeholder="Search conversations"
          aria-label="Search conversations"
        />
      </div>

      <section class="sidebar-section conversations-section">
        <div class="section-label">
          <span>Conversations</span>
          <Button label="New conversation" showLabel={false} icon={<PlusIcon />} ariaLabel="New conversation" variant="tertiary" size="small" onClick={() => { newConversationOpen.value = true }} />
        </div>
        <div class="conversation-list"><ConversationList /></div>
      </section>

      <section class="sidebar-section contacts-section">
        <div class="section-label">
          <span>Contacts</span>
          <Button label="Add contact" showLabel={false} icon={<UserPlusIcon />} ariaLabel="Add contact" variant="tertiary" size="small" onClick={() => { addContactOpen.value = true }} />
        </div>
        <div class="contact-list"><ContactList /></div>
      </section>

      <button type="button" class="profile-card" onClick={() => { profileOpen.value = true }}>
        <Avatar name={computed(() => avatarName(profileHandle.value))} size="medium" status="online" />
        <span class="profile-copy">
          <strong>{computed(() => `@${normalizeHandle(profileHandle.value) || 'profile'}`)}</strong>
          <span><StatusDot /> {statusLabel}</span>
        </span>
        <SettingsIcon size="1rem" />
      </button>
    </aside>
  )
}

function Messages() {
  return computed(() => {
    const conversation = selectedConversation.value
    if (!conversation) return null
    if (conversation.messages.length === 0) {
      return (
        <EmptyState
          icon={<LockIcon size="1.4rem" />}
          title="No messages yet"
          description="Messages in this conversation will be end-to-end encrypted once the browser core is connected."
        />
      )
    }
    return conversation.messages.map(message => (
      <div class={`message-row ${message.outgoing ? 'is-outgoing' : 'is-incoming'}`}>
        <div class="message-bubble">
          <p>{message.text}</p>
          <time>{message.sentAt}</time>
        </div>
      </div>
    ))
  })
}

function ConversationDetail() {
  return computed(() => {
    const conversation = selectedConversation.value
    if (!conversation) {
      return (
        <main class="empty-detail">
          <EmptyState icon={<ChatIcon size="1.7rem" />} title="Choose a conversation" description="Open a contact or create a conversation to start messaging." />
        </main>
      )
    }
    return (
      <main class="conversation-detail">
        <header class="conversation-header">
          <Button label="Open sidebar" showLabel={false} icon={<ChatIcon />} ariaLabel="Open conversations" variant="tertiary" size="small" class="mobile-menu" onClick={() => { mobileSidebarOpen.value = true }} />
          <Avatar name={avatarName(conversation.title)} size="large" />
          <div class="conversation-heading">
            <h1>{conversation.title}</h1>
            <p>Private one-to-one conversation</p>
          </div>
          <div class="conversation-status"><StatusDot /><span>{statusLabel}</span></div>
        </header>

        <div class="delivery-banner">
          <LiveStatusIcon size="1rem" />
          <div>
            <strong>Browser transport not connected</strong>
            <span>This shell is ready for the shared WASM messaging core and WebTextMessaging host.</span>
          </div>
          <Badge value="Preview" tone="warning" />
        </div>

        <div class="secure-note"><LockIcon size="0.8rem" /><span>Secure conversation initialization is pending browser-core integration.</span></div>

        <section class="message-list" aria-live="polite"><Messages /></section>

        <form class="composer" onSubmit={sendPreviewMessage}>
          <textarea
            value={composerText}
            onInput={event => { composerText.value = event.currentTarget.value }}
            onKeyDown={event => {
              if (event.key === 'Enter' && !event.shiftKey) {
                event.preventDefault()
                sendPreviewMessage(event)
              }
            }}
            placeholder="Message"
            aria-label={`Message ${conversation.title}`}
            rows="1"
          ></textarea>
          <Button type="submit" label="Send" showLabel={false} icon={<SendIcon />} ariaLabel="Save preview message" variant="primary" disabled={computed(() => !composerText.value.trim())} />
        </form>
      </main>
    )
  })
}

function UsernamePopup({ open, title, description }) {
  return (
    <Popup
      open={open}
      title={title}
      ariaDescription={description}
      size="small"
      onClose={() => { contactQuery.value = ''; notice.value = '' }}
      footer={() => (
        <div class="popup-actions">
          <Button label="Cancel" variant="secondary" onClick={() => { open.value = false }} />
          <Button label="Find and open" icon={<SearchIcon />} variant="primary" loading={isResolving} onClick={addOrOpenContact} />
        </div>
      )}
    >
      <form class="popup-form" onSubmit={addOrOpenContact}>
        <label for="contact-handle">Username</label>
        <TextField id="contact-handle" value={contactQuery} placeholder="alice" autocomplete="off" />
        <p>Recipient IDs and device counts resolve automatically through the directory API.</p>
        {computed(() => notice.value ? <Alert tone="error">{notice}</Alert> : null)}
      </form>
    </Popup>
  )
}

function ProfilePopup() {
  return (
    <Popup
      open={profileOpen}
      title="Web profile"
      ariaDescription="Configure the local interface preview and directory access."
      size="medium"
      footer={() => (
        <div class="popup-actions is-split">
          <Button label="Reset preview" variant="tertiary" onClick={resetPreview} />
          <Button label="Save" variant="primary" onClick={() => { profileOpen.value = false; notice.value = 'Profile settings updated.' }} />
        </div>
      )}
    >
      <div class="profile-form">
        <label for="profile-handle">Profile username</label>
        <TextField id="profile-handle" value={profileHandle} placeholder="username" autocomplete="username" />
        <label for="auth-base">Account service</label>
        <TextField id="auth-base" value={authBaseURL} placeholder="/links-api" autocomplete="off" />
        <label for="access-token">Bearer token</label>
        <TextField id="access-token" value={accessToken} type="password" placeholder="Optional for directory lookup" autocomplete="off" />
        <div class="privacy-copy"><LockIcon size="0.9rem" /><span>The token stays in memory and is never written to browser storage.</span></div>
      </div>
    </Popup>
  )
}

function App() {
  return (
    <div class="app" use:style={prismTheme}>
      <div class="mobile-scrim" onClick={() => { mobileSidebarOpen.value = false }}></div>
      <Sidebar />
      <ConversationDetail />
      <UsernamePopup open={newConversationOpen} title="New conversation" description="Find someone by username and open a private conversation." />
      <UsernamePopup open={addContactOpen} title="Add contact" description="Resolve and save a Links account by username." />
      <ProfilePopup />
      {computed(() => notice.value && !newConversationOpen.value && !addContactOpen.value ? (
        <div class="toast" role="status">{notice}</div>
      ) : null)}
    </div>
  )
}

mount(<App />, document.querySelector('#app'))
