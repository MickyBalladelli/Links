import {
  WebRtcCallFlow,
  type WebRtcCallFlowOptions,
  type WebRtcCallState,
  type WebRtcCallTrack,
  type WebRtcMlsEpochKeyMaterial,
  type WebRtcSfuSignal
} from './WebRtcCallFlow'

export type WebCallMode = 'voice' | 'video' | 'live-stream'
export type WebCallRole = 'participant' | 'publisher' | 'subscriber'

export interface WebCallSurfaceOptions extends Omit<WebRtcCallFlowOptions, 'receiveKinds'> {
  mode: WebCallMode
  role?: WebCallRole
  receiveKinds?: readonly ('audio' | 'video')[]
}

/**
 * Web call surface for voice/video calls and live streams. The existing
 * WebRtcCallFlow owns MLS/SFrame/SDP ordering; this class adds user-facing
 * mode and publisher/subscriber policy without exposing media keys to UI code.
 */
export class WebCallSurface {
  readonly mode: WebCallMode
  readonly role: WebCallRole
  readonly flow: WebRtcCallFlow

  constructor(options: WebCallSurfaceOptions) {
    const role = options.role ?? 'participant'
    const { mode, receiveKinds, ...flowOptions } = options
    const localTracks = flowOptions.localTracks ?? []
    validateSurface(mode, role, localTracks, receiveKinds)
    this.mode = mode
    this.role = role
    this.flow = new WebRtcCallFlow({
      ...flowOptions,
      localTracks,
      receiveKinds: receiveKinds ?? defaultReceiveKinds(mode, role)
    })
  }

  get state(): WebRtcCallState {
    return this.flow.state
  }

  get sessionID(): string {
    return this.flow.sessionID
  }

  get mediaSessionID(): string {
    return this.flow.mediaSessionID
  }

  get peerConnection(): RTCPeerConnection | null {
    return this.flow.peerConnection
  }

  get sframe() {
    return this.flow.sframe
  }

  start(): Promise<void> {
    return this.flow.start()
  }

  stop(): Promise<void> {
    return this.flow.stop()
  }

  handleSignal(signal: WebRtcSfuSignal): Promise<void> {
    return this.flow.handleSignal(signal)
  }

  publishSFrameEpochKey(material: WebRtcMlsEpochKeyMaterial): Promise<void> {
    return this.flow.publishSFrameEpochKey(material)
  }

  installSFrameEpochKey(material: WebRtcMlsEpochKeyMaterial): Promise<void> {
    return this.flow.installSFrameEpochKey(material)
  }
}

function defaultReceiveKinds(mode: WebCallMode, role: WebCallRole): readonly ('audio' | 'video')[] {
  if (mode === 'voice') return ['audio']
  if (mode === 'live-stream' && role === 'publisher') return []
  return ['audio', 'video']
}

function validateSurface(
  mode: WebCallMode,
  role: WebCallRole,
  localTracks: readonly WebRtcCallTrack[],
  receiveKinds: readonly ('audio' | 'video')[] | undefined
): void {
  if (mode !== 'voice' && mode !== 'video' && mode !== 'live-stream') {
    throw new Error('Invalid Web call mode')
  }
  if (role !== 'participant' && role !== 'publisher' && role !== 'subscriber') {
    throw new Error('Invalid Web call role')
  }
  if (mode !== 'live-stream' && role !== 'participant') {
    throw new Error('Publisher and subscriber roles require a live stream')
  }
  if (role === 'subscriber' && localTracks.length > 0) {
    throw new Error('A live-stream subscriber cannot publish local tracks')
  }
  if (mode === 'voice' && localTracks.some(item => item.track.kind !== 'audio')) {
    throw new Error('Voice calls accept audio tracks only')
  }
  if (receiveKinds?.some(kind => mode === 'voice' && kind !== 'audio')) {
    throw new Error('Voice calls receive audio only')
  }
}
