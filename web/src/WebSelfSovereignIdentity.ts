import { requireCanonicalUUID } from './LinksWebClient'

export interface WebSelfSovereignIdentityWasm {
  public_key(): Uint8Array
  sign(transcript: Uint8Array): Uint8Array
  username_registration_signature(
    handle: string,
    deviceID: string,
    mlsNodeID: string,
    nonce: Uint8Array
  ): Uint8Array
  username_login_signature(
    handle: string,
    deviceID: string,
    mlsNodeID: string,
    nonce: Uint8Array
  ): Uint8Array
}

export interface WebSelfSovereignIdentityModule {
  new (): WebSelfSovereignIdentityWasm
  generate_recovery_phrase(wordCount: number): string
  passkey_identity_prf_salt(): Uint8Array
  from_recovery_phrase(phrase: string, passphrase: string): WebSelfSovereignIdentityWasm
  from_passkey_prf(prfOutput: Uint8Array): WebSelfSovereignIdentityWasm
}

export interface WebSelfSovereignWasmModule {
  WebSelfSovereignIdentity: WebSelfSovereignIdentityModule
}

const HANDLE_PATTERN = /^[a-z][a-z0-9_]{2,31}$/

function requireHandle(handle: string): string {
  if (!HANDLE_PATTERN.test(handle)) throw new Error('Invalid username handle')
  return handle
}

/** Browser wrapper for local mnemonic/passkey identity derivation. */
export class WebSelfSovereignIdentity {
  private constructor(private readonly core: WebSelfSovereignIdentityWasm) {}

  static create(wasm: WebSelfSovereignWasmModule): WebSelfSovereignIdentity {
    return new WebSelfSovereignIdentity(new wasm.WebSelfSovereignIdentity())
  }

  static generateRecoveryPhrase(
    wasm: WebSelfSovereignWasmModule,
    wordCount: 12 | 24
  ): string {
    return wasm.WebSelfSovereignIdentity.generate_recovery_phrase(wordCount)
  }

  static passkeyIdentityPRFSalt(wasm: WebSelfSovereignWasmModule): Uint8Array {
    return wasm.WebSelfSovereignIdentity.passkey_identity_prf_salt().slice()
  }

  static fromRecoveryPhrase(
    wasm: WebSelfSovereignWasmModule,
    phrase: string,
    passphrase = ''
  ): WebSelfSovereignIdentity {
    if (phrase.trim().length === 0) throw new Error('Recovery phrase required')
    return new WebSelfSovereignIdentity(
      wasm.WebSelfSovereignIdentity.from_recovery_phrase(phrase, passphrase)
    )
  }

  static fromPasskeyPRF(
    wasm: WebSelfSovereignWasmModule,
    prfOutput: Uint8Array
  ): WebSelfSovereignIdentity {
    if (prfOutput.length !== 32) throw new Error('Invalid passkey PRF output')
    return new WebSelfSovereignIdentity(
      wasm.WebSelfSovereignIdentity.from_passkey_prf(prfOutput)
    )
  }

  publicKey(): Uint8Array {
    return this.core.public_key().slice()
  }

  sign(transcript: Uint8Array): Uint8Array {
    return this.core.sign(transcript).slice()
  }

  usernameRegistrationSignature(
    handle: string,
    deviceID: string,
    mlsNodeID: string,
    nonce: Uint8Array
  ): Uint8Array {
    return this.core.username_registration_signature(
      requireHandle(handle),
      requireCanonicalUUID(deviceID, 'device ID'),
      requireCanonicalUUID(mlsNodeID, 'MLS node ID'),
      nonce
    ).slice()
  }

  usernameLoginSignature(
    handle: string,
    deviceID: string,
    mlsNodeID: string,
    nonce: Uint8Array
  ): Uint8Array {
    return this.core.username_login_signature(
      requireHandle(handle),
      requireCanonicalUUID(deviceID, 'device ID'),
      requireCanonicalUUID(mlsNodeID, 'MLS node ID'),
      nonce
    ).slice()
  }
}
