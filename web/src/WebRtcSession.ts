import {
  WebConnectionManager,
  WEB_MAX_FRAME_BYTES
} from './WebConnectionManager'
import type {
  WebConnectionState,
  WebConnectionTransport
} from './WebConnectionManager'
import {
  WebTransportConnectionManager,
  validateWebTransportEndpoint
} from './WebTransportConnectionManager'
import { requireCanonicalUUID } from './LinksWebClient'
import type { WebRtcSignalDelivery } from './LinksWebClient'
import type {
  WebCoreTransport,
  WebMessagingCore,
  WebReceivedTextMessage
} from './WebTextMessaging'
import {
  fingerprintWebRtcSFrameKey,
  WebRtcSFrameController,
  importWebRtcSFrameKey
} from './WebRtcSFrame'
import type {
  WebRtcSFrameControllerOptions,
  WebRtcSFrameControlKey,
  WebRtcSFrameEpochKey,
  WebRtcSFrameTransformError
} from './WebRtcSFrame'

export type WebRtcSignalKind = 'offer' | 'answer' | 'ice-candidate'
export type WebRtcSessionState = 'stopped' | 'connecting' | 'connected' | 'failed'

export interface WebRtcSignalingCodec {
  encodeWebRtcSignal(
    requestID: string,
    sessionID: string,
    targetDeviceID: string,
    kind: number,
    sdp: string,
    sdpMid?: string,
    sdpMLineIndex?: number
  ): Uint8Array
  isWebRtcSignalFrame(frame: Uint8Array): boolean
  decodeWebRtcSignal(frame: Uint8Array): WebRtcSignalDelivery
}

export interface WebRtcSessionOptions {
  endpoint: string
  /** Optional HTTPS/HTTP3 signaling endpoint used after WebSocket failure. */
  webTransportEndpoint?: string
  core: WebMessagingCore
  signaling: WebRtcSignalingCodec
  accessToken: () => string
  targetDeviceID: string
  sessionID?: string
  rtcConfiguration?: RTCConfiguration
  onState?: (state: WebRtcSessionState) => void
  onDataChannel?: (channel: RTCDataChannel) => void
  onTextMessage?: (message: WebReceivedTextMessage) => void
  onSFrameError?: (error: WebRtcSFrameTransformError) => void
  onFailure?: () => void
}

/** Authenticated browser WebRTC session with SDP and ICE signaling. */
export class WebRtcSession implements WebCoreTransport {
  static readonly maximumSdpBytes = 256 * 1024

  private sessionIDValue: string | null
  readonly targetDeviceID: string
  private readonly endpoint: string
  private readonly webTransportEndpoint: string | null
  private readonly core: WebMessagingCore
  private readonly signaling: WebRtcSignalingCodec
  private readonly accessToken: () => string
  private readonly rtcConfiguration?: RTCConfiguration
  private readonly onStateCallback: (state: WebRtcSessionState) => void
  private readonly onDataChannelCallback: (channel: RTCDataChannel) => void
  private readonly onTextMessageCallback: (message: WebReceivedTextMessage) => void
  private readonly onFailureCallback: () => void
  private manager: WebConnectionTransport | null = null
  private usingWebTransport = false
  private peer: RTCPeerConnection | null = null
  private pendingCandidates: RTCIceCandidateInit[] = []
  private currentState: WebRtcSessionState = 'stopped'
  private coreFailed = false
  private readonly onSFrameErrorCallback: (error: WebRtcSFrameTransformError) => void
  private sframeController: WebRtcSFrameController | null = null
  private readonly sframeControlKeys = new Map<
    string,
    { key: CryptoKey
      fingerprint: string }
  >()

  constructor(options: WebRtcSessionOptions) {
    if (typeof options.endpoint !== 'string' || options.core === null ||
        options.signaling === null || typeof options.accessToken !== 'function') {
      throw new Error('Invalid WebRTC session')
    }
    requireCanonicalUUID(options.core.deviceID, 'device ID')
    requireCanonicalUUID(options.targetDeviceID, 'target device ID')
    const sessionID = options.sessionID ?? null
    if (sessionID !== null) requireCanonicalUUID(sessionID, 'WebRTC session ID')
    this.sessionIDValue = sessionID
    this.targetDeviceID = options.targetDeviceID
    this.endpoint = options.endpoint
    this.webTransportEndpoint = options.webTransportEndpoint ?? null
    if (this.webTransportEndpoint !== null) {
      validateWebTransportEndpoint(this.webTransportEndpoint)
    }
    this.core = options.core
    this.signaling = options.signaling
    this.accessToken = options.accessToken
    this.rtcConfiguration = options.rtcConfiguration
    this.onStateCallback = options.onState ?? (() => {})
    this.onDataChannelCallback = options.onDataChannel ?? (() => {})
    this.onTextMessageCallback = options.onTextMessage ?? (() => {})
    this.onSFrameErrorCallback = options.onSFrameError ?? (() => {})
    this.onFailureCallback = options.onFailure ?? (() => {})
  }

  get state(): WebRtcSessionState {
    return this.currentState
  }

  get sessionID(): string | null {
    return this.sessionIDValue
  }

  get isConnected(): boolean {
    return this.currentState === 'connected' && this.peer?.connectionState === 'connected'
  }

  get peerConnection(): RTCPeerConnection | null {
    return this.peer
  }

  get sframe(): WebRtcSFrameController | null {
    return this.sframeController
  }

  /** Enable native SFrame before offer/answer negotiation. */
  enableSFrame(options: WebRtcSFrameControllerOptions = {}): WebRtcSFrameController {
    const peer = this.peer ?? this.createPeer(false)
    if (this.sframeController === null) {
      this.sframeController = new WebRtcSFrameController(peer, {
        ...options,
        onError: error => this.notify(() => {
          options.onError?.(error)
          this.onSFrameErrorCallback(error)
        })
      })
    }
    return this.sframeController
  }

  /** Install the current MLS epoch media key into all attached transforms. */
  async installSFrameKey(epochKey: WebRtcSFrameEpochKey): Promise<void> {
    await this.enableSFrame().installKey(epochKey)
  }

  /** Install a decrypted MLS SFrame control update for this peer session. */
  async installSFrameControlKey(update: WebRtcSFrameControlKey): Promise<void> {
    requireCanonicalUUID(update.mediaSessionID, 'SFrame media session ID')
    if (this.sessionIDValue === null || update.mediaSessionID !== this.sessionIDValue) {
      throw new Error('SFrame media session mismatch')
    }
    const keyIdentity = `${update.mediaSessionID}:${String(update.keyID)}:${String(update.epoch)}`
    const fingerprint = await fingerprintWebRtcSFrameKey(update.key)
    const cached = this.sframeControlKeys.get(keyIdentity)
    if (cached !== undefined && cached.fingerprint !== fingerprint) {
      throw new Error('SFrame control key changed during replay')
    }
    const key = cached?.key ?? await importWebRtcSFrameKey(update.key)
    if (cached === undefined) this.sframeControlKeys.set(keyIdentity, { key, fingerprint })
    await this.installSFrameKey({
      key,
      keyID: update.keyID,
      epoch: update.epoch
    })
  }

  /** Attach SFrame to all media transceivers created so far. */
  async attachSFrameTransforms(): Promise<void> {
    if (this.sframeController === null) throw new Error('SFrame is not enabled')
    await this.sframeController.attachTransceivers()
  }

  start(): void {
    if (this.manager !== null || this.coreFailed) return
    let manager: WebConnectionManager
    manager = new WebConnectionManager({
      endpoint: this.endpoint,
      helloProvider: () => this.createHello(),
      onFrame: frame => this.handleFrame(manager, frame),
      onState: state => this.handleManagerState(manager, state),
      onFailure: () => this.handleFailure(manager)
    })
    this.manager = manager
    this.usingWebTransport = false
    this.coreFailed = false
    this.setState('connecting')
    manager.start()
  }

  stop(): void {
    const manager = this.manager
    this.manager = null
    this.coreFailed = false
    this.closePeer()
    manager?.stop()
    this.setState('stopped')
  }

  shutdown(): void {
    const manager = this.manager
    this.manager = null
    this.coreFailed = false
    this.closePeer()
    manager?.shutdown()
    this.setState('stopped')
  }

  send(frame: Uint8Array): boolean {
    return this.manager?.send(frame) === true
  }

  /** Call after the WebSocket or WebTransport signaling path reaches ready. */
  async startOffer(dataChannelLabel?: string): Promise<RTCDataChannel | null> {
    if (!this.isSignalingReady()) throw new Error('WebRTC signaling is not connected')
    if (this.sessionIDValue === null) this.sessionIDValue = crypto.randomUUID()
    const peer = this.createPeer(true)
    const channel = dataChannelLabel === undefined
      ? null
      : peer.createDataChannel(dataChannelLabel, { ordered: true })
    if (this.sframeController !== null) {
      await this.sframeController.attachTransceivers()
    }
    const offer = await peer.createOffer()
    await peer.setLocalDescription(offer)
    const description = peer.localDescription
    if (description?.type !== 'offer' || description.sdp.length === 0) {
      throw new Error('WebRTC offer was not created')
    }
    this.sendSignal('offer', description.sdp)
    return channel
  }

  async handleSignal(delivery: WebRtcSignalDelivery): Promise<void> {
    requireCanonicalUUID(delivery.requestID, 'WebRTC request ID')
    requireCanonicalUUID(delivery.senderDeviceID, 'sender device ID')
    requireCanonicalUUID(delivery.sessionID, 'WebRTC session ID')
    requireCanonicalUUID(delivery.targetDeviceID, 'target device ID')
    if (delivery.senderDeviceID !== this.targetDeviceID ||
        delivery.targetDeviceID !== this.core.deviceID) {
      throw new Error('WebRTC signal peer mismatch')
    }
    if (this.sessionIDValue === null && delivery.kind === 1) {
      this.sessionIDValue = delivery.sessionID
    }
    if (this.sessionIDValue === null || delivery.sessionID !== this.sessionIDValue ||
        delivery.sdp.length === 0 ||
        new TextEncoder().encode(delivery.sdp).byteLength > WebRtcSession.maximumSdpBytes) {
      throw new Error('WebRTC signal does not match this session')
    }
    if (delivery.kind === 1) {
      await this.acceptOffer(delivery.sdp)
    } else if (delivery.kind === 2) {
      await this.acceptAnswer(delivery.sdp)
    } else if (delivery.kind === 3) {
      await this.acceptCandidate(delivery)
    } else {
      throw new Error('Unknown WebRTC signal')
    }
  }

  private async acceptOffer(sdp: string): Promise<void> {
    const peer = this.createPeer(false)
    await peer.setRemoteDescription({ type: 'offer', sdp })
    if (this.sframeController !== null) {
      await this.sframeController.attachTransceivers()
    }
    await this.flushCandidates(peer)
    const answer = await peer.createAnswer()
    await peer.setLocalDescription(answer)
    const description = peer.localDescription
    if (description?.type !== 'answer' || description.sdp.length === 0) {
      throw new Error('WebRTC answer was not created')
    }
    this.sendSignal('answer', description.sdp)
  }

  private async acceptAnswer(sdp: string): Promise<void> {
    const peer = this.peer
    if (peer === null || peer.signalingState !== 'have-local-offer') {
      throw new Error('Unexpected WebRTC answer')
    }
    await peer.setRemoteDescription({ type: 'answer', sdp })
    if (this.sframeController !== null) {
      await this.sframeController.attachTransceivers()
    }
    await this.flushCandidates(peer)
  }

  private async acceptCandidate(delivery: WebRtcSignalDelivery): Promise<void> {
    const candidate: RTCIceCandidateInit = {
      candidate: delivery.sdp,
      sdpMid: delivery.sdpMid || null,
      sdpMLineIndex: delivery.sdpMLineIndex
    }
    const peer = this.peer
    if (peer === null || peer.remoteDescription === null) {
      this.pendingCandidates.push(candidate)
      return
    }
    await peer.addIceCandidate(candidate)
  }

  private async flushCandidates(peer: RTCPeerConnection): Promise<void> {
    const candidates = this.pendingCandidates.splice(0)
    for (const candidate of candidates) await peer.addIceCandidate(candidate)
  }

  private createPeer(offerer: boolean): RTCPeerConnection {
    if (this.peer !== null) return this.peer
    if (typeof RTCPeerConnection === 'undefined') throw new Error('WebRTC unavailable')
    const peer = new RTCPeerConnection(this.rtcConfiguration)
    peer.onicecandidate = event => {
      if (event.candidate === null) return
      try {
        this.sendSignal(
          'ice-candidate',
          event.candidate.candidate,
          event.candidate.sdpMid ?? '',
          event.candidate.sdpMLineIndex ?? 0
        )
      } catch {
        this.fail()
      }
    }
    peer.ondatachannel = event => this.notify(() => this.onDataChannelCallback(event.channel))
    peer.onconnectionstatechange = () => {
      if (peer.connectionState === 'connected') this.setState('connected')
      else if (peer.connectionState === 'failed') this.fail()
      else if (peer.connectionState === 'connecting') this.setState('connecting')
    }
    this.peer = peer
    if (!offerer) this.setState('connecting')
    return peer
  }

  private sendSignal(
    kind: WebRtcSignalKind,
    sdp: string,
    sdpMid = '',
    sdpMLineIndex = 0
  ): void {
    if (sdp.length === 0 || new TextEncoder().encode(sdp).byteLength > WebRtcSession.maximumSdpBytes) {
      throw new Error('WebRTC signal is too large')
    }
    const kindNumber = kind === 'offer' ? 1 : kind === 'answer' ? 2 : 3
    const frame = this.signaling.encodeWebRtcSignal(
      crypto.randomUUID(),
      this.sessionIDValue ?? (() => { throw new Error('WebRTC session ID unavailable') })(),
      this.targetDeviceID,
      kindNumber,
      sdp,
      sdpMid,
      sdpMLineIndex
    )
    if (!(frame instanceof Uint8Array) || frame.byteLength === 0 ||
        frame.byteLength > WEB_MAX_FRAME_BYTES || !this.send(frame)) {
      throw new Error('WebRTC signal could not be sent')
    }
  }

  private handleFrame(manager: WebConnectionTransport, frame: Uint8Array): void {
    if (this.manager !== manager || this.coreFailed) return
    if (this.signaling.isWebRtcSignalFrame(frame)) {
      try {
        void this.handleSignal(this.signaling.decodeWebRtcSignal(frame)).catch(() => this.fail())
      } catch {
        this.fail()
      }
      return
    }
    try {
      this.core.handleServerFrame(frame, manager, false, message => {
        this.notify(() => this.onTextMessageCallback(message))
      })
    } catch {
      this.fail()
    }
  }

  private createHello(): Uint8Array {
    const token = this.accessToken()
    if (typeof token !== 'string' || token.length === 0) {
      throw new Error('Authenticated Web session required')
    }
    const hello = this.core.createHello(token, this.core.durableCursor())
    if (!(hello instanceof Uint8Array) || hello.byteLength === 0 ||
        hello.byteLength > WEB_MAX_FRAME_BYTES) {
      throw new Error('Invalid Web Hello frame')
    }
    return hello
  }

  private isSignalingReady(): boolean {
    return this.manager?.isConnected === true && this.currentState !== 'failed'
  }

  private handleManagerState(manager: WebConnectionTransport, state: WebConnectionState): void {
    if (this.manager !== manager) return
    if (state === 'failed') {
      if (!this.usingWebTransport && this.webTransportEndpoint !== null) {
        this.setState('connecting')
      } else {
        this.setState('failed')
      }
    }
    else if (state === 'connecting') this.setState('connecting')
  }

  private handleFailure(manager: WebConnectionTransport): void {
    if (this.manager !== manager || this.coreFailed) return
    if (!this.usingWebTransport && this.webTransportEndpoint !== null) {
      this.startWebTransportFallback(manager)
      return
    }
    this.fail()
  }

  private startWebTransportFallback(primary: WebConnectionTransport): void {
    const endpoint = this.webTransportEndpoint
    if (endpoint === null || this.manager !== primary || this.usingWebTransport) {
      this.fail()
      return
    }
    this.usingWebTransport = true
    let fallback: WebTransportConnectionManager
    try {
      let manager: WebTransportConnectionManager
      manager = new WebTransportConnectionManager({
        endpoint,
        helloProvider: () => this.createHello(),
        onFrame: frame => this.handleFrame(manager, frame),
        onState: state => this.handleManagerState(manager, state),
        onFailure: () => this.handleFailure(manager)
      })
      fallback = manager
    } catch {
      this.fail()
      return
    }
    this.manager = fallback
    primary.stop()
    this.setState('connecting')
    fallback.start()
  }

  private fail(): void {
    if (this.coreFailed) return
    this.coreFailed = true
    this.setState('failed')
    this.manager?.stop()
    this.notify(this.onFailureCallback)
  }

  private closePeer(): void {
    const peer = this.peer
    this.peer = null
    this.pendingCandidates = []
    this.sframeController?.close()
    this.sframeController = null
    this.sframeControlKeys.clear()
    peer?.close()
  }

  private setState(state: WebRtcSessionState): void {
    this.currentState = state
    this.notify(() => this.onStateCallback(state))
  }

  private notify(callback: () => void): void {
    try {
      callback()
    } catch {
      // Host callbacks must not break signaling or core state.
    }
  }
}
