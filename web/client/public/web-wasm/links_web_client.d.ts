/* tslint:disable */
/* eslint-disable */

/**
 * Web device identity kept inside the WASM instance.
 *
 * The seed is intentionally not exposed to JavaScript. A host may persist
 * only its opaque client record through an audited storage/provider boundary;
 * this initial facade does not export or log key material.
 */
export class WebClientIdentity {
    free(): void;
    [Symbol.dispose](): void;
    /**
     * Install the mobile approval response after strict server-response
     * validation. The MLS credential is public, but it must match this exact
     * device identity before the Web client uses it.
     */
    complete_registration(user_id: string, device_id: string, mls_node_id: string, public_key: Uint8Array, mls_credential: Uint8Array): void;
    /**
     * Move this paired identity into the encrypted Web messaging core.
     * The seed stays in WASM; JavaScript receives only the opaque core handle.
     */
    create_messaging_core(): WebMessagingCore;
    /**
     * Decode one server-delivered WebRTC signal. Non-signaling frames are
     * rejected so the host must dispatch frames by body before calling this.
     */
    decode_webrtc_signal(frame: Uint8Array): WebRtcSignalDelivery;
    device_id(): string;
    /**
     * Encode one authenticated-device WebRTC offer, answer, or ICE signal.
     * The access token remains in the normal Hello frame, not in SDP.
     */
    encode_webrtc_signal(request_id: string, session_id: string, target_device_id: string, kind: number, sdp: string, sdp_mid: string, sdp_mline_index: number): Uint8Array;
    is_registered(): boolean;
    is_webrtc_signal_frame(frame: Uint8Array): boolean;
    /**
     * Return the public MLS BasicCredential for the Web OpenMLS provider.
     */
    mls_credential(): Uint8Array;
    mls_node_id(): string;
    /**
     * Create a fresh Web/desktop identity. IDs must be canonical non-nil UUIDs.
     */
    constructor(user_id: string, device_id: string, mls_node_id: string);
    /**
     * Build a fresh signed `links://connect?...` URI for mobile approval.
     * Calling this again starts a new attempt with a new nonce.
     */
    pairing_uri(): string;
    /**
     * Return only the public Ed25519 identity key.
     */
    public_key(): Uint8Array;
    user_id(): string;
}

/**
 * WASM handle for chunk decryption after private MLS metadata arrives.
 */
export class WebLargeFileDecryptor {
    free(): void;
    [Symbol.dispose](): void;
    decrypt_chunk(chunk_index: string, ciphertext: Uint8Array): Uint8Array;
    constructor(attachment_id: string, mime_type: string, original_size_bytes: string, ciphertext_size_bytes: string, content_key: Uint8Array, nonce: Uint8Array, ciphertext_sha256: Uint8Array, width: number, height: number, duration_ms: string);
}

/**
 * WASM handle for the same bounded chunk encryptor used by native hosts.
 */
export class WebLargeFileEncryptor {
    free(): void;
    [Symbol.dispose](): void;
    encrypt_chunk(plaintext: Uint8Array): Uint8Array;
    finish(): WebLargeFileMetadata;
    constructor(attachment_id: string, mime_type: string, width: number, height: number, duration_ms: string);
}

/**
 * Private metadata returned after a browser has staged all encrypted chunks.
 * Sizes are decimal strings so JavaScript never loses uint64 precision.
 */
export class WebLargeFileMetadata {
    private constructor();
    free(): void;
    [Symbol.dispose](): void;
    attachment_id(): string;
    ciphertext_sha256(): Uint8Array;
    ciphertext_size_bytes(): string;
    content_key(): Uint8Array;
    duration_ms(): string;
    height(): number;
    mime_type(): string;
    nonce(): Uint8Array;
    original_size_bytes(): string;
    width(): number;
}

export class WebMessagingCore {
    free(): void;
    [Symbol.dispose](): void;
    create_hello(access_token: string): Uint8Array;
    device_id(): string;
    durable_cursor(): string;
    /**
     * Build the messaging core from the browser account's Ed25519 seed.
     * The seed is consumed immediately into the Rust signer and never
     * returned to JavaScript.
     */
    static from_identity_seed(user_id: string, device_id: string, mls_credential: Uint8Array, seed: Uint8Array): WebMessagingCore;
    handle_server_frame(frame: Uint8Array): void;
    key_package(): Uint8Array;
    constructor(user_id: string, device_id: string, mls_credential: Uint8Array);
    pending_outgoing_count(): number;
    prekey_upload(curve_count: number, kem_count: number): Uint8Array;
    profile_upload(): Uint8Array;
    public_key(): Uint8Array;
    send_text(conversation_id: string, recipient_user_id: string, text: string): void;
    set_recipient(user_id: string, device_id: string, identity_public_key: Uint8Array, prekey_bundle: Uint8Array, _mls_credential: Uint8Array, mls_key_package: Uint8Array): void;
    take_messages(): string;
    take_outgoing(): Array<any>;
    user_id(): string;
}

/**
 * Server-delivered SDP/ICE signal decoded by the shared Rust protocol.
 * Signaling text is exposed to the WebRTC host, never to the Links server
 * application layer or the encrypted media path.
 */
export class WebRtcSignalDelivery {
    private constructor();
    free(): void;
    [Symbol.dispose](): void;
    kind(): number;
    request_id(): string;
    sdp(): string;
    sdp_mid(): string;
    sdp_mline_index(): number;
    sender_device_id(): string;
    session_id(): string;
    target_device_id(): string;
}

/**
 * Browser-local self-sovereign identity. The seed is held inside WASM and is
 * never returned to JavaScript; only public keys, signatures and recovery
 * phrases explicitly requested by the host cross the binding.
 */
export class WebSelfSovereignIdentity {
    free(): void;
    [Symbol.dispose](): void;
    /**
     * Derive an identity from a local WebAuthn PRF result. The PRF must come
     * from a user-verified passkey operation and must never be sent to Links.
     */
    static from_passkey_prf(prf_output: Uint8Array): WebSelfSovereignIdentity;
    /**
     * Restore an identity from a BIP-39 phrase. The phrase remains local.
     */
    static from_recovery_phrase(phrase: string, passphrase: string): WebSelfSovereignIdentity;
    /**
     * Generate a local English 12- or 24-word recovery phrase.
     */
    static generate_recovery_phrase(word_count: number): string;
    constructor();
    /**
     * Stable WebAuthn PRF salt for passkey-derived identities.
     */
    static passkey_identity_prf_salt(): Uint8Array;
    public_key(): Uint8Array;
    /**
     * Sign a transcript built by the shared protocol helpers.
     */
    sign(transcript: Uint8Array): Uint8Array;
    username_login_signature(challenge_id: string, handle: string, device_id: string, mls_node_id: string, challenge: Uint8Array, expires_at_ms: bigint): Uint8Array;
    username_registration_signature(challenge_id: string, handle: string, device_id: string, mls_node_id: string, challenge: Uint8Array, expires_at_ms: bigint): Uint8Array;
}

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly __wbg_webclientidentity_free: (a: number, b: number) => void;
    readonly __wbg_weblargefiledecryptor_free: (a: number, b: number) => void;
    readonly __wbg_weblargefileencryptor_free: (a: number, b: number) => void;
    readonly __wbg_weblargefilemetadata_free: (a: number, b: number) => void;
    readonly __wbg_webmessagingcore_free: (a: number, b: number) => void;
    readonly __wbg_webrtcsignaldelivery_free: (a: number, b: number) => void;
    readonly __wbg_webselfsovereignidentity_free: (a: number, b: number) => void;
    readonly webclientidentity_complete_registration: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number, i: number, j: number, k: number) => [number, number];
    readonly webclientidentity_create_messaging_core: (a: number) => [number, number, number];
    readonly webclientidentity_decode_webrtc_signal: (a: number, b: number, c: number) => [number, number, number];
    readonly webclientidentity_device_id: (a: number) => [number, number];
    readonly webclientidentity_encode_webrtc_signal: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number, i: number, j: number, k: number, l: number, m: number) => [number, number, number, number];
    readonly webclientidentity_is_registered: (a: number) => number;
    readonly webclientidentity_is_webrtc_signal_frame: (a: number, b: number, c: number) => number;
    readonly webclientidentity_mls_credential: (a: number) => [number, number, number, number];
    readonly webclientidentity_mls_node_id: (a: number) => [number, number];
    readonly webclientidentity_new: (a: number, b: number, c: number, d: number, e: number, f: number) => [number, number, number];
    readonly webclientidentity_pairing_uri: (a: number) => [number, number, number, number];
    readonly webclientidentity_public_key: (a: number) => [number, number];
    readonly webclientidentity_user_id: (a: number) => [number, number];
    readonly weblargefiledecryptor_decrypt_chunk: (a: number, b: number, c: number, d: number, e: number) => [number, number, number, number];
    readonly weblargefiledecryptor_new: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number, i: number, j: number, k: number, l: number, m: number, n: number, o: number, p: number, q: number, r: number) => [number, number, number];
    readonly weblargefileencryptor_encrypt_chunk: (a: number, b: number, c: number) => [number, number, number, number];
    readonly weblargefileencryptor_finish: (a: number) => [number, number, number];
    readonly weblargefileencryptor_new: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number) => [number, number, number];
    readonly weblargefilemetadata_attachment_id: (a: number) => [number, number];
    readonly weblargefilemetadata_ciphertext_sha256: (a: number) => [number, number];
    readonly weblargefilemetadata_ciphertext_size_bytes: (a: number) => [number, number];
    readonly weblargefilemetadata_content_key: (a: number) => [number, number];
    readonly weblargefilemetadata_duration_ms: (a: number) => [number, number];
    readonly weblargefilemetadata_height: (a: number) => number;
    readonly weblargefilemetadata_mime_type: (a: number) => [number, number];
    readonly weblargefilemetadata_nonce: (a: number) => [number, number];
    readonly weblargefilemetadata_original_size_bytes: (a: number) => [number, number];
    readonly weblargefilemetadata_width: (a: number) => number;
    readonly webmessagingcore_create_hello: (a: number, b: number, c: number) => [number, number, number, number];
    readonly webmessagingcore_device_id: (a: number) => [number, number];
    readonly webmessagingcore_durable_cursor: (a: number) => [number, number];
    readonly webmessagingcore_from_identity_seed: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number) => [number, number, number];
    readonly webmessagingcore_handle_server_frame: (a: number, b: number, c: number) => [number, number];
    readonly webmessagingcore_key_package: (a: number) => [number, number, number, number];
    readonly webmessagingcore_new: (a: number, b: number, c: number, d: number, e: number, f: number) => [number, number, number];
    readonly webmessagingcore_pending_outgoing_count: (a: number) => number;
    readonly webmessagingcore_prekey_upload: (a: number, b: number, c: number) => [number, number, number, number];
    readonly webmessagingcore_profile_upload: (a: number) => [number, number];
    readonly webmessagingcore_public_key: (a: number) => [number, number];
    readonly webmessagingcore_send_text: (a: number, b: number, c: number, d: number, e: number, f: number, g: number) => [number, number];
    readonly webmessagingcore_set_recipient: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number, i: number, j: number, k: number, l: number, m: number) => [number, number];
    readonly webmessagingcore_take_messages: (a: number) => [number, number];
    readonly webmessagingcore_take_outgoing: (a: number) => any;
    readonly webmessagingcore_user_id: (a: number) => [number, number];
    readonly webrtcsignaldelivery_kind: (a: number) => number;
    readonly webrtcsignaldelivery_request_id: (a: number) => [number, number];
    readonly webrtcsignaldelivery_sdp: (a: number) => [number, number];
    readonly webrtcsignaldelivery_sdp_mid: (a: number) => [number, number];
    readonly webrtcsignaldelivery_sdp_mline_index: (a: number) => number;
    readonly webrtcsignaldelivery_sender_device_id: (a: number) => [number, number];
    readonly webrtcsignaldelivery_session_id: (a: number) => [number, number];
    readonly webrtcsignaldelivery_target_device_id: (a: number) => [number, number];
    readonly webselfsovereignidentity_from_passkey_prf: (a: number, b: number) => [number, number, number];
    readonly webselfsovereignidentity_from_recovery_phrase: (a: number, b: number, c: number, d: number) => [number, number, number];
    readonly webselfsovereignidentity_generate_recovery_phrase: (a: number) => [number, number, number, number];
    readonly webselfsovereignidentity_new: () => [number, number, number];
    readonly webselfsovereignidentity_passkey_identity_prf_salt: () => [number, number];
    readonly webselfsovereignidentity_public_key: (a: number) => [number, number];
    readonly webselfsovereignidentity_sign: (a: number, b: number, c: number) => [number, number];
    readonly webselfsovereignidentity_username_login_signature: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number, i: number, j: number, k: number, l: bigint) => [number, number, number, number];
    readonly webselfsovereignidentity_username_registration_signature: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number, i: number, j: number, k: number, l: bigint) => [number, number, number, number];
    readonly __wbindgen_exn_store: (a: number) => void;
    readonly __externref_table_alloc: () => number;
    readonly __wbindgen_externrefs: WebAssembly.Table;
    readonly __wbindgen_malloc: (a: number, b: number) => number;
    readonly __wbindgen_realloc: (a: number, b: number, c: number, d: number) => number;
    readonly __externref_table_dealloc: (a: number) => void;
    readonly __wbindgen_free: (a: number, b: number, c: number) => void;
    readonly __wbindgen_start: () => void;
}

export type SyncInitInput = BufferSource | WebAssembly.Module;

/**
 * Instantiates the given `module`, which can either be bytes or
 * a precompiled `WebAssembly.Module`.
 *
 * @param {{ module: SyncInitInput }} module - Passing `SyncInitInput` directly is deprecated.
 *
 * @returns {InitOutput}
 */
export function initSync(module: { module: SyncInitInput } | SyncInitInput): InitOutput;

/**
 * If `module_or_path` is {RequestInfo} or {URL}, makes a request and
 * for everything else, calls `WebAssembly.instantiate` directly.
 *
 * @param {{ module_or_path: InitInput | Promise<InitInput> }} module_or_path - Passing `InitInput` directly is deprecated.
 *
 * @returns {Promise<InitOutput>}
 */
export default function __wbg_init (module_or_path?: { module_or_path: InitInput | Promise<InitInput> } | InitInput | Promise<InitInput>): Promise<InitOutput>;
