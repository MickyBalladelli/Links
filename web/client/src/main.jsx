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
const profilePictureKey = 'links-web-client-profile-picture-v1'
const profileHandleKey = 'links-web-client-profile-handle-v1'
const profileDisplayNameKey = 'links-web-client-profile-display-name-v1'
function loadProfileHandle() {
  try {
    const saved = localStorage.getItem(profileHandleKey)
    if (validHandle(saved)) return saved
  } catch {
    // Keep the preview usable when browser storage is unavailable.
  }
  return 'micky'
}
function loadProfileDisplayName() {
  try {
    const saved = localStorage.getItem(profileDisplayNameKey)
    if (validDisplayName(saved)) return saved.trim()
  } catch {
    // Keep the preview usable when browser storage is unavailable.
  }
  return ''
}
const profileHandle = signal(loadProfileHandle())
const profileHandleDraft = signal(profileHandle.value)
const profileDisplayName = signal(loadProfileDisplayName())
const profileDisplayNameDraft = signal(profileDisplayName.value)
const profileDisplayNameError = signal('')
const isChangingUsername = signal(false)
const profileUsernameError = signal('')
const isSavingDisplayName = signal(false)
const profilePicture = signal('')
const profilePictureSrc = signal('')
const profilePictureError = signal('')
let profilePictureObjectURL = ''
const connectionState = signal('preview')
const selectedConversationID = signal('karine')
const composerText = signal('')
const contactQuery = signal('')
const searchQuery = signal('')
const notice = signal('')
const newConversationOpen = signal(false)
const addContactOpen = signal(false)
const removeContactOpen = signal(false)
const contactPendingRemoval = signal(null)
const contactPictures = signal({})
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

function rememberProfilePicture(dataUrl) {
  if (profilePictureObjectURL) URL.revokeObjectURL(profilePictureObjectURL)
  profilePictureObjectURL = ''
  profilePicture.value = dataUrl
  if (!dataUrl) {
    profilePictureSrc.value = ''
    localStorage.removeItem(profilePictureKey)
    return
  }
  profilePictureObjectURL = URL.createObjectURL(dataUrlToBlob(dataUrl))
  profilePictureSrc.value = profilePictureObjectURL
  try {
    localStorage.setItem(profilePictureKey, dataUrl)
  } catch {
    profilePictureError.value = 'The picture could not be saved in this browser.'
  }
}

function dataUrlToBlob(dataUrl) {
  const [header, body] = String(dataUrl).split(',')
  const mime = header.match(/data:(.*?);/)?.[1] || 'image/jpeg'
  const binary = atob(body || '')
  const bytes = new Uint8Array(binary.length)
  for (let index = 0; index < binary.length; index += 1) bytes[index] = binary.charCodeAt(index)
  return new Blob([bytes], { type: mime })
}

function setProfilePicture(file) {
  profilePictureError.value = ''
  if (!file) return
  if (!file.type.startsWith('image/') || file.size > 8 * 1024 * 1024) {
    profilePictureError.value = 'Use an image under 8 MB.'
    return
  }
  const reader = new FileReader()
  reader.onload = () => {
    const image = new Image()
    image.onload = () => {
      const longest = Math.max(image.width, image.height)
      const scale = longest ? Math.min(1, 512 / longest) : 1
      const canvas = document.createElement('canvas')
      canvas.width = Math.max(1, Math.round(image.width * scale))
      canvas.height = Math.max(1, Math.round(image.height * scale))
      const context = canvas.getContext('2d')
      if (!context) {
        profilePictureError.value = 'The picture could not be saved.'
        return
      }
      context.drawImage(image, 0, 0, canvas.width, canvas.height)
      const jpeg = canvas.toDataURL('image/jpeg', 0.82)
      rememberProfilePicture(jpeg)
      publishProfilePictureBytes(jpeg)
    }
    image.onerror = () => { profilePictureError.value = 'The picture could not be read.' }
    image.src = String(reader.result || '')
  }
  reader.onerror = () => { profilePictureError.value = 'The picture could not be read.' }
  reader.readAsDataURL(file)
}

function removeProfilePicture() {
  profilePictureError.value = ''
  rememberProfilePicture('')
  publishProfilePictureRemoval()
}

function authBase() {
  return authBaseURL.value.trim().replace(/\/$/, '')
}

async function publishProfilePictureBytes(dataUrl) {
  const token = accessToken.value.trim()
  if (!token || !dataUrl) return
  await fetch(`${authBase()}/v1/profile/picture`, {
    method: 'PUT',
    headers: {
      Authorization: `Bearer ${token}`,
      'Content-Type': 'image/jpeg'
    },
    body: dataUrlToBlob(dataUrl),
    cache: 'no-store',
    credentials: 'omit',
    redirect: 'error'
  })
}

async function publishProfilePictureRemoval() {
  const token = accessToken.value.trim()
  if (!token) return
  await fetch(`${authBase()}/v1/profile/picture`, {
    method: 'DELETE',
    headers: { Authorization: `Bearer ${token}` },
    cache: 'no-store',
    credentials: 'omit',
    redirect: 'error'
  })
}

async function refreshContactPictures() {
  const next = { ...contactPictures.value }
  await Promise.all(contacts.value.map(async contact => {
    try {
      const response = await fetch(`${authBase()}/v1/directory/${encodeURIComponent(contact.handle)}/picture`, {
        cache: 'no-store',
        credentials: 'omit',
        redirect: 'error'
      })
      if (response.status === 404) {
        if (next[contact.userID]) URL.revokeObjectURL(next[contact.userID])
        delete next[contact.userID]
        return
      }
      if (!response.ok) return
      const blob = await response.blob()
      if (!blob.type.includes('jpeg') && blob.size < 3) return
      if (next[contact.userID]) URL.revokeObjectURL(next[contact.userID])
      next[contact.userID] = URL.createObjectURL(blob)
    } catch {
      // Keep the last picture when a contact is temporarily unreachable.
    }
  }))
  contactPictures.value = next
}

setInterval(() => { refreshContactPictures() }, 10000)
refreshContactPictures()

try {
  rememberProfilePicture(localStorage.getItem(profilePictureKey) || '')
} catch {
  profilePictureError.value = 'The saved profile picture could not be opened.'
  rememberProfilePicture('')
}

function ProfilePicture({ size = 'medium' }) {
  return computed(() => profilePictureSrc.value
    ? <img class={`profile-picture is-${size}`} src={profilePictureSrc.value} alt="" />
    : <Avatar name={avatarName(profileHandle.value)} size={size} status={size === 'medium' ? 'online' : undefined} />)
}

function avatarName(value) {
  return String(value || '')
    .trim()
    .replace(/^@/, '')
    .replace(/[_-]+/g, ' ')
}

function validHandle(value) {
  return /^[a-z][a-z0-9_]{2,31}$/.test(value)
}

function validDisplayName(value) {
  if (typeof value !== 'string') return false
  const cleanName = value.trim()
  return cleanName.length > 0
    && new TextEncoder().encode(cleanName).length <= 80
    && !/[\u0000-\u001f\u007f-\u009f]/u.test(cleanName)
}

async function saveProfileDisplayName() {
  if (isSavingDisplayName.value) return
  profileDisplayNameError.value = ''
  const cleanName = profileDisplayNameDraft.value.trim()
  if (!validDisplayName(cleanName)) {
    profileDisplayNameError.value = 'Use a name up to 80 bytes without control characters.'
    return
  }
  if (!accessToken.value.trim()) {
    profileDisplayNameError.value = 'Add a bearer token for your signed-in account.'
    return
  }
  isSavingDisplayName.value = true
  try {
    const response = await fetch(`${authBase()}/v1/account/display-name`, {
      method: 'PUT',
      headers: {
        ...authHeaders(),
        Accept: 'application/json',
        'Content-Type': 'application/json'
      },
      body: JSON.stringify({ display_name: cleanName }),
      cache: 'no-store',
      credentials: 'omit',
      redirect: 'error'
    })
    if (response.status === 401) throw new Error('Sign in again before changing your display name.')
    if (!response.ok) throw new Error('Could not save the display name. Check the account service.')
    const result = await response.json()
    const savedName = typeof result.display_name === 'string' ? result.display_name : ''
    if (savedName !== cleanName || !result.display_name_set) {
      throw new Error('The account service returned an invalid display name.')
    }
    profileDisplayName.value = savedName
    profileDisplayNameDraft.value = savedName
    try {
      localStorage.setItem(profileDisplayNameKey, cleanName)
    } catch {
      // Keep the account name in this tab when browser storage is unavailable.
    }
  } catch (error) {
    profileDisplayNameError.value = error.message || 'Could not save the display name.'
    return
  } finally {
    isSavingDisplayName.value = false
  }
  profileOpen.value = false
  notice.value = 'Display name saved to your account.'
}

function authHeaders() {
  const token = accessToken.value.trim()
  return token ? { Authorization: `Bearer ${token}` } : {}
}

async function changeProfileUsername() {
  if (isChangingUsername.value) return
  profileUsernameError.value = ''
  const handle = normalizeHandle(profileHandleDraft.value)
  if (!validHandle(handle)) {
    profileUsernameError.value = 'Use 3–32 lowercase letters, numbers, or underscores.'
    return
  }
  const token = accessToken.value.trim()
  if (!token) {
    profileUsernameError.value = 'Add a bearer token for your signed-in account.'
    return
  }

  isChangingUsername.value = true
  try {
    const base = authBaseURL.value.trim().replace(/\/$/, '')
    const response = await fetch(`${base}/v1/account/username`, {
      method: 'PUT',
      headers: {
        ...authHeaders(),
        Accept: 'application/json',
        'Content-Type': 'application/json'
      },
      body: JSON.stringify({ handle }),
      cache: 'no-store',
      credentials: 'omit',
      redirect: 'error'
    })
    if (response.status === 400) throw new Error('Use a valid lowercase username.')
    if (response.status === 401) throw new Error('Sign in again before changing your username.')
    if (response.status === 409) throw new Error('That username is already in use.')
    if (!response.ok) throw new Error('Could not change the username. Check the account service.')
    const result = await response.json()
    if (result.handle !== handle || !validHandle(result.handle)) {
      throw new Error('The account service returned an invalid username.')
    }
    profileHandle.value = result.handle
    profileHandleDraft.value = result.handle
    try {
      localStorage.setItem(profileHandleKey, result.handle)
    } catch {
      // Keep the updated username in this tab when browser storage is unavailable.
    }
    profileOpen.value = false
    notice.value = `Username changed to @${result.handle}. Your old username is now available to others.`
  } catch (error) {
    profileUsernameError.value = error.message || 'Could not change the username.'
  } finally {
    isChangingUsername.value = false
  }
}

async function refreshProfileUsername() {
  const token = accessToken.value.trim()
  if (!token) return
  try {
    const base = authBaseURL.value.trim().replace(/\/$/, '')
    const response = await fetch(`${base}/v1/account/username`, {
      headers: { ...authHeaders(), Accept: 'application/json' },
      cache: 'no-store',
      credentials: 'omit',
      redirect: 'error'
    })
    if (!response.ok) return
    const result = await response.json()
    if (result.handle == null) return
    if (!validHandle(result.handle)) return
    profileHandle.value = result.handle
    profileHandleDraft.value = result.handle
    try {
      localStorage.setItem(profileHandleKey, result.handle)
    } catch {
      // Keep the account name in this tab when browser storage is unavailable.
    }
  } catch {
    // Keep the last saved profile name when the account service is offline.
  }
}

async function refreshProfileDisplayName() {
  if (!accessToken.value.trim()) return
  try {
    const response = await fetch(`${authBase()}/v1/account/display-name`, {
      headers: { ...authHeaders(), Accept: 'application/json' },
      cache: 'no-store',
      credentials: 'omit',
      redirect: 'error'
    })
    if (!response.ok) return
    const result = await response.json()
    if (result.display_name_set) {
      const name = typeof result.display_name === 'string' ? result.display_name : ''
      if (name && !validDisplayName(name)) return
      profileDisplayName.value = name
      profileDisplayNameDraft.value = name
      try {
        if (name) localStorage.setItem(profileDisplayNameKey, name)
        else localStorage.removeItem(profileDisplayNameKey)
      } catch {
        // Keep the account name in this tab when browser storage is unavailable.
      }
      return
    }
    const savedLocalName = profileDisplayName.value
    if (!validDisplayName(savedLocalName)) return
    const publishResponse = await fetch(`${authBase()}/v1/account/display-name`, {
      method: 'PUT',
      headers: {
        ...authHeaders(),
        Accept: 'application/json',
        'Content-Type': 'application/json'
      },
      body: JSON.stringify({ display_name: savedLocalName }),
      cache: 'no-store',
      credentials: 'omit',
      redirect: 'error'
    })
    if (!publishResponse.ok) return
    profileDisplayNameDraft.value = savedLocalName
  } catch {
    // Keep the last saved profile name when the account service is offline.
  }
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
    displayName: typeof result.display_name === 'string' && validDisplayName(result.display_name)
      ? result.display_name.trim()
      : null,
    userID: result.user_id || result.userID,
    deviceCount: Array.isArray(result.devices) ? result.devices.length : Number(result.device_count || 0)
  }
}

let lastSavedContactNamesRefreshAt = 0
let isRefreshingSavedContactNames = false

async function refreshSavedContactNames() {
  const token = accessToken.value.trim()
  if (!token || isRefreshingSavedContactNames || Date.now() - lastSavedContactNamesRefreshAt < 30000) return
  const userIDs = [...new Set([
    ...contacts.value.map(contact => contact.userID),
    ...conversations.value.map(conversation => conversation.recipientUserID)
  ].filter(Boolean))]
  if (userIDs.length === 0) return
  isRefreshingSavedContactNames = true
  lastSavedContactNamesRefreshAt = Date.now()
  try {
    for (const userID of userIDs) {
      try {
        const response = await fetch(`${authBase()}/v1/directory/users/${encodeURIComponent(userID)}`, {
          headers: { ...authHeaders(), Accept: 'application/json' },
          cache: 'no-store',
          credentials: 'omit',
          redirect: 'error'
        })
        if (response.status === 429) break
        if (!response.ok) continue
        const result = await response.json()
        if ((result.user_id || result.userID) !== userID) continue
        const contact = {
          handle: normalizeHandle(result.handle),
          displayName: typeof result.display_name === 'string' && validDisplayName(result.display_name)
            ? result.display_name.trim()
            : null,
          userID,
          deviceCount: Array.isArray(result.devices) ? result.devices.length : Number(result.device_count || 0)
        }
        contacts.value = contacts.value.map(saved => saved.userID === userID ? contact : saved)
        conversations.value = conversations.value.map(conversation => (
          conversation.recipientUserID === userID
            ? { ...conversation, title: `@${contact.handle}`, displayName: contact.displayName }
            : conversation
        ))
      } catch {
        continue
      }
    }
    persistState()
  } finally {
    isRefreshingSavedContactNames = false
  }
}

function refreshNamesWhenVisible() {
  if (document.visibilityState !== 'hidden') {
    refreshProfileDisplayName()
    refreshSavedContactNames()
  }
}

window.addEventListener('focus', refreshNamesWhenVisible)
document.addEventListener('visibilitychange', refreshNamesWhenVisible)

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
    conversations.value = conversations.value.map(conversation => (
      conversation.id === existing.id
        ? { ...conversation, title: `@${contact.handle}`, displayName: contact.displayName }
        : conversation
    ))
    selectConversation(existing.id)
    persistState()
    return
  }
  const conversation = {
    id: crypto.randomUUID(),
    title: `@${contact.handle}`,
    displayName: contact.displayName,
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
  return conversations.value.filter(conversation => !query
    || normalizeHandle(conversation.displayName || conversation.title).includes(query)
    || normalizeHandle(conversation.title).includes(query))
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
      {contactPictures.value[conversation.recipientUserID]
        ? <img class="profile-picture is-medium" src={contactPictures.value[conversation.recipientUserID]} alt="" />
        : <Avatar name={avatarName(conversation.displayName || conversation.title)} size="medium" />}
      <span class="conversation-copy">
        <strong>{conversation.displayName || conversation.title}</strong>
        <span>{conversation.displayName ? conversation.title : (conversation.messages.at(-1)?.text || 'No messages yet')}</span>
        {conversation.displayName ? <span>{conversation.messages.at(-1)?.text || 'No messages yet'}</span> : null}
      </span>
      {conversation.unreadCount > 0 ? <Badge value={conversation.unreadCount > 99 ? '99+' : conversation.unreadCount} tone="info" /> : null}
    </button>
  )) : (
    <p class="sidebar-empty">No matching conversations.</p>
  ))
}

function askRemoveContact(event, contact) {
  event.preventDefault()
  event.stopPropagation()
  contactPendingRemoval.value = contact
  removeContactOpen.value = true
}

function confirmRemoveContact() {
  const contact = contactPendingRemoval.value
  removeContactOpen.value = false
  contactPendingRemoval.value = null
  if (!contact) return
  contacts.value = contacts.value.filter(item => item.userID !== contact.userID)
  persistState()
}

function ContactList() {
  return computed(() => contacts.value.length ? contacts.value.map(contact => (
    <div class="contact-row">
      <button type="button" class="contact-open" onClick={() => openConversation(contact)}>
        {contactPictures.value[contact.userID]
          ? <img class="profile-picture is-small" src={contactPictures.value[contact.userID]} alt="" />
          : <Avatar name={avatarName(contact.displayName || contact.handle)} size="small" />}
        <span class="contact-copy">
          <strong>{contact.displayName || `@${contact.handle}`}</strong>
          <small>@{contact.handle} · {contact.deviceCount || 'No'} active {contact.deviceCount === 1 ? 'device' : 'devices'}</small>
        </span>
      </button>
      <button
        type="button"
        class="contact-remove"
        aria-label={`Remove @${contact.handle}`}
        title={`Remove @${contact.handle}`}
        onClick={event => askRemoveContact(event, contact)}
      >
        <TrashIcon />
      </button>
    </div>
  )) : <p class="sidebar-empty">Add someone by username.</p>)
}

function TrashIcon() {
  return (
    <svg viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
      <path d="M4 7h16" />
      <path d="M9 7V5h6v2" />
      <path d="M8 7l1 12h6l1-12" />
    </svg>
  )
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

      <button type="button" class="profile-card" onClick={() => { profileHandleDraft.value = profileHandle.value; profileDisplayNameDraft.value = profileDisplayName.value; profileDisplayNameError.value = ''; profileUsernameError.value = ''; profileOpen.value = true; refreshProfileUsername(); refreshProfileDisplayName(); refreshSavedContactNames() }}>
        <ProfilePicture />
        <span class="profile-copy">
          <strong>{computed(() => profileDisplayName.value || `@${normalizeHandle(profileHandle.value) || 'profile'}`)}</strong>
          <span><span>@{computed(() => normalizeHandle(profileHandle.value) || 'profile')}</span><StatusDot /> {statusLabel}</span>
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
          {contactPictures.value[conversation.recipientUserID]
            ? <img class="profile-picture is-large" src={contactPictures.value[conversation.recipientUserID]} alt="" />
            : <Avatar name={avatarName(conversation.displayName || conversation.title)} size="large" />}
          <div class="conversation-heading">
            <h1>{conversation.displayName || conversation.title}</h1>
            <p>{conversation.title}</p>
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

function RemoveContactPopup() {
  return (
    <Popup
      open={removeContactOpen}
      title="Remove contact?"
      ariaDescription="This removes the saved contact. Existing conversations and messages stay."
      size="small"
      onClose={() => { removeContactOpen.value = false; contactPendingRemoval.value = null }}
      footer={() => (
        <div class="popup-actions">
          <Button label="Cancel" variant="secondary" onClick={() => { removeContactOpen.value = false; contactPendingRemoval.value = null }} />
          <Button label="Remove" variant="primary" onClick={confirmRemoveContact} />
        </div>
      )}
    >
      <p>{computed(() => {
        const contact = contactPendingRemoval.value
        return contact
          ? `Remove @${contact.handle}? Existing conversations and messages stay.`
          : 'Existing conversations and messages stay.'
      })}</p>
    </Popup>
  )
}

function ProfilePopup() {
  return (
    <Popup
      open={profileOpen}
      title="Web profile"
      ariaDescription="Change the shared display name, account username, and directory access."
      size="medium"
      footer={() => (
        <div class="popup-actions is-split">
          <Button label="Reset preview" variant="tertiary" onClick={resetPreview} />
          <Button label="Save display name" variant="secondary" loading={isSavingDisplayName} disabled={isSavingDisplayName} onClick={saveProfileDisplayName} />
          <Button label="Change username" variant="primary" loading={isChangingUsername} disabled={isChangingUsername} onClick={changeProfileUsername} />
        </div>
      )}
    >
      <div class="profile-form">
        <div class="profile-picture-editor">
          <ProfilePicture size="large" />
          <div class="profile-picture-actions">
            <label class="picture-picker">
              {computed(() => profilePicture.value ? 'Change picture' : 'Add picture')}
              <input
                type="file"
                accept="image/jpeg,image/png,image/webp,image/gif"
                onChange={event => {
                  setProfilePicture(event.currentTarget.files?.[0])
                  event.currentTarget.value = ''
                }}
              />
            </label>
            {computed(() => profilePicture.value
              ? <Button label="Remove picture" variant="tertiary" onClick={removeProfilePicture} />
              : null)}
          </div>
          {computed(() => profilePictureError.value ? <Alert tone="error">{profilePictureError}</Alert> : null)}
        </div>
        <label for="profile-display-name">Display name</label>
        <TextField id="profile-display-name" value={profileDisplayNameDraft} placeholder="Name" autocomplete="name" />
        <p>Shown to people in your contacts and conversations.</p>
        {computed(() => profileDisplayNameError.value ? <Alert tone="error">{profileDisplayNameError}</Alert> : null)}
        <label for="profile-handle">New username</label>
        <TextField id="profile-handle" value={profileHandleDraft} placeholder="username" autocomplete="username" />
        <p>Use 3–32 lowercase letters, numbers, or underscores. Your old username becomes available to others.</p>
        {computed(() => profileUsernameError.value ? <Alert tone="error">{profileUsernameError}</Alert> : null)}
        <label for="auth-base">Account service</label>
        <TextField id="auth-base" value={authBaseURL} placeholder="/links-api" autocomplete="off" />
        <label for="access-token">Bearer token</label>
        <TextField id="access-token" value={accessToken} type="password" placeholder="Required to change names" autocomplete="off" />
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
      <RemoveContactPopup />
      <ProfilePopup />
      {computed(() => notice.value && !newConversationOpen.value && !addContactOpen.value ? (
        <div class="toast" role="status">{notice}</div>
      ) : null)}
    </div>
  )
}

mount(<App />, document.querySelector('#app'))
