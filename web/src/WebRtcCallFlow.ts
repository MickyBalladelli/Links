import {
  importWebRtcSFrameKey,
  supportsWebRtcSFrame,
  WebRtcSFrameController
} from './WebRtcSFrame'
import type { WebRtcSFrameTransformError } from './WebRtcSFrame'
import { requireCanonicalUUID } from './LinksWebClient'

const MAX_SFU_TOKEN_BYTES = 4096
const MAX_SFU_ROOM_NAME_BYTES = 128
const MIN_SFU_ROOM_NAME_BYTES = 16
const MAX_SIGNAL_BYTES = 256 * 1024
const SFRAME_KEY_BYTES = 16
const MAX_SFRAME_KEY_ID = 0xffff_ffff_ffff_ffffn

export type WebRtcCallState =
  | 'idle'
  | 'preparing'
  | 'joining'
  | 'negotiating'
  | 'connected'
  | 'ended'
  | 'failed'

export interface WebRtcSfuPlacement {
  roomName: string
  region: string
  endpoint: string
  /** Short-lived room token. This is not a LiveKit API secret. */
  accessToken: string
  requireSFrame: boolean
}

export type WebRtcSfuSignalKind = 'offer' | 'answer' | 'ice-candidate'

export interface WebRtcSfuSignal {
  sessionID: string
  kind: WebRtcSfuSignalKind
  sdp: string
  sdpMid?: string
  sdpMLineIndex?: number
}

export interface WebRtcSfuJoinRequest {
  placement: WebRtcSfuPlacement
  sessionID: string
  mediaSessionID: string
}

/** Provider adapter for LiveKit/Mediasoup signaling. It never receives MLS keys. */
export interface WebRtcSfuSignaling {
  join(request: WebRtcSfuJoinRequest): Promise<void>
  send(signal: WebRtcSfuSignal): Promise<void> | void
  subscribe(listener: (signal: WebRtcSfuSignal) => void): () => void
  leave(): Promise<void> | void
}

export interface WebRtcMlsEpochKeyMaterial {
  mediaSessionID: string
  keyID: bigint | number
  epoch: bigint | number
  /** Raw key is consumed immediately by the MLS adapter and then wiped here. */
  key: BufferSource
}

/** MLS host boundary for initial key publication and authenticated rotations. */
export interface WebRtcMlsMediaKeyExchange {
  createInitialKey(mediaSessionID: string): Promise<WebRtcMlsEpochKeyMaterial>
  publishEpochKey(update: WebRtcMlsEpochKeyMaterial): Promise<void>
  subscribe(listener: (update: WebRtcMlsEpochKeyMaterial) => void): () => void
}

export interface WebRtcCallTrack {
  track: MediaStreamTrack
  streams?: readonly MediaStream[]
}

export interface WebRtcCallFlowOptions {
  placement: WebRtcSfuPlacement
  signaling: WebRtcSfuSignaling
  mls: WebRtcMlsMediaKeyExchange
  sessionID?: string
  mediaSessionID?: string
  rtcConfiguration?: RTCConfiguration
  localTracks?: readonly WebRtcCallTrack[]
  /** Add receive-only m-lines so an SFU can send the selected media kinds. */
  receiveKinds?: readonly ('audio' | 'video')[]
  onState?: (state: WebRtcCallState) => void
  onTrack?: (event: RTCTrackEvent) => void
  onSFrameError?: (error: WebRtcSFrameTransformError) => void
  onFailure?: () => void
}

/** Complete browser call orchestration against a managed SFU adapter. */
export class WebRtcCallFlow {
  private readonly placement: WebRtcSfuPlacement
  private readonly signaling: WebRtcSfuSignaling
  private readonly mls: WebRtcMlsMediaKeyExchange
  private readonly sessionIDValue: string
  private readonly mediaSessionIDValue: string
  private readonly rtcConfiguration?: RTCConfiguration
  private readonly localTracks: readonly WebRtcCallTrack[]
  private readonly receiveKinds: readonly ('audio' | 'video')[]
  private readonly onStateCallback: (state: WebRtcCallState) => void
  private readonly onTrackCallback: (event: RTCTrackEvent) => void
  private readonly onSFrameErrorCallback: (error: WebRtcSFrameTransformError) => void
  private readonly onFailureCallback: () => void

  private peer: RTCPeerConnection | null = null
  private pendingCandidates: RTCIceCandidateInit[] = []
  private sframeController: WebRtcSFrameController | null = null
  private unsubscribeSignals: (() => void) | null = null
  private unsubscribeMls: (() => void) | null = null
  private keyQueue: Promise<void> = Promise.resolve()
  private currentState: WebRtcCallState = 'idle'
  private failed = false
  private joined = false

  constructor(options: WebRtcCallFlowOptions) {
    validatePlacement(options.placement)
    if (options.signaling === null || options.mls === null ||
        typeof options.signaling.join !== 'function' ||
        typeof options.signaling.send !== 'function' ||
        typeof options.signaling.subscribe !== 'function' ||
        typeof options.signaling.leave !== 'function' ||
        typeof options.mls.createInitialKey !== 'function' ||
        typeof options.mls.publishEpochKey !== 'function' ||
        typeof options.mls.subscribe !== 'function') {
      throw new Error('Invalid WebRTC SFU call adapters')
    }
    if (!supportsWebRtcSFrame()) throw new Error('SFrame is required for SFU calls')

    this.placement = options.placement
    this.signaling = options.signaling
    this.mls = options.mls
    this.sessionIDValue = requireCanonicalUUID(
      options.sessionID ?? crypto.randomUUID(), 'WebRTC call session ID')
    this.mediaSessionIDValue = requireCanonicalUUID(
      options.mediaSessionID ?? this.sessionIDValue, 'WebRTC media session ID')
    this.rtcConfiguration = options.rtcConfiguration
    this.localTracks = options.localTracks ?? []
    this.receiveKinds = options.receiveKinds ?? ['audio', 'video']
    this.onStateCallback = options.onState ?? (() => {})
    this.onTrackCallback = options.onTrack ?? (() => {})
    this.onSFrameErrorCallback = options.onSFrameError ?? (() => {})
    this.onFailureCallback = options.onFailure ?? (() => {})
    validateTracks(this.localTracks)
    validateReceiveKinds(this.receiveKinds)
  }

  get state(): WebRtcCallState {
    return this.currentState
  }

  get sessionID(): string {
    return this.sessionIDValue
  }

  get mediaSessionID(): string {
    return this.mediaSessionIDValue
  }

  get roomName(): string {
    return this.placement.roomName
  }

  get region(): string {
    return this.placement.region
  }

  get peerConnection(): RTCPeerConnection | null {
    return this.peer
  }

  get sframe(): WebRtcSFrameController | null {
    return this.sframeController
  }

  /** Run MLS key setup, SFrame setup, SFU join, and SDP offer exchange. */
  async start(): Promise<void> {
    if (this.currentState !== 'idle') throw new Error('WebRTC call already started')
    this.setState('preparing')
    try {
      this.createPeer()
      await this.prepareMediaKey()
      this.subscribeToControl()
      this.setState('joining')
      this.joined = true
      await this.signaling.join({
        placement: this.placement,
        sessionID: this.sessionIDValue,
        mediaSessionID: this.mediaSessionIDValue
      })
      this.setState('negotiating')
      await this.createAndSendOffer()
    } catch (error) {
      this.fail()
      throw error
    }
  }

  /** Publish and install a new MLS-authenticated SFrame epoch key. */
  async publishSFrameEpochKey(material: WebRtcMlsEpochKeyMaterial): Promise<void> {
    validateKeyMaterial(material, this.mediaSessionIDValue)
    if (this.failed || this.currentState === 'ended') throw new Error('WebRTC call is closed')
    await this.enqueueKey(async () => {
      await this.installKey(material)
      const key = copyKeyBytes(material.key)
      try {
        await this.mls.publishEpochKey({
          mediaSessionID: material.mediaSessionID,
          keyID: material.keyID,
          epoch: material.epoch,
          key
        })
      } finally {
        key.fill(0)
      }
    })
  }

  /** Apply an MLS-authenticated SFrame update delivered by the host. */
  async installSFrameEpochKey(material: WebRtcMlsEpochKeyMaterial): Promise<void> {
    validateKeyMaterial(material, this.mediaSessionIDValue)
    if (this.failed || this.currentState === 'ended') throw new Error('WebRTC call is closed')
    await this.enqueueKey(() => this.installKey(material))
  }

  async handleSignal(signal: WebRtcSfuSignal): Promise<void> {
    validateSignal(signal, this.sessionIDValue)
    const peer = this.peer
    if (peer === null || !this.joined) throw new Error('WebRTC SFU call is not joined')

    if (signal.kind === 'offer') {
      await peer.setRemoteDescription({ type: 'offer', sdp: signal.sdp })
      await this.sframeController?.attachTransceivers()
      await this.flushCandidates(peer)
      const answer = await peer.createAnswer()
      await peer.setLocalDescription(answer)
      const description = peer.localDescription
      if (description?.type !== 'answer' || description.sdp.length === 0) {
        throw new Error('WebRTC SFU answer was not created')
      }
      await this.sendSignal({
        sessionID: this.sessionIDValue,
        kind: 'answer',
        sdp: description.sdp
      })
    } else if (signal.kind === 'answer') {
      if (peer.signalingState !== 'have-local-offer') {
        throw new Error('Unexpected WebRTC SFU answer')
      }
      await peer.setRemoteDescription({ type: 'answer', sdp: signal.sdp })
      await this.sframeController?.attachTransceivers()
      await this.flushCandidates(peer)
    } else {
      const candidate: RTCIceCandidateInit = {
        candidate: signal.sdp,
        sdpMid: signal.sdpMid ?? null,
        sdpMLineIndex: signal.sdpMLineIndex
      }
      if (peer.remoteDescription === null) this.pendingCandidates.push(candidate)
      else await peer.addIceCandidate(candidate)
    }
  }

  async stop(): Promise<void> {
    if (this.currentState === 'ended') return
    this.failed = false
    this.joined = false
    this.unsubscribeSignals?.()
    this.unsubscribeSignals = null
    this.unsubscribeMls?.()
    this.unsubscribeMls = null
    try {
      await this.signaling.leave()
    } finally {
      this.sframeController?.close()
      this.sframeController = null
      this.peer?.close()
      this.peer = null
      this.setState('ended')
    }
  }

  private async prepareMediaKey(): Promise<void> {
    const initial = await this.mls.createInitialKey(this.mediaSessionIDValue)
    validateKeyMaterial(initial, this.mediaSessionIDValue)
    await this.installKey(initial)
    const key = copyKeyBytes(initial.key)
    try {
      await this.mls.publishEpochKey({
        mediaSessionID: initial.mediaSessionID,
        keyID: initial.keyID,
        epoch: initial.epoch,
        key
      })
    } finally {
      key.fill(0)
    }
  }

  private createPeer(): void {
    if (typeof RTCPeerConnection === 'undefined') throw new Error('WebRTC unavailable')
    const peer = new RTCPeerConnection(this.rtcConfiguration)
    const controller = new WebRtcSFrameController(peer, {
      onError: error => this.notify(() => this.onSFrameErrorCallback(error))
    })
    this.peer = peer
    this.sframeController = controller
    peer.onicecandidate = event => {
      if (event.candidate === null || !this.joined) return
      void this.sendSignal({
        sessionID: this.sessionIDValue,
        kind: 'ice-candidate',
        sdp: event.candidate.candidate,
        sdpMid: event.candidate.sdpMid ?? undefined,
        sdpMLineIndex: event.candidate.sdpMLineIndex ?? undefined
      }).catch(() => this.fail())
    }
    peer.ontrack = event => {
      void this.handleTrack(event).catch(() => this.fail())
    }
    peer.onconnectionstatechange = () => {
      if (peer.connectionState === 'connected') this.setState('connected')
      else if (peer.connectionState === 'failed' || peer.connectionState === 'closed') this.fail()
      else if (peer.connectionState === 'connecting') this.setState('negotiating')
    }
    for (const local of this.localTracks) {
      peer.addTransceiver(local.track, {
        direction: 'sendonly',
        streams: local.streams === undefined ? [] : Array.from(local.streams)
      })
    }
    for (const kind of this.receiveKinds) {
      peer.addTransceiver(kind, { direction: 'recvonly' })
    }
  }

  private subscribeToControl(): void {
    this.unsubscribeSignals = this.signaling.subscribe(signal => {
      void this.handleSignal(signal).catch(() => this.fail())
    })
    this.unsubscribeMls = this.mls.subscribe(update => {
      void this.installSFrameEpochKey(update).catch(() => this.fail())
    })
  }

  private async createAndSendOffer(): Promise<void> {
    const peer = this.peer
    if (peer === null || this.sframeController === null) throw new Error('WebRTC peer unavailable')
    await this.sframeController.attachTransceivers()
    const offer = await peer.createOffer()
    await peer.setLocalDescription(offer)
    const description = peer.localDescription
    if (description?.type !== 'offer' || description.sdp.length === 0) {
      throw new Error('WebRTC SFU offer was not created')
    }
    await this.sendSignal({
      sessionID: this.sessionIDValue,
      kind: 'offer',
      sdp: description.sdp
    })
  }

  private async flushCandidates(peer: RTCPeerConnection): Promise<void> {
    const candidates = this.pendingCandidates.splice(0)
    for (const candidate of candidates) await peer.addIceCandidate(candidate)
  }

  private async sendSignal(signal: WebRtcSfuSignal): Promise<void> {
    if (!this.joined) throw new Error('WebRTC SFU call is not joined')
    validateSignal(signal, this.sessionIDValue)
    await this.signaling.send(signal)
  }

  private async handleTrack(event: RTCTrackEvent): Promise<void> {
    const controller = this.sframeController
    if (controller === null) throw new Error('SFrame controller unavailable')
    await controller.attachReceiver(event.receiver)
    this.notify(() => this.onTrackCallback(event))
  }

  private async installKey(material: WebRtcMlsEpochKeyMaterial): Promise<void> {
    const controller = this.sframeController
    if (controller === null) throw new Error('SFrame controller unavailable')
    const keyBytes = copyKeyBytes(material.key)
    try {
      const key = await importWebRtcSFrameKey(keyBytes)
      await controller.installKey({
        key,
        keyID: material.keyID,
        epoch: material.epoch
      })
    } finally {
      keyBytes.fill(0)
    }
  }

  private enqueueKey(operation: () => Promise<void>): Promise<void> {
    const next = this.keyQueue.then(operation)
    this.keyQueue = next.catch(() => {})
    return next
  }

  private fail(): void {
    if (this.failed || this.currentState === 'ended') return
    this.failed = true
    this.setState('failed')
    this.unsubscribeSignals?.()
    this.unsubscribeSignals = null
    this.unsubscribeMls?.()
    this.unsubscribeMls = null
    this.pendingCandidates = []
    this.peer?.close()
    void Promise.resolve(this.signaling.leave()).catch(() => {})
    this.notify(this.onFailureCallback)
  }

  private setState(state: WebRtcCallState): void {
    this.currentState = state
    this.notify(() => this.onStateCallback(state))
  }

  private notify(callback: () => void): void {
    try {
      callback()
    } catch {
      // Host callbacks must not break call state or key installation.
    }
  }
}

function validatePlacement(placement: WebRtcSfuPlacement): void {
  if (placement === null || placement.requireSFrame !== true ||
      !validOpaqueRoomName(placement.roomName) ||
      !validRegion(placement.region) || !validSfuEndpoint(placement.endpoint) ||
      typeof placement.accessToken !== 'string' || placement.accessToken.length === 0 ||
      new TextEncoder().encode(placement.accessToken).byteLength > MAX_SFU_TOKEN_BYTES) {
    throw new Error('Invalid WebRTC SFU placement')
  }
}

function validateTracks(tracks: readonly WebRtcCallTrack[]): void {
  if (tracks.length > 8) throw new Error('Too many WebRTC call tracks')
  for (const item of tracks) {
    if (item === null || item.track === null ||
        (item.track.kind !== 'audio' && item.track.kind !== 'video')) {
      throw new Error('Invalid WebRTC call track')
    }
    if (item.streams !== undefined && item.streams.length > 4) {
      throw new Error('Too many WebRTC call streams')
    }
  }
}

function validateReceiveKinds(kinds: readonly ('audio' | 'video')[]): void {
  if (kinds.length > 2 || new Set(kinds).size !== kinds.length) {
    throw new Error('Invalid WebRTC receive kinds')
  }
}

function validateSignal(signal: WebRtcSfuSignal, sessionID: string): void {
  requireCanonicalUUID(signal.sessionID, 'WebRTC signal session ID')
  if (signal.sessionID !== sessionID ||
      (signal.kind !== 'offer' && signal.kind !== 'answer' && signal.kind !== 'ice-candidate') ||
      typeof signal.sdp !== 'string' || signal.sdp.length === 0 ||
      new TextEncoder().encode(signal.sdp).byteLength > MAX_SIGNAL_BYTES ||
      signal.sdpMid !== undefined && new TextEncoder().encode(signal.sdpMid).byteLength > 256 ||
      signal.sdpMLineIndex !== undefined &&
        (!Number.isInteger(signal.sdpMLineIndex) || signal.sdpMLineIndex < 0)) {
    throw new Error('Invalid WebRTC SFU signal')
  }
}

function validateKeyMaterial(
  material: WebRtcMlsEpochKeyMaterial,
  expectedMediaSessionID: string
): void {
  requireCanonicalUUID(material.mediaSessionID, 'SFrame media session ID')
  if (material.mediaSessionID !== expectedMediaSessionID) throw new Error('SFrame media session mismatch')
  normalizeKeyID(material.keyID, 'SFrame key ID')
  normalizeKeyID(material.epoch, 'SFrame epoch')
  const key = copyKeyBytes(material.key)
  if (key.byteLength !== SFRAME_KEY_BYTES || key.every(byte => byte === 0)) {
    key.fill(0)
    throw new Error('Invalid SFrame key material')
  }
  key.fill(0)
}

function copyKeyBytes(source: BufferSource): Uint8Array {
  const bytes = source instanceof ArrayBuffer
    ? new Uint8Array(source)
    : new Uint8Array(source.buffer, source.byteOffset, source.byteLength)
  return bytes.slice()
}

function normalizeKeyID(value: bigint | number, field: string): bigint {
  const normalized = typeof value === 'bigint'
    ? value
    : Number.isSafeInteger(value) ? BigInt(value) : -1n
  if (normalized < 0n || normalized > MAX_SFRAME_KEY_ID) throw new Error(`Invalid ${field}`)
  return normalized
}

function validOpaqueRoomName(value: string): boolean {
  return typeof value === 'string' &&
    value.length >= MIN_SFU_ROOM_NAME_BYTES && value.length <= MAX_SFU_ROOM_NAME_BYTES &&
    /^[A-Za-z0-9_.-]+$/u.test(value)
}

function validRegion(value: string): boolean {
  return typeof value === 'string' && /^[A-Za-z0-9_.:-]{1,128}$/u.test(value)
}

function validSfuEndpoint(value: string): boolean {
  let parsed: URL
  try {
    parsed = new URL(value)
  } catch {
    return false
  }
  return parsed.protocol === 'wss:' && parsed.hostname.length > 0 &&
    parsed.username.length === 0 && parsed.password.length === 0 &&
    parsed.search.length === 0 && parsed.hash.length === 0
}
