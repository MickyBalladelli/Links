import type {
  WebCoreTransport,
  WebMessagingCore,
  WebReceivedTextMessage
} from './WebTextMessaging'

export interface WebWasmMessagingHandle {
  user_id(): string
  device_id(): string
  durable_cursor(): string
  create_hello(accessToken: string): Uint8Array
  handle_server_frame(frame: Uint8Array): void
  send_text(conversationID: string, recipientUserID: string, text: string): void
  take_outgoing(): ArrayLike<Uint8Array>
  take_messages(): string
  set_recipient(
    userID: string,
    deviceID: string,
    identityPublicKey: Uint8Array,
    prekeyBundle: Uint8Array,
    mlsCredential: Uint8Array,
    mlsKeyPackage: Uint8Array
  ): void
}

/** Adapts the Rust/WASM core to the browser WebSocket host. */
export class WebWasmMessagingCore implements WebMessagingCore {
  readonly userID: string
  readonly deviceID: string

  constructor(private readonly inner: WebWasmMessagingHandle) {
    this.userID = inner.user_id()
    this.deviceID = inner.device_id()
  }

  durableCursor(): bigint {
    return BigInt(this.inner.durable_cursor())
  }

  createHello(accessToken: string): Uint8Array {
    return this.inner.create_hello(accessToken)
  }

  handleServerFrame(
    frame: Uint8Array,
    transport: WebCoreTransport,
    _fullSync: boolean,
    onTextMessage: (message: WebReceivedTextMessage) => void
  ): 'pending' {
    this.inner.handle_server_frame(frame)
    this.flush(transport)
    const messages = JSON.parse(this.inner.take_messages() || '[]')
    if (!Array.isArray(messages)) throw new Error('Invalid Web message batch')
    for (const message of messages) {
      if (!message || typeof message !== 'object') throw new Error('Invalid Web message')
      onTextMessage({
        conversationID: String(message.conversationID),
        senderDeviceID: String(message.senderDeviceID),
        senderUserID: String(message.senderUserID || ''),
        text: String(message.text),
        sequenceID: BigInt(message.sequenceID),
        sentAtMs: BigInt(message.sentAtMs)
      })
    }
    return 'pending'
  }

  sendText(
    conversationID: string,
    recipientUserID: string,
    text: string,
    transport: WebCoreTransport
  ): void {
    this.inner.send_text(conversationID, recipientUserID, text)
    this.flush(transport)
  }

  setRecipient(
    userID: string,
    deviceID: string,
    identityPublicKey: Uint8Array,
    prekeyBundle: Uint8Array,
    mlsCredential: Uint8Array,
    mlsKeyPackage: Uint8Array
  ): void {
    this.inner.set_recipient(
      userID,
      deviceID,
      identityPublicKey,
      prekeyBundle,
      mlsCredential,
      mlsKeyPackage
    )
  }

  private flush(transport: WebCoreTransport): void {
    const frames = this.inner.take_outgoing()
    for (let index = 0; index < frames.length; index += 1) {
      const frame = frames[index]
      if (!(frame instanceof Uint8Array) || !transport.send(frame)) {
        throw new Error('WebSocket is not connected')
      }
    }
  }
}
