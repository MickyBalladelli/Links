export interface WebClientIdentityModule {
  new (userID: string, deviceID: string, mlsNodeID: string): WebClientIdentity
}

export interface WebClientIdentity {
  pairing_uri(): string
  public_key(): Uint8Array
  complete_registration(
    userID: string,
    deviceID: string,
    mlsNodeID: string,
    publicKey: Uint8Array,
    mlsCredential: Uint8Array
  ): void
  is_registered(): boolean
  mls_credential(): Uint8Array
  user_id(): string
  device_id(): string
  mls_node_id(): string
  encode_webrtc_signal(
    requestID: string,
    sessionID: string,
    targetDeviceID: string,
    kind: number,
    sdp: string,
    sdpMid: string,
    sdpMLineIndex: number
  ): Uint8Array
  decode_webrtc_signal(frame: Uint8Array): WebWasmRtcSignalDelivery
  is_webrtc_signal_frame(frame: Uint8Array): boolean
}

export interface WebWasmRtcSignalDelivery {
  request_id(): string
  sender_device_id(): string
  session_id(): string
  target_device_id(): string
  kind(): number
  sdp(): string
  sdp_mid(): string
  sdp_mline_index(): number
}

export interface WebRtcSignalDelivery {
  requestID: string
  senderDeviceID: string
  sessionID: string
  targetDeviceID: string
  kind: number
  sdp: string
  sdpMid: string
  sdpMLineIndex: number
}

export interface WebClientWasmModule {
  WebClientIdentity: WebClientIdentityModule
}

export interface PairedDeviceRegistration {
  userID: string
  deviceID: string
  mlsNodeID: string
  publicKey: string
  mlsCredential: string
}

export interface WebClientSnapshot {
  userID: string
  deviceID: string
  mlsNodeID: string
  registered: boolean
}

export { WebConnectionManager } from './WebConnectionManager'
export type {
  WebConnectionManagerOptions,
  WebConnectionState
} from './WebConnectionManager'

const MAX_CREDENTIAL_BYTES = 1024

export function requireCanonicalUUID(value: string, field: string): string {
  if (!/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/.test(value) ||
      value === '00000000-0000-0000-0000-000000000000') {
    throw new Error(`Invalid ${field}`)
  }
  return value
}

function decodeBase64URL(value: string, field: string, expectedLength?: number): Uint8Array {
  if (!/^[A-Za-z0-9_-]+$/.test(value) || value.length % 4 === 1) {
    throw new Error(`Invalid ${field}`)
  }
  const normalized = value.replace(/-/g, '+').replace(/_/g, '/')
  const padded = normalized + '='.repeat((4 - normalized.length % 4) % 4)
  let binary: string
  try {
    binary = atob(padded)
  } catch {
    throw new Error(`Invalid ${field}`)
  }
  const bytes = Uint8Array.from(binary, character => character.charCodeAt(0))
  if (expectedLength !== undefined && bytes.length !== expectedLength) {
    throw new Error(`Invalid ${field}`)
  }
  const canonical = btoa(Array.from(bytes, byte => String.fromCharCode(byte)).join(''))
    .replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/g, '')
  if (canonical !== value) {
    throw new Error(`Invalid ${field}`)
  }
  return bytes
}

/** Browser host for the shared Rust/WASM Web identity core. */
export class LinksWebClient {
  private readonly core: WebClientIdentity

  private constructor(core: WebClientIdentity) {
    this.core = core
  }

  static create(
    wasm: WebClientWasmModule,
    userID: string,
    deviceID: string,
    mlsNodeID: string
  ): LinksWebClient {
    requireCanonicalUUID(userID, 'user ID')
    requireCanonicalUUID(deviceID, 'device ID')
    requireCanonicalUUID(mlsNodeID, 'MLS node ID')
    return new LinksWebClient(new wasm.WebClientIdentity(userID, deviceID, mlsNodeID))
  }

  /** URI text for the platform QR renderer. No private key enters the URI. */
  pairingURI(): string {
    return this.core.pairing_uri()
  }

  /** Public identity key for the mobile approval display/confirmation. */
  publicKey(): Uint8Array {
    return this.core.public_key().slice()
  }

  /** Complete the two-device handoff using the mobile's registration response. */
  completePairing(registration: PairedDeviceRegistration): void {
    requireCanonicalUUID(registration.userID, 'registered user ID')
    requireCanonicalUUID(registration.deviceID, 'registered device ID')
    requireCanonicalUUID(registration.mlsNodeID, 'registered MLS node ID')
    const publicKey = decodeBase64URL(registration.publicKey, 'public key', 32)
    const mlsCredential = decodeBase64URL(registration.mlsCredential, 'MLS credential')
    if (mlsCredential.length === 0 || mlsCredential.length > MAX_CREDENTIAL_BYTES) {
      throw new Error('Invalid MLS credential')
    }
    this.core.complete_registration(
      registration.userID,
      registration.deviceID,
      registration.mlsNodeID,
      publicKey,
      mlsCredential
    )
  }

  isRegistered(): boolean {
    return this.core.is_registered()
  }

  mlsCredential(): Uint8Array {
    return this.core.mls_credential().slice()
  }

  snapshot(): WebClientSnapshot {
    return {
      userID: this.core.user_id(),
      deviceID: this.core.device_id(),
      mlsNodeID: this.core.mls_node_id(),
      registered: this.core.is_registered()
    }
  }

  encodeWebRtcSignal(
    requestID: string,
    sessionID: string,
    targetDeviceID: string,
    kind: number,
    sdp: string,
    sdpMid = '',
    sdpMLineIndex = 0
  ): Uint8Array {
    return this.core.encode_webrtc_signal(
      requestID,
      sessionID,
      targetDeviceID,
      kind,
      sdp,
      sdpMid,
      sdpMLineIndex
    )
  }

  isWebRtcSignalFrame(frame: Uint8Array): boolean {
    return this.core.is_webrtc_signal_frame(frame)
  }

  decodeWebRtcSignal(frame: Uint8Array): WebRtcSignalDelivery {
    const delivery = this.core.decode_webrtc_signal(frame)
    return {
      requestID: delivery.request_id(),
      senderDeviceID: delivery.sender_device_id(),
      sessionID: delivery.session_id(),
      targetDeviceID: delivery.target_device_id(),
      kind: delivery.kind(),
      sdp: delivery.sdp(),
      sdpMid: delivery.sdp_mid(),
      sdpMLineIndex: delivery.sdp_mline_index()
    }
  }
}
