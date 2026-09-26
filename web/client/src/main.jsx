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
import { clearAttachments, readAttachment, saveAttachment } from './attachmentStore.js'

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
const isLoggingOut = signal(false)
const logoutConfirmOpen = signal(false)
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
const createGroupOpen = signal(false)
const groupInfoOpen = signal(false)
const groupNameDraft = signal('')
const groupMemberDraft = signal([])
const groupError = signal('')
const pendingAttachment = signal(null)
const attachmentURLs = signal({})
const composerDragActive = signal(false)

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
    },
    {
      id: 'launch-crew',
      title: 'Launch crew',
      isGroup: true,
      groupActive: true,
      unreadCount: 1,
      members: [
        { userID: 'self', handle: 'micky', role: 'owner', isSelf: true },
        { userID: '4bc1797c-2dc3-4854-aada-6a52037a35e1', handle: 'karine', role: 'member' },
        { userID: 'c394da90-1982-4541-bc6a-af981bd67978', handle: 'bob', role: 'member' }
      ],
      messages: [
        { id: 'm4', text: 'Images and files can live in this group too.', outgoing: false, senderHandle: 'karine', sentAt: 'Yesterday' }
      ]
    }
  ]
}

function loadState() {
  try {
    const stored = JSON.parse(localStorage.getItem(storageKey) || 'null')
    if (Array.isArray(stored?.contacts) && Array.isArray(stored?.conversations)) {
      return {
        contacts: stored.contacts,
        conversations: stored.conversations.map(conversation => ({
          ...conversation,
          unreadCount: Number(conversation.unreadCount || 0),
          messages: Array.isArray(conversation.messages) ? conversation.messages : [],
          members: conversation.isGroup && Array.isArray(conversation.members) ? conversation.members : []
        }))
      }
    }
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

function currentTimeLabel() {
  return new Intl.DateTimeFormat([], { hour: '2-digit', minute: '2-digit' }).format(new Date())
}

function conversationSummary(conversation) {
  const message = conversation.messages.at(-1)
  if (!message) return conversation.isGroup ? `${conversation.members?.length || 1} members` : 'No messages yet'
  if (message.attachment?.kind === 'image') return message.text || 'Image'
  if (message.attachment?.kind === 'file') return message.attachment.name || 'File'
  return message.text || 'Message'
}

function formatBytes(bytes) {
  const size = Number(bytes || 0)
  if (size < 1024) return `${size} B`
  if (size < 1024 * 1024) return `${Math.round(size / 1024)} KB`
  return `${(size / (1024 * 1024)).toFixed(size >= 10 * 1024 * 1024 ? 0 : 1)} MB`
}

function setAttachmentURL(id, url) {
  const previous = attachmentURLs.value[id]
  if (previous && previous !== url) URL.revokeObjectURL(previous)
  attachmentURLs.value = { ...attachmentURLs.value, [id]: url }
}

async function hydrateAttachment(attachment) {
  if (!attachment?.id || attachmentURLs.value[attachment.id]) return
  try {
    const blob = await readAttachment(attachment.id)
    if (blob) setAttachmentURL(attachment.id, URL.createObjectURL(blob))
  } catch {
    // A missing browser database leaves the message metadata visible.
  }
}

function hydrateStoredAttachments() {
  conversations.value.forEach(conversation => {
    conversation.messages.forEach(message => { hydrateAttachment(message.attachment) })
  })
}

function clearPendingAttachment() {
  const pending = pendingAttachment.value
  if (pending?.previewURL) URL.revokeObjectURL(pending.previewURL)
  pendingAttachment.value = null
  composerDragActive.value = false
}

function selectComposerAttachment(file) {
  if (!(file instanceof File)) return
  const isImage = file.type.startsWith('image/')
  const maximumBytes = isImage ? 32 * 1024 * 1024 : 20 * 1024 * 1024
  if (file.size === 0 || file.size > maximumBytes) {
    notice.value = isImage ? 'Images must be under 32 MB.' : 'Files must be under 20 MB.'
    return
  }
  clearPendingAttachment()
  pendingAttachment.value = {
    id: crypto.randomUUID(),
    kind: isImage ? 'image' : 'file',
    name: file.name || (isImage ? 'Image' : 'File'),
    mimeType: file.type || 'application/octet-stream',
    size: file.size,
    blob: file,
    previewURL: isImage ? URL.createObjectURL(file) : ''
  }
  notice.value = ''
}

function handleComposerFiles(files) {
  const file = Array.from(files || [])[0]
  if (file) selectComposerAttachment(file)
}

function handleComposerPaste(event) {
  const file = Array.from(event.clipboardData?.files || [])[0]
  if (!file) return
  event.preventDefault()
  selectComposerAttachment(file)
}

function handleComposerDrop(event) {
  event.preventDefault()
  composerDragActive.value = false
  handleComposerFiles(event.dataTransfer?.files)
}

hydrateStoredAttachments()
window.addEventListener('beforeunload', () => {
  clearPendingAttachment()
  Object.values(attachmentURLs.value).forEach(url => URL.revokeObjectURL(url))
})

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

function openLogoutConfirmation() {
  profileOpen.value = false
  requestAnimationFrame(() => { logoutConfirmOpen.value = true })
}

function cancelLogoutConfirmation() {
  if (isLoggingOut.value) return
  logoutConfirmOpen.value = false
  requestAnimationFrame(() => { profileOpen.value = true })
}

async function logoutAccount() {
  if (isLoggingOut.value) return
  const token = accessToken.value.trim()
  if (!token) {
    accessToken.value = ''
    connectionState.value = 'preview'
    logoutConfirmOpen.value = false
    profileOpen.value = false
    notice.value = 'Logged out locally. No remote session token was available to revoke.'
    return
  }

  isLoggingOut.value = true
  let remoteRevoked = false
  try {
    const response = await fetch(`${authBase()}/v1/auth/logout`, {
      method: 'POST',
      headers: { Authorization: `Bearer ${token}` },
      cache: 'no-store',
      credentials: 'omit',
      redirect: 'error'
    })
    remoteRevoked = response.ok || response.status === 401
  } catch {
    remoteRevoked = false
  } finally {
    if (accessToken.value.trim() === token) accessToken.value = ''
    connectionState.value = 'preview'
    isLoggingOut.value = false
    logoutConfirmOpen.value = false
    profileOpen.value = false
  }
  notice.value = remoteRevoked
    ? 'Logged out. Local conversations and attachments were kept.'
    : 'Logged out locally, but remote session revocation could not be confirmed.'
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
let lastOwnProfileRefreshAt = 0

async function refreshSavedContactNames() {
  const token = accessToken.value.trim()
  if (!token || isRefreshingSavedContactNames || Date.now() - lastSavedContactNamesRefreshAt < 4000) return
  const userIDs = [...new Set([
    ...contacts.value.map(contact => contact.userID),
    ...conversations.value.map(conversation => conversation.recipientUserID)
  ].filter(Boolean))]
  if (userIDs.length === 0) return
  isRefreshingSavedContactNames = true
  lastSavedContactNamesRefreshAt = Date.now()
  try {
    for (let offset = 0; offset < userIDs.length; offset += 256) {
      const batch = userIDs.slice(offset, offset + 256)
      try {
        const response = await fetch(`${authBase()}/v1/directory/profiles/sync`, {
          method: 'POST',
          headers: {
            ...authHeaders(),
            Accept: 'application/json',
            'Content-Type': 'application/json'
          },
          body: JSON.stringify({ user_ids: batch }),
          cache: 'no-store',
          credentials: 'omit',
          redirect: 'error'
        })
        if (response.status === 429) break
        if (!response.ok) continue
        const result = await response.json()
        if (!Array.isArray(result.profiles)) continue
        for (const profile of result.profiles) {
          const userID = profile.user_id || profile.userID
          const handle = normalizeHandle(profile.handle)
          if (!batch.includes(userID) || !validHandle(handle)) continue
          const contact = {
            handle,
            displayName: typeof profile.display_name === 'string' && validDisplayName(profile.display_name)
              ? profile.display_name.trim()
              : null,
            userID,
            deviceCount: Math.max(0, Number(profile.device_count) || 0)
          }
          contacts.value = contacts.value
            .map(saved => saved.userID === userID ? contact : saved)
            .sort((left, right) => left.handle.localeCompare(right.handle))
          conversations.value = conversations.value.map(conversation => (
            conversation.recipientUserID === userID
              ? { ...conversation, title: `@${contact.handle}`, displayName: contact.displayName }
              : conversation
          ))
        }
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
    if (Date.now() - lastOwnProfileRefreshAt >= 30000) {
      lastOwnProfileRefreshAt = Date.now()
      refreshProfileUsername()
      refreshProfileDisplayName()
    }
    refreshSavedContactNames()
  }
}

window.addEventListener('focus', refreshNamesWhenVisible)
document.addEventListener('visibilitychange', refreshNamesWhenVisible)
window.setInterval(refreshNamesWhenVisible, 5000)

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

function openCreateGroup() {
  groupNameDraft.value = ''
  groupMemberDraft.value = []
  groupError.value = ''
  createGroupOpen.value = true
}

function toggleGroupMember(userID) {
  groupMemberDraft.value = groupMemberDraft.value.includes(userID)
    ? groupMemberDraft.value.filter(id => id !== userID)
    : [...groupMemberDraft.value, userID]
}

function createPreviewGroup(event) {
  event?.preventDefault()
  const name = groupNameDraft.value.trim()
  if (!name || name.length > 64) {
    groupError.value = 'Use a group name of 1–64 characters.'
    return
  }
  if (groupMemberDraft.value.length === 0) {
    groupError.value = 'Choose at least one contact.'
    return
  }
  const members = [
    { userID: 'self', handle: normalizeHandle(profileHandle.value), role: 'owner', isSelf: true },
    ...contacts.value
      .filter(contact => groupMemberDraft.value.includes(contact.userID))
      .map(contact => ({ userID: contact.userID, handle: contact.handle, displayName: contact.displayName, role: 'member' }))
  ]
  const group = {
    id: crypto.randomUUID(),
    title: name,
    isGroup: true,
    groupActive: true,
    unreadCount: 0,
    members,
    messages: []
  }
  conversations.value = [group, ...conversations.value]
  selectedConversationID.value = group.id
  createGroupOpen.value = false
  groupError.value = ''
  persistState()
  notice.value = `Created ${name} in the local preview.`
}

function openGroupInfo(conversation) {
  if (!conversation?.isGroup) return
  groupNameDraft.value = conversation.title
  groupMemberDraft.value = (conversation.members || []).filter(member => !member.isSelf).map(member => member.userID)
  groupError.value = ''
  groupInfoOpen.value = true
}

function saveGroupDetails(event) {
  event?.preventDefault()
  const conversation = selectedConversation.value
  const name = groupNameDraft.value.trim()
  if (!conversation?.isGroup) return
  if (!name || name.length > 64) {
    groupError.value = 'Use a group name of 1–64 characters.'
    return
  }
  const existingSelf = conversation.members?.find(member => member.isSelf) || {
    userID: 'self', handle: normalizeHandle(profileHandle.value), role: 'owner', isSelf: true
  }
  const members = [
    existingSelf,
    ...contacts.value
      .filter(contact => groupMemberDraft.value.includes(contact.userID))
      .map(contact => {
        const current = conversation.members?.find(member => member.userID === contact.userID)
        return {
          userID: contact.userID,
          handle: contact.handle,
          displayName: contact.displayName,
          role: current?.role || 'member'
        }
      })
  ]
  conversations.value = conversations.value.map(item => item.id === conversation.id
    ? { ...item, title: name, members }
    : item)
  groupInfoOpen.value = false
  groupError.value = ''
  persistState()
  notice.value = 'Group details saved in the local preview.'
}

function disbandSelectedGroup() {
  const conversation = selectedConversation.value
  if (!conversation?.isGroup) return
  conversations.value = conversations.value.filter(item => item.id !== conversation.id)
  selectedConversationID.value = conversations.value[0]?.id || null
  groupInfoOpen.value = false
  persistState()
  notice.value = `Removed ${conversation.title} from the local preview.`
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

async function sendPreviewMessage(event) {
  event?.preventDefault()
  const text = composerText.value.trim()
  const attachment = pendingAttachment.value
  const id = selectedConversationID.value
  if ((!text && !attachment) || !id) return

  const additions = []
  if (attachment) {
    additions.push({
      id: crypto.randomUUID(),
      text: attachment.kind === 'image' ? 'Image' : attachment.name,
      attachment: {
        id: attachment.id,
        kind: attachment.kind,
        name: attachment.name,
        mimeType: attachment.mimeType,
        size: attachment.size
      },
      outgoing: true,
      sentAt: currentTimeLabel()
    })
  }
  if (text) {
    additions.push({ id: crypto.randomUUID(), text, outgoing: true, sentAt: currentTimeLabel() })
  }

  if (attachment) {
    try {
      await saveAttachment(attachment.id, attachment.blob)
      setAttachmentURL(attachment.id, attachment.previewURL || URL.createObjectURL(attachment.blob))
      pendingAttachment.value = null
    } catch {
      notice.value = 'The attachment could not be saved in this browser.'
      return
    }
  }
  conversations.value = conversations.value.map(conversation => conversation.id === id
    ? { ...conversation, messages: [...conversation.messages, ...additions] }
    : conversation)
  composerText.value = ''
  notice.value = 'Saved in the local preview. Encrypted transport is not connected yet.'
  persistState()
  requestAnimationFrame(() => document.querySelector('.message-list')?.scrollTo({ top: 999999, behavior: 'smooth' }))
}

async function resetPreview() {
  clearPendingAttachment()
  Object.values(attachmentURLs.value).forEach(url => URL.revokeObjectURL(url))
  attachmentURLs.value = {}
  try { await clearAttachments() } catch { /* Keep reset available without IndexedDB. */ }
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

function GroupIcon({ size = 40 }) {
  return (
    <span class={`group-avatar is-${size}`} aria-hidden="true">
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
        <path d="M16 20v-1.4c0-2-1.8-3.6-4-3.6H6c-2.2 0-4 1.6-4 3.6V20" />
        <circle cx="9" cy="7" r="4" />
        <path d="M17 11a3.5 3.5 0 1 0-2.8-5.6M18 15c2.2 0 4 1.6 4 3.6V20" />
      </svg>
    </span>
  )
}

function ConversationAvatar({ conversation, size = 'medium' }) {
  if (conversation.isGroup) return <GroupIcon size={size === 'large' ? 48 : size === 'small' ? 32 : 40} />
  return contactPictures.value[conversation.recipientUserID]
    ? <img class={`profile-picture is-${size}`} src={contactPictures.value[conversation.recipientUserID]} alt="" />
    : <Avatar name={avatarName(conversation.displayName || conversation.title)} size={size} />
}

function ConversationList() {
  return computed(() => filteredConversations.value.length ? filteredConversations.value.map(conversation => (
    <button
      type="button"
      class={computed(() => `conversation-row ${selectedConversationID.value === conversation.id ? 'is-selected' : ''}`)}
      onClick={() => selectConversation(conversation.id)}
    >
      <ConversationAvatar conversation={conversation} />
      <span class="conversation-copy">
        <strong>{conversation.displayName || conversation.title}</strong>
        {conversation.isGroup
          ? <span>{conversationSummary(conversation)}</span>
          : <>
              <span>{conversation.displayName ? conversation.title : conversationSummary(conversation)}</span>
              {conversation.displayName ? <span>{conversationSummary(conversation)}</span> : null}
            </>}
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

function FileIcon() {
  return (
    <svg viewBox="0 0 24 24" width="20" height="20" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
      <path d="M6 2h8l4 4v16H6z" />
      <path d="M14 2v5h5" />
    </svg>
  )
}

function PaperclipIcon() {
  return (
    <svg viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
      <path d="m20.5 11.5-8.8 8.8a6 6 0 0 1-8.5-8.5l9.2-9.2a4 4 0 0 1 5.7 5.7l-9.2 9.2a2 2 0 1 1-2.8-2.8l8.5-8.5" />
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
          <div class="section-actions">
            <button type="button" class="icon-action" aria-label="New group" title="New group" onClick={openCreateGroup}><GroupIcon size={24} /></button>
            <Button label="New conversation" showLabel={false} icon={<PlusIcon />} ariaLabel="New conversation" variant="tertiary" size="small" onClick={() => { newConversationOpen.value = true }} />
          </div>
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
    return conversation.messages.map(message => {
      const attachment = message.attachment
      const source = attachment ? attachmentURLs.value[attachment.id] : ''
      if (attachment && !source) hydrateAttachment(attachment)
      return (
        <div class={`message-row ${message.outgoing ? 'is-outgoing' : 'is-incoming'}`}>
          <div class="message-bubble">
            {conversation.isGroup && !message.outgoing
              ? <span class="message-sender">@{message.senderHandle || 'member'}</span>
              : null}
            {attachment?.kind === 'image'
              ? source
                ? <a class="message-image-link" href={source} download={attachment.name} title="Save image">
                    <img class="message-image" src={source} alt={attachment.name || 'Shared image'} />
                  </a>
                : <div class="attachment-loading">Image unavailable</div>
              : attachment?.kind === 'file'
                ? <a class={`file-message ${source ? '' : 'is-unavailable'}`} href={source || undefined} download={attachment.name}>
                    <FileIcon />
                    <span><strong>{attachment.name}</strong><small>{formatBytes(attachment.size)}</small></span>
                  </a>
                : <p>{message.text}</p>}
            <time>{message.sentAt}</time>
          </div>
        </div>
      )
    })
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
          <ConversationAvatar conversation={conversation} size="large" />
          <div class="conversation-heading">
            <h1>{conversation.displayName || conversation.title}</h1>
            <p>{conversation.isGroup ? `${conversation.members?.length || 1} members` : conversation.title}</p>
          </div>
          {conversation.isGroup
            ? <button type="button" class="group-info-button" onClick={() => openGroupInfo(conversation)}>Members</button>
            : null}
          <div class="conversation-status"><StatusDot /><span>{statusLabel}</span></div>
        </header>

        <div class="delivery-banner">
          <LiveStatusIcon size="1rem" />
          <div>
            <strong>Local browser mode</strong>
            <span>Messages and attachments are saved on this device.</span>
          </div>
          <Badge value="Local" tone="warning" />
        </div>

        <div class="secure-note"><LockIcon size="0.8rem" /><span>Encrypted sync is not enabled in this browser build.</span></div>

        <section class="message-list" aria-live="polite"><Messages /></section>

        <form
          class={computed(() => `composer ${composerDragActive.value ? 'is-dragging' : ''}`)}
          onSubmit={sendPreviewMessage}
          onDragEnter={event => { event.preventDefault(); composerDragActive.value = true }}
          onDragOver={event => { event.preventDefault(); composerDragActive.value = true }}
          onDragLeave={event => { if (!event.currentTarget.contains(event.relatedTarget)) composerDragActive.value = false }}
          onDrop={handleComposerDrop}
        >
          {computed(() => pendingAttachment.value ? (
            <div class="composer-attachment">
              {pendingAttachment.value.kind === 'image'
                ? <img src={pendingAttachment.value.previewURL} alt="Selected attachment" />
                : <FileIcon />}
              <span><strong>{pendingAttachment.value.name}</strong><small>{formatBytes(pendingAttachment.value.size)}</small></span>
              <button type="button" aria-label="Remove attachment" onClick={clearPendingAttachment}><CloseIcon /></button>
            </div>
          ) : null)}
          <label class="attach-button" aria-label="Attach image or file" title="Attach image or file">
            <PaperclipIcon />
            <input type="file" onChange={event => { handleComposerFiles(event.currentTarget.files); event.currentTarget.value = '' }} />
          </label>
          <textarea
            value={composerText}
            onInput={event => { composerText.value = event.currentTarget.value }}
            onPaste={handleComposerPaste}
            onKeyDown={event => {
              if (event.key === 'Enter' && !event.shiftKey) {
                event.preventDefault()
                sendPreviewMessage(event)
              }
            }}
            placeholder={computed(() => pendingAttachment.value ? 'Add a caption' : 'Message')}
            aria-label={`Message ${conversation.title}`}
            rows="1"
          ></textarea>
          <Button type="submit" label="Send" showLabel={false} icon={<SendIcon />} ariaLabel="Save preview message or attachment" variant="primary" disabled={computed(() => !composerText.value.trim() && !pendingAttachment.value)} />
          <span class="drop-hint">Drop an image or file to attach</span>
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

function GroupContactPicker() {
  return computed(() => contacts.value.length ? (
    <div class="group-contact-picker">
      {contacts.value.map(contact => (
        <label class="group-contact-option">
          <input
            type="checkbox"
            checked={groupMemberDraft.value.includes(contact.userID)}
            onChange={() => toggleGroupMember(contact.userID)}
          />
          {contactPictures.value[contact.userID]
            ? <img class="profile-picture is-small" src={contactPictures.value[contact.userID]} alt="" />
            : <Avatar name={avatarName(contact.displayName || contact.handle)} size="small" />}
          <span><strong>{contact.displayName || `@${contact.handle}`}</strong><small>@{contact.handle}</small></span>
        </label>
      ))}
    </div>
  ) : <p class="sidebar-empty">Add contacts before creating a group.</p>)
}

function CreateGroupPopup() {
  return (
    <Popup
      open={createGroupOpen}
      title="New group"
      ariaDescription="Name the group and choose at least one contact."
      size="small"
      onClose={() => { createGroupOpen.value = false; groupError.value = '' }}
      footer={() => (
        <div class="popup-actions">
          <Button label="Cancel" variant="secondary" onClick={() => { createGroupOpen.value = false }} />
          <Button label="Create group" variant="primary" onClick={createPreviewGroup} />
        </div>
      )}
    >
      <form class="popup-form" onSubmit={createPreviewGroup}>
        <label for="group-name">Group name</label>
        <TextField id="group-name" value={groupNameDraft} placeholder="Launch crew" autocomplete="off" />
        <label>Members</label>
        <GroupContactPicker />
        {computed(() => groupError.value ? <Alert tone="error">{groupError}</Alert> : null)}
      </form>
    </Popup>
  )
}

function GroupInfoPopup() {
  return computed(() => {
    const conversation = selectedConversation.value
    if (!conversation?.isGroup) return null
    return (
      <Popup
        open={groupInfoOpen}
        title="Group details"
        ariaDescription="Rename the group or change its members."
        size="medium"
        onClose={() => { groupInfoOpen.value = false; groupError.value = '' }}
        footer={() => (
          <div class="popup-actions is-split">
            <Button label="Disband group" variant="tertiary" onClick={disbandSelectedGroup} />
            <span class="popup-actions">
              <Button label="Cancel" variant="secondary" onClick={() => { groupInfoOpen.value = false }} />
              <Button label="Save changes" variant="primary" onClick={saveGroupDetails} />
            </span>
          </div>
        )}
      >
        <form class="popup-form" onSubmit={saveGroupDetails}>
          <label for="group-details-name">Group name</label>
          <TextField id="group-details-name" value={groupNameDraft} autocomplete="off" />
          <label>Members</label>
          <div class="group-owner-row">
            <ProfilePicture size="small" />
            <span><strong>You</strong><small>@{normalizeHandle(profileHandle.value)} · Owner</small></span>
          </div>
          <GroupContactPicker />
          <p>Membership changes stay in this browser preview until the Web MLS group core is connected.</p>
          {computed(() => groupError.value ? <Alert tone="error">{groupError}</Alert> : null)}
        </form>
      </Popup>
    )
  })
}

function LogoutPopup() {
  return (
    <Popup
      open={logoutConfirmOpen}
      title="Log out?"
      ariaDescription="The current account session will be revoked. Local conversations and attachments will remain on this device."
      size="small"
      onClose={cancelLogoutConfirmation}
      footer={() => (
        <div class="popup-actions">
          <Button label="Cancel" variant="secondary" disabled={isLoggingOut} onClick={cancelLogoutConfirmation} />
          <Button label="Log out" variant="primary" loading={isLoggingOut} disabled={isLoggingOut} onClick={logoutAccount} />
        </div>
      )}
    >
      <p>Local conversations, contacts, profile information, and attachments will not be deleted.</p>
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
          <span class="popup-actions">
            <Button label="Log out" variant="tertiary" disabled={isLoggingOut} onClick={openLogoutConfirmation} />
            <Button label="Reset preview" variant="tertiary" onClick={resetPreview} />
          </span>
          <span class="popup-actions">
            <Button label="Save display name" variant="secondary" loading={isSavingDisplayName} disabled={isSavingDisplayName} onClick={saveProfileDisplayName} />
            <Button label="Change username" variant="primary" loading={isChangingUsername} disabled={isChangingUsername} onClick={changeProfileUsername} />
          </span>
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
      <CreateGroupPopup />
      <GroupInfoPopup />
      <RemoveContactPopup />
      <ProfilePopup />
      <LogoutPopup />
      {computed(() => notice.value && !newConversationOpen.value && !addContactOpen.value ? (
        <div class="toast" role="status">{notice}</div>
      ) : null)}
    </div>
  )
}

mount(<App />, document.querySelector('#app'))
