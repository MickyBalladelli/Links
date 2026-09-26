/* @ts-self-types="./links_web_client.d.ts" */

/**
 * Web device identity kept inside the WASM instance.
 *
 * The seed is intentionally not exposed to JavaScript. A host may persist
 * only its opaque client record through an audited storage/provider boundary;
 * this initial facade does not export or log key material.
 */
export class WebClientIdentity {
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        WebClientIdentityFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_webclientidentity_free(ptr, 0);
    }
    /**
     * Install the mobile approval response after strict server-response
     * validation. The MLS credential is public, but it must match this exact
     * device identity before the Web client uses it.
     * @param {string} user_id
     * @param {string} device_id
     * @param {string} mls_node_id
     * @param {Uint8Array} public_key
     * @param {Uint8Array} mls_credential
     */
    complete_registration(user_id, device_id, mls_node_id, public_key, mls_credential) {
        const ptr0 = passStringToWasm0(user_id, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(device_id, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len1 = WASM_VECTOR_LEN;
        const ptr2 = passStringToWasm0(mls_node_id, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len2 = WASM_VECTOR_LEN;
        const ptr3 = passArray8ToWasm0(public_key, wasm.__wbindgen_malloc);
        const len3 = WASM_VECTOR_LEN;
        const ptr4 = passArray8ToWasm0(mls_credential, wasm.__wbindgen_malloc);
        const len4 = WASM_VECTOR_LEN;
        const ret = wasm.webclientidentity_complete_registration(this.__wbg_ptr, ptr0, len0, ptr1, len1, ptr2, len2, ptr3, len3, ptr4, len4);
        if (ret[1]) {
            throw takeFromExternrefTable0(ret[0]);
        }
    }
    /**
     * Move this paired identity into the encrypted Web messaging core.
     * The seed stays in WASM; JavaScript receives only the opaque core handle.
     * @returns {WebMessagingCore}
     */
    create_messaging_core() {
        const ptr = this.__destroy_into_raw();
        const ret = wasm.webclientidentity_create_messaging_core(ptr);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return WebMessagingCore.__wrap(ret[0]);
    }
    /**
     * Decode one server-delivered WebRTC signal. Non-signaling frames are
     * rejected so the host must dispatch frames by body before calling this.
     * @param {Uint8Array} frame
     * @returns {WebRtcSignalDelivery}
     */
    decode_webrtc_signal(frame) {
        const ptr0 = passArray8ToWasm0(frame, wasm.__wbindgen_malloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.webclientidentity_decode_webrtc_signal(this.__wbg_ptr, ptr0, len0);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return WebRtcSignalDelivery.__wrap(ret[0]);
    }
    /**
     * @returns {string}
     */
    device_id() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.webclientidentity_device_id(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * Encode one authenticated-device WebRTC offer, answer, or ICE signal.
     * The access token remains in the normal Hello frame, not in SDP.
     * @param {string} request_id
     * @param {string} session_id
     * @param {string} target_device_id
     * @param {number} kind
     * @param {string} sdp
     * @param {string} sdp_mid
     * @param {number} sdp_mline_index
     * @returns {Uint8Array}
     */
    encode_webrtc_signal(request_id, session_id, target_device_id, kind, sdp, sdp_mid, sdp_mline_index) {
        const ptr0 = passStringToWasm0(request_id, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(session_id, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len1 = WASM_VECTOR_LEN;
        const ptr2 = passStringToWasm0(target_device_id, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len2 = WASM_VECTOR_LEN;
        const ptr3 = passStringToWasm0(sdp, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len3 = WASM_VECTOR_LEN;
        const ptr4 = passStringToWasm0(sdp_mid, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len4 = WASM_VECTOR_LEN;
        const ret = wasm.webclientidentity_encode_webrtc_signal(this.__wbg_ptr, ptr0, len0, ptr1, len1, ptr2, len2, kind, ptr3, len3, ptr4, len4, sdp_mline_index);
        if (ret[3]) {
            throw takeFromExternrefTable0(ret[2]);
        }
        var v6 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
        return v6;
    }
    /**
     * @returns {boolean}
     */
    is_registered() {
        const ret = wasm.webclientidentity_is_registered(this.__wbg_ptr);
        return ret !== 0;
    }
    /**
     * @param {Uint8Array} frame
     * @returns {boolean}
     */
    is_webrtc_signal_frame(frame) {
        const ptr0 = passArray8ToWasm0(frame, wasm.__wbindgen_malloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.webclientidentity_is_webrtc_signal_frame(this.__wbg_ptr, ptr0, len0);
        return ret !== 0;
    }
    /**
     * Return the public MLS BasicCredential for the Web OpenMLS provider.
     * @returns {Uint8Array}
     */
    mls_credential() {
        const ret = wasm.webclientidentity_mls_credential(this.__wbg_ptr);
        if (ret[3]) {
            throw takeFromExternrefTable0(ret[2]);
        }
        var v1 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
        return v1;
    }
    /**
     * @returns {string}
     */
    mls_node_id() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.webclientidentity_mls_node_id(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * Create a fresh Web/desktop identity. IDs must be canonical non-nil UUIDs.
     * @param {string} user_id
     * @param {string} device_id
     * @param {string} mls_node_id
     */
    constructor(user_id, device_id, mls_node_id) {
        const ptr0 = passStringToWasm0(user_id, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(device_id, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len1 = WASM_VECTOR_LEN;
        const ptr2 = passStringToWasm0(mls_node_id, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len2 = WASM_VECTOR_LEN;
        const ret = wasm.webclientidentity_new(ptr0, len0, ptr1, len1, ptr2, len2);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        this.__wbg_ptr = ret[0];
        WebClientIdentityFinalization.register(this, this.__wbg_ptr, this);
        return this;
    }
    /**
     * Build a fresh signed `links://connect?...` URI for mobile approval.
     * Calling this again starts a new attempt with a new nonce.
     * @returns {string}
     */
    pairing_uri() {
        let deferred2_0;
        let deferred2_1;
        try {
            const ret = wasm.webclientidentity_pairing_uri(this.__wbg_ptr);
            var ptr1 = ret[0];
            var len1 = ret[1];
            if (ret[3]) {
                ptr1 = 0; len1 = 0;
                throw takeFromExternrefTable0(ret[2]);
            }
            deferred2_0 = ptr1;
            deferred2_1 = len1;
            return getStringFromWasm0(ptr1, len1);
        } finally {
            wasm.__wbindgen_free(deferred2_0, deferred2_1, 1);
        }
    }
    /**
     * Return only the public Ed25519 identity key.
     * @returns {Uint8Array}
     */
    public_key() {
        const ret = wasm.webclientidentity_public_key(this.__wbg_ptr);
        var v1 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
        return v1;
    }
    /**
     * @returns {string}
     */
    user_id() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.webclientidentity_user_id(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
}
if (Symbol.dispose) WebClientIdentity.prototype[Symbol.dispose] = WebClientIdentity.prototype.free;

/**
 * One-shot encrypted document attachment for files up to the regular file
 * limit. The ciphertext is uploaded separately; only this private metadata
 * is sent through the encrypted message.
 */
export class WebFileEncryption {
    static __wrap(ptr) {
        const obj = Object.create(WebFileEncryption.prototype);
        obj.__wbg_ptr = ptr;
        WebFileEncryptionFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        WebFileEncryptionFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_webfileencryption_free(ptr, 0);
    }
    /**
     * @returns {string}
     */
    attachment_id() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.webfileencryption_attachment_id(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * @returns {Uint8Array}
     */
    ciphertext() {
        const ret = wasm.webfileencryption_ciphertext(this.__wbg_ptr);
        var v1 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
        return v1;
    }
    /**
     * @returns {Uint8Array}
     */
    ciphertext_sha256() {
        const ret = wasm.webfileencryption_ciphertext_sha256(this.__wbg_ptr);
        var v1 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
        return v1;
    }
    /**
     * @returns {string}
     */
    ciphertext_size_bytes() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.webfileencryption_ciphertext_size_bytes(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * @returns {Uint8Array}
     */
    content_key() {
        const ret = wasm.webfileencryption_content_key(this.__wbg_ptr);
        var v1 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
        return v1;
    }
    /**
     * @returns {string}
     */
    file_name() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.webfileencryption_file_name(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * @returns {string}
     */
    mime_type() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.webfileencryption_mime_type(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * @returns {Uint8Array}
     */
    nonce() {
        const ret = wasm.webfileencryption_nonce(this.__wbg_ptr);
        var v1 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
        return v1;
    }
}
if (Symbol.dispose) WebFileEncryption.prototype[Symbol.dispose] = WebFileEncryption.prototype.free;

/**
 * WASM handle for chunk decryption after private MLS metadata arrives.
 */
export class WebLargeFileDecryptor {
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        WebLargeFileDecryptorFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_weblargefiledecryptor_free(ptr, 0);
    }
    /**
     * @param {string} chunk_index
     * @param {Uint8Array} ciphertext
     * @returns {Uint8Array}
     */
    decrypt_chunk(chunk_index, ciphertext) {
        const ptr0 = passStringToWasm0(chunk_index, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passArray8ToWasm0(ciphertext, wasm.__wbindgen_malloc);
        const len1 = WASM_VECTOR_LEN;
        const ret = wasm.weblargefiledecryptor_decrypt_chunk(this.__wbg_ptr, ptr0, len0, ptr1, len1);
        if (ret[3]) {
            throw takeFromExternrefTable0(ret[2]);
        }
        var v3 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
        return v3;
    }
    /**
     * @param {string} attachment_id
     * @param {string} mime_type
     * @param {string} original_size_bytes
     * @param {string} ciphertext_size_bytes
     * @param {Uint8Array} content_key
     * @param {Uint8Array} nonce
     * @param {Uint8Array} ciphertext_sha256
     * @param {number} width
     * @param {number} height
     * @param {string} duration_ms
     */
    constructor(attachment_id, mime_type, original_size_bytes, ciphertext_size_bytes, content_key, nonce, ciphertext_sha256, width, height, duration_ms) {
        const ptr0 = passStringToWasm0(attachment_id, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(mime_type, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len1 = WASM_VECTOR_LEN;
        const ptr2 = passStringToWasm0(original_size_bytes, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len2 = WASM_VECTOR_LEN;
        const ptr3 = passStringToWasm0(ciphertext_size_bytes, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len3 = WASM_VECTOR_LEN;
        const ptr4 = passArray8ToWasm0(content_key, wasm.__wbindgen_malloc);
        const len4 = WASM_VECTOR_LEN;
        const ptr5 = passArray8ToWasm0(nonce, wasm.__wbindgen_malloc);
        const len5 = WASM_VECTOR_LEN;
        const ptr6 = passArray8ToWasm0(ciphertext_sha256, wasm.__wbindgen_malloc);
        const len6 = WASM_VECTOR_LEN;
        const ptr7 = passStringToWasm0(duration_ms, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len7 = WASM_VECTOR_LEN;
        const ret = wasm.weblargefiledecryptor_new(ptr0, len0, ptr1, len1, ptr2, len2, ptr3, len3, ptr4, len4, ptr5, len5, ptr6, len6, width, height, ptr7, len7);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        this.__wbg_ptr = ret[0];
        WebLargeFileDecryptorFinalization.register(this, this.__wbg_ptr, this);
        return this;
    }
}
if (Symbol.dispose) WebLargeFileDecryptor.prototype[Symbol.dispose] = WebLargeFileDecryptor.prototype.free;

/**
 * WASM handle for the same bounded chunk encryptor used by native hosts.
 */
export class WebLargeFileEncryptor {
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        WebLargeFileEncryptorFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_weblargefileencryptor_free(ptr, 0);
    }
    /**
     * @param {Uint8Array} plaintext
     * @returns {Uint8Array}
     */
    encrypt_chunk(plaintext) {
        const ptr0 = passArray8ToWasm0(plaintext, wasm.__wbindgen_malloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.weblargefileencryptor_encrypt_chunk(this.__wbg_ptr, ptr0, len0);
        if (ret[3]) {
            throw takeFromExternrefTable0(ret[2]);
        }
        var v2 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
        return v2;
    }
    /**
     * @returns {WebLargeFileMetadata}
     */
    finish() {
        const ret = wasm.weblargefileencryptor_finish(this.__wbg_ptr);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return WebLargeFileMetadata.__wrap(ret[0]);
    }
    /**
     * @param {string} attachment_id
     * @param {string} mime_type
     * @param {number} width
     * @param {number} height
     * @param {string} duration_ms
     */
    constructor(attachment_id, mime_type, width, height, duration_ms) {
        const ptr0 = passStringToWasm0(attachment_id, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(mime_type, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len1 = WASM_VECTOR_LEN;
        const ptr2 = passStringToWasm0(duration_ms, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len2 = WASM_VECTOR_LEN;
        const ret = wasm.weblargefileencryptor_new(ptr0, len0, ptr1, len1, width, height, ptr2, len2);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        this.__wbg_ptr = ret[0];
        WebLargeFileEncryptorFinalization.register(this, this.__wbg_ptr, this);
        return this;
    }
}
if (Symbol.dispose) WebLargeFileEncryptor.prototype[Symbol.dispose] = WebLargeFileEncryptor.prototype.free;

/**
 * Private metadata returned after a browser has staged all encrypted chunks.
 * Sizes are decimal strings so JavaScript never loses uint64 precision.
 */
export class WebLargeFileMetadata {
    static __wrap(ptr) {
        const obj = Object.create(WebLargeFileMetadata.prototype);
        obj.__wbg_ptr = ptr;
        WebLargeFileMetadataFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        WebLargeFileMetadataFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_weblargefilemetadata_free(ptr, 0);
    }
    /**
     * @returns {string}
     */
    attachment_id() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.weblargefilemetadata_attachment_id(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * @returns {Uint8Array}
     */
    ciphertext_sha256() {
        const ret = wasm.weblargefilemetadata_ciphertext_sha256(this.__wbg_ptr);
        var v1 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
        return v1;
    }
    /**
     * @returns {string}
     */
    ciphertext_size_bytes() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.weblargefilemetadata_ciphertext_size_bytes(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * @returns {Uint8Array}
     */
    content_key() {
        const ret = wasm.weblargefilemetadata_content_key(this.__wbg_ptr);
        var v1 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
        return v1;
    }
    /**
     * @returns {string}
     */
    duration_ms() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.weblargefilemetadata_duration_ms(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * @returns {number}
     */
    height() {
        const ret = wasm.weblargefilemetadata_height(this.__wbg_ptr);
        return ret >>> 0;
    }
    /**
     * @returns {string}
     */
    mime_type() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.weblargefilemetadata_mime_type(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * @returns {Uint8Array}
     */
    nonce() {
        const ret = wasm.weblargefilemetadata_nonce(this.__wbg_ptr);
        var v1 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
        return v1;
    }
    /**
     * @returns {string}
     */
    original_size_bytes() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.weblargefilemetadata_original_size_bytes(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * @returns {number}
     */
    width() {
        const ret = wasm.weblargefilemetadata_width(this.__wbg_ptr);
        return ret >>> 0;
    }
}
if (Symbol.dispose) WebLargeFileMetadata.prototype[Symbol.dispose] = WebLargeFileMetadata.prototype.free;

export class WebMessagingCore {
    static __wrap(ptr) {
        const obj = Object.create(WebMessagingCore.prototype);
        obj.__wbg_ptr = ptr;
        WebMessagingCoreFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        WebMessagingCoreFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_webmessagingcore_free(ptr, 0);
    }
    /**
     * @param {string} access_token
     * @returns {Uint8Array}
     */
    create_hello(access_token) {
        const ptr0 = passStringToWasm0(access_token, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.webmessagingcore_create_hello(this.__wbg_ptr, ptr0, len0);
        if (ret[3]) {
            throw takeFromExternrefTable0(ret[2]);
        }
        var v2 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
        return v2;
    }
    /**
     * @returns {string}
     */
    device_id() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.webmessagingcore_device_id(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * @returns {string}
     */
    durable_cursor() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.webmessagingcore_durable_cursor(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * Build the messaging core from the browser account's Ed25519 seed.
     * The seed is consumed immediately into the Rust signer and never
     * returned to JavaScript.
     * @param {string} user_id
     * @param {string} device_id
     * @param {Uint8Array} mls_credential
     * @param {Uint8Array} seed
     * @returns {WebMessagingCore}
     */
    static from_identity_seed(user_id, device_id, mls_credential, seed) {
        const ptr0 = passStringToWasm0(user_id, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(device_id, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len1 = WASM_VECTOR_LEN;
        const ptr2 = passArray8ToWasm0(mls_credential, wasm.__wbindgen_malloc);
        const len2 = WASM_VECTOR_LEN;
        const ptr3 = passArray8ToWasm0(seed, wasm.__wbindgen_malloc);
        const len3 = WASM_VECTOR_LEN;
        const ret = wasm.webmessagingcore_from_identity_seed(ptr0, len0, ptr1, len1, ptr2, len2, ptr3, len3);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return WebMessagingCore.__wrap(ret[0]);
    }
    /**
     * @param {Uint8Array} frame
     */
    handle_server_frame(frame) {
        const ptr0 = passArray8ToWasm0(frame, wasm.__wbindgen_malloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.webmessagingcore_handle_server_frame(this.__wbg_ptr, ptr0, len0);
        if (ret[1]) {
            throw takeFromExternrefTable0(ret[0]);
        }
    }
    /**
     * @returns {Uint8Array}
     */
    key_package() {
        const ret = wasm.webmessagingcore_key_package(this.__wbg_ptr);
        if (ret[3]) {
            throw takeFromExternrefTable0(ret[2]);
        }
        var v1 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
        return v1;
    }
    /**
     * @param {string} user_id
     * @param {string} device_id
     * @param {Uint8Array} mls_credential
     */
    constructor(user_id, device_id, mls_credential) {
        const ptr0 = passStringToWasm0(user_id, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(device_id, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len1 = WASM_VECTOR_LEN;
        const ptr2 = passArray8ToWasm0(mls_credential, wasm.__wbindgen_malloc);
        const len2 = WASM_VECTOR_LEN;
        const ret = wasm.webmessagingcore_new(ptr0, len0, ptr1, len1, ptr2, len2);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        this.__wbg_ptr = ret[0];
        WebMessagingCoreFinalization.register(this, this.__wbg_ptr, this);
        return this;
    }
    /**
     * @returns {number}
     */
    pending_outgoing_count() {
        const ret = wasm.webmessagingcore_pending_outgoing_count(this.__wbg_ptr);
        return ret >>> 0;
    }
    /**
     * @param {number} curve_count
     * @param {number} kem_count
     * @returns {Uint8Array}
     */
    prekey_upload(curve_count, kem_count) {
        const ret = wasm.webmessagingcore_prekey_upload(this.__wbg_ptr, curve_count, kem_count);
        if (ret[3]) {
            throw takeFromExternrefTable0(ret[2]);
        }
        var v1 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
        return v1;
    }
    /**
     * @returns {Uint8Array}
     */
    profile_upload() {
        const ret = wasm.webmessagingcore_profile_upload(this.__wbg_ptr);
        var v1 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
        return v1;
    }
    /**
     * @returns {Uint8Array}
     */
    public_key() {
        const ret = wasm.webmessagingcore_public_key(this.__wbg_ptr);
        var v1 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
        return v1;
    }
    /**
     * @param {string} conversation_id
     * @param {string} recipient_user_id
     * @param {string} attachment_id
     * @param {string} mime_type
     * @param {string} file_name
     * @param {string} ciphertext_size_bytes
     * @param {Uint8Array} content_key
     * @param {Uint8Array} nonce
     * @param {Uint8Array} ciphertext_sha256
     */
    send_file(conversation_id, recipient_user_id, attachment_id, mime_type, file_name, ciphertext_size_bytes, content_key, nonce, ciphertext_sha256) {
        const ptr0 = passStringToWasm0(conversation_id, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(recipient_user_id, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len1 = WASM_VECTOR_LEN;
        const ptr2 = passStringToWasm0(attachment_id, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len2 = WASM_VECTOR_LEN;
        const ptr3 = passStringToWasm0(mime_type, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len3 = WASM_VECTOR_LEN;
        const ptr4 = passStringToWasm0(file_name, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len4 = WASM_VECTOR_LEN;
        const ptr5 = passStringToWasm0(ciphertext_size_bytes, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len5 = WASM_VECTOR_LEN;
        const ptr6 = passArray8ToWasm0(content_key, wasm.__wbindgen_malloc);
        const len6 = WASM_VECTOR_LEN;
        const ptr7 = passArray8ToWasm0(nonce, wasm.__wbindgen_malloc);
        const len7 = WASM_VECTOR_LEN;
        const ptr8 = passArray8ToWasm0(ciphertext_sha256, wasm.__wbindgen_malloc);
        const len8 = WASM_VECTOR_LEN;
        const ret = wasm.webmessagingcore_send_file(this.__wbg_ptr, ptr0, len0, ptr1, len1, ptr2, len2, ptr3, len3, ptr4, len4, ptr5, len5, ptr6, len6, ptr7, len7, ptr8, len8);
        if (ret[1]) {
            throw takeFromExternrefTable0(ret[0]);
        }
    }
    /**
     * @param {string} conversation_id
     * @param {string} recipient_user_id
     * @param {string} text
     */
    send_text(conversation_id, recipient_user_id, text) {
        const ptr0 = passStringToWasm0(conversation_id, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(recipient_user_id, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len1 = WASM_VECTOR_LEN;
        const ptr2 = passStringToWasm0(text, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len2 = WASM_VECTOR_LEN;
        const ret = wasm.webmessagingcore_send_text(this.__wbg_ptr, ptr0, len0, ptr1, len1, ptr2, len2);
        if (ret[1]) {
            throw takeFromExternrefTable0(ret[0]);
        }
    }
    /**
     * @param {string} conversation_id
     * @param {string} recipient_user_id
     * @param {string} text
     */
    send_text_to_self(conversation_id, recipient_user_id, text) {
        const ptr0 = passStringToWasm0(conversation_id, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(recipient_user_id, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len1 = WASM_VECTOR_LEN;
        const ptr2 = passStringToWasm0(text, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len2 = WASM_VECTOR_LEN;
        const ret = wasm.webmessagingcore_send_text_to_self(this.__wbg_ptr, ptr0, len0, ptr1, len1, ptr2, len2);
        if (ret[1]) {
            throw takeFromExternrefTable0(ret[0]);
        }
    }
    /**
     * @param {string} user_id
     * @param {string} device_id
     * @param {Uint8Array} identity_public_key
     * @param {Uint8Array} prekey_bundle
     * @param {Uint8Array} _mls_credential
     * @param {Uint8Array} mls_key_package
     */
    set_recipient(user_id, device_id, identity_public_key, prekey_bundle, _mls_credential, mls_key_package) {
        const ptr0 = passStringToWasm0(user_id, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(device_id, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len1 = WASM_VECTOR_LEN;
        const ptr2 = passArray8ToWasm0(identity_public_key, wasm.__wbindgen_malloc);
        const len2 = WASM_VECTOR_LEN;
        const ptr3 = passArray8ToWasm0(prekey_bundle, wasm.__wbindgen_malloc);
        const len3 = WASM_VECTOR_LEN;
        const ptr4 = passArray8ToWasm0(_mls_credential, wasm.__wbindgen_malloc);
        const len4 = WASM_VECTOR_LEN;
        const ptr5 = passArray8ToWasm0(mls_key_package, wasm.__wbindgen_malloc);
        const len5 = WASM_VECTOR_LEN;
        const ret = wasm.webmessagingcore_set_recipient(this.__wbg_ptr, ptr0, len0, ptr1, len1, ptr2, len2, ptr3, len3, ptr4, len4, ptr5, len5);
        if (ret[1]) {
            throw takeFromExternrefTable0(ret[0]);
        }
    }
    /**
     * @returns {string}
     */
    take_messages() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.webmessagingcore_take_messages(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * @returns {Array<any>}
     */
    take_outgoing() {
        const ret = wasm.webmessagingcore_take_outgoing(this.__wbg_ptr);
        return ret;
    }
    /**
     * @returns {string}
     */
    user_id() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.webmessagingcore_user_id(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
}
if (Symbol.dispose) WebMessagingCore.prototype[Symbol.dispose] = WebMessagingCore.prototype.free;

/**
 * Server-delivered SDP/ICE signal decoded by the shared Rust protocol.
 * Signaling text is exposed to the WebRTC host, never to the Links server
 * application layer or the encrypted media path.
 */
export class WebRtcSignalDelivery {
    static __wrap(ptr) {
        const obj = Object.create(WebRtcSignalDelivery.prototype);
        obj.__wbg_ptr = ptr;
        WebRtcSignalDeliveryFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        WebRtcSignalDeliveryFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_webrtcsignaldelivery_free(ptr, 0);
    }
    /**
     * @returns {number}
     */
    kind() {
        const ret = wasm.webrtcsignaldelivery_kind(this.__wbg_ptr);
        return ret;
    }
    /**
     * @returns {string}
     */
    request_id() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.webrtcsignaldelivery_request_id(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * @returns {string}
     */
    sdp() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.webrtcsignaldelivery_sdp(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * @returns {string}
     */
    sdp_mid() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.webrtcsignaldelivery_sdp_mid(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * @returns {number}
     */
    sdp_mline_index() {
        const ret = wasm.webrtcsignaldelivery_sdp_mline_index(this.__wbg_ptr);
        return ret >>> 0;
    }
    /**
     * @returns {string}
     */
    sender_device_id() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.webrtcsignaldelivery_sender_device_id(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * @returns {string}
     */
    session_id() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.webrtcsignaldelivery_session_id(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * @returns {string}
     */
    target_device_id() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.webrtcsignaldelivery_target_device_id(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
}
if (Symbol.dispose) WebRtcSignalDelivery.prototype[Symbol.dispose] = WebRtcSignalDelivery.prototype.free;

/**
 * Browser-local self-sovereign identity. The seed is held inside WASM and is
 * never returned to JavaScript; only public keys, signatures and recovery
 * phrases explicitly requested by the host cross the binding.
 */
export class WebSelfSovereignIdentity {
    static __wrap(ptr) {
        const obj = Object.create(WebSelfSovereignIdentity.prototype);
        obj.__wbg_ptr = ptr;
        WebSelfSovereignIdentityFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        WebSelfSovereignIdentityFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_webselfsovereignidentity_free(ptr, 0);
    }
    /**
     * Derive an identity from a local WebAuthn PRF result. The PRF must come
     * from a user-verified passkey operation and must never be sent to Links.
     * @param {Uint8Array} prf_output
     * @returns {WebSelfSovereignIdentity}
     */
    static from_passkey_prf(prf_output) {
        const ptr0 = passArray8ToWasm0(prf_output, wasm.__wbindgen_malloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.webselfsovereignidentity_from_passkey_prf(ptr0, len0);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return WebSelfSovereignIdentity.__wrap(ret[0]);
    }
    /**
     * Restore an identity from a BIP-39 phrase. The phrase remains local.
     * @param {string} phrase
     * @param {string} passphrase
     * @returns {WebSelfSovereignIdentity}
     */
    static from_recovery_phrase(phrase, passphrase) {
        const ptr0 = passStringToWasm0(phrase, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(passphrase, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len1 = WASM_VECTOR_LEN;
        const ret = wasm.webselfsovereignidentity_from_recovery_phrase(ptr0, len0, ptr1, len1);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return WebSelfSovereignIdentity.__wrap(ret[0]);
    }
    /**
     * Generate a local English 12- or 24-word recovery phrase.
     * @param {number} word_count
     * @returns {string}
     */
    static generate_recovery_phrase(word_count) {
        let deferred2_0;
        let deferred2_1;
        try {
            const ret = wasm.webselfsovereignidentity_generate_recovery_phrase(word_count);
            var ptr1 = ret[0];
            var len1 = ret[1];
            if (ret[3]) {
                ptr1 = 0; len1 = 0;
                throw takeFromExternrefTable0(ret[2]);
            }
            deferred2_0 = ptr1;
            deferred2_1 = len1;
            return getStringFromWasm0(ptr1, len1);
        } finally {
            wasm.__wbindgen_free(deferred2_0, deferred2_1, 1);
        }
    }
    constructor() {
        const ret = wasm.webselfsovereignidentity_new();
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        this.__wbg_ptr = ret[0];
        WebSelfSovereignIdentityFinalization.register(this, this.__wbg_ptr, this);
        return this;
    }
    /**
     * Stable WebAuthn PRF salt for passkey-derived identities.
     * @returns {Uint8Array}
     */
    static passkey_identity_prf_salt() {
        const ret = wasm.webselfsovereignidentity_passkey_identity_prf_salt();
        var v1 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
        return v1;
    }
    /**
     * @returns {Uint8Array}
     */
    public_key() {
        const ret = wasm.webselfsovereignidentity_public_key(this.__wbg_ptr);
        var v1 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
        return v1;
    }
    /**
     * Sign a transcript built by the shared protocol helpers.
     * @param {Uint8Array} transcript
     * @returns {Uint8Array}
     */
    sign(transcript) {
        const ptr0 = passArray8ToWasm0(transcript, wasm.__wbindgen_malloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.webselfsovereignidentity_sign(this.__wbg_ptr, ptr0, len0);
        var v2 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
        return v2;
    }
    /**
     * @param {string} challenge_id
     * @param {string} handle
     * @param {string} device_id
     * @param {string} mls_node_id
     * @param {Uint8Array} challenge
     * @param {bigint} expires_at_ms
     * @returns {Uint8Array}
     */
    username_login_signature(challenge_id, handle, device_id, mls_node_id, challenge, expires_at_ms) {
        const ptr0 = passStringToWasm0(challenge_id, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(handle, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len1 = WASM_VECTOR_LEN;
        const ptr2 = passStringToWasm0(device_id, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len2 = WASM_VECTOR_LEN;
        const ptr3 = passStringToWasm0(mls_node_id, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len3 = WASM_VECTOR_LEN;
        const ptr4 = passArray8ToWasm0(challenge, wasm.__wbindgen_malloc);
        const len4 = WASM_VECTOR_LEN;
        const ret = wasm.webselfsovereignidentity_username_login_signature(this.__wbg_ptr, ptr0, len0, ptr1, len1, ptr2, len2, ptr3, len3, ptr4, len4, expires_at_ms);
        if (ret[3]) {
            throw takeFromExternrefTable0(ret[2]);
        }
        var v6 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
        return v6;
    }
    /**
     * @param {string} challenge_id
     * @param {string} handle
     * @param {string} device_id
     * @param {string} mls_node_id
     * @param {Uint8Array} challenge
     * @param {bigint} expires_at_ms
     * @returns {Uint8Array}
     */
    username_registration_signature(challenge_id, handle, device_id, mls_node_id, challenge, expires_at_ms) {
        const ptr0 = passStringToWasm0(challenge_id, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(handle, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len1 = WASM_VECTOR_LEN;
        const ptr2 = passStringToWasm0(device_id, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len2 = WASM_VECTOR_LEN;
        const ptr3 = passStringToWasm0(mls_node_id, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len3 = WASM_VECTOR_LEN;
        const ptr4 = passArray8ToWasm0(challenge, wasm.__wbindgen_malloc);
        const len4 = WASM_VECTOR_LEN;
        const ret = wasm.webselfsovereignidentity_username_registration_signature(this.__wbg_ptr, ptr0, len0, ptr1, len1, ptr2, len2, ptr3, len3, ptr4, len4, expires_at_ms);
        if (ret[3]) {
            throw takeFromExternrefTable0(ret[2]);
        }
        var v6 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
        return v6;
    }
}
if (Symbol.dispose) WebSelfSovereignIdentity.prototype[Symbol.dispose] = WebSelfSovereignIdentity.prototype.free;

/**
 * Encrypt one regular document attachment with the same Rust core used by
 * native clients. The original bytes never leave the browser unencrypted.
 * @param {string} attachment_id
 * @param {Uint8Array} plaintext
 * @param {string} mime_type
 * @param {string} file_name
 * @returns {WebFileEncryption}
 */
export function encrypt_file(attachment_id, plaintext, mime_type, file_name) {
    const ptr0 = passStringToWasm0(attachment_id, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len0 = WASM_VECTOR_LEN;
    const ptr1 = passArray8ToWasm0(plaintext, wasm.__wbindgen_malloc);
    const len1 = WASM_VECTOR_LEN;
    const ptr2 = passStringToWasm0(mime_type, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len2 = WASM_VECTOR_LEN;
    const ptr3 = passStringToWasm0(file_name, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len3 = WASM_VECTOR_LEN;
    const ret = wasm.encrypt_file(ptr0, len0, ptr1, len1, ptr2, len2, ptr3, len3);
    if (ret[2]) {
        throw takeFromExternrefTable0(ret[1]);
    }
    return WebFileEncryption.__wrap(ret[0]);
}
function __wbg_get_imports() {
    const import0 = {
        __proto__: null,
        __wbg___wbindgen_is_function_fcda5e3902d732fe: function(arg0) {
            const ret = typeof(arg0) === 'function';
            return ret;
        },
        __wbg___wbindgen_is_object_edb6b15aa3afe12e: function(arg0) {
            const val = arg0;
            const ret = typeof(val) === 'object' && val !== null;
            return ret;
        },
        __wbg___wbindgen_is_string_c4f7cb494a2a21f1: function(arg0) {
            const ret = typeof(arg0) === 'string';
            return ret;
        },
        __wbg___wbindgen_is_undefined_8c687d0b90d5b524: function(arg0) {
            const ret = arg0 === undefined;
            return ret;
        },
        __wbg___wbindgen_throw_5d9e815e6fdf150f: function(arg0, arg1) {
            throw new Error(getStringFromWasm0(arg0, arg1));
        },
        __wbg_call_6bcf8d3e20937e46: function() { return handleError(function (arg0, arg1, arg2) {
            const ret = arg0.call(arg1, arg2);
            return ret;
        }, arguments); },
        __wbg_crypto_38df2bab126b63dc: function(arg0) {
            const ret = arg0.crypto;
            return ret;
        },
        __wbg_getRandomValues_436a51d0629d84e1: function() { return handleError(function (arg0, arg1) {
            globalThis.crypto.getRandomValues(getArrayU8FromWasm0(arg0, arg1));
        }, arguments); },
        __wbg_getRandomValues_a678b7300e8ed57f: function() { return handleError(function (arg0, arg1) {
            globalThis.crypto.getRandomValues(getArrayU8FromWasm0(arg0, arg1));
        }, arguments); },
        __wbg_getRandomValues_c44a50d8cfdaebeb: function() { return handleError(function (arg0, arg1) {
            arg0.getRandomValues(arg1);
        }, arguments); },
        __wbg_length_31bdaf014f5fbde2: function(arg0) {
            const ret = arg0.length;
            return ret;
        },
        __wbg_msCrypto_bd5a034af96bcba6: function(arg0) {
            const ret = arg0.msCrypto;
            return ret;
        },
        __wbg_new_ffa92086ea89f79c: function() {
            const ret = new Array();
            return ret;
        },
        __wbg_new_from_slice_4ee02165f9de919e: function(arg0, arg1) {
            const ret = new Uint8Array(getArrayU8FromWasm0(arg0, arg1));
            return ret;
        },
        __wbg_new_with_length_5ffeddb9d9fbb96f: function(arg0) {
            const ret = new Uint8Array(arg0 >>> 0);
            return ret;
        },
        __wbg_node_84ea875411254db1: function(arg0) {
            const ret = arg0.node;
            return ret;
        },
        __wbg_now_d1fb6650485d7f3e: function() {
            const ret = Date.now();
            return ret;
        },
        __wbg_process_44c7a14e11e9f69e: function(arg0) {
            const ret = arg0.process;
            return ret;
        },
        __wbg_prototypesetcall_ae9f5e7459250748: function(arg0, arg1, arg2) {
            Uint8Array.prototype.set.call(getArrayU8FromWasm0(arg0, arg1), arg2);
        },
        __wbg_push_bfdf956ba476f65b: function(arg0, arg1) {
            const ret = arg0.push(arg1);
            return ret;
        },
        __wbg_randomFillSync_6c25eac9869eb53c: function() { return handleError(function (arg0, arg1) {
            arg0.randomFillSync(arg1);
        }, arguments); },
        __wbg_random_7bce4e9e78e2e0d0: function() {
            const ret = Math.random();
            return ret;
        },
        __wbg_require_b4edbdcf3e2a1ef0: function() { return handleError(function () {
            const ret = module.require;
            return ret;
        }, arguments); },
        __wbg_static_accessor_GLOBAL_8eb4cd83130a11a0: function() {
            const ret = typeof global === 'undefined' ? null : global;
            return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
        },
        __wbg_static_accessor_GLOBAL_THIS_1e7044f654e934db: function() {
            const ret = typeof globalThis === 'undefined' ? null : globalThis;
            return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
        },
        __wbg_static_accessor_SELF_d8b50611246a6d92: function() {
            const ret = typeof self === 'undefined' ? null : self;
            return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
        },
        __wbg_static_accessor_WINDOW_fd0bc376bf0f8b42: function() {
            const ret = typeof window === 'undefined' ? null : window;
            return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
        },
        __wbg_subarray_1daff70dde20c145: function(arg0, arg1, arg2) {
            const ret = arg0.subarray(arg1 >>> 0, arg2 >>> 0);
            return ret;
        },
        __wbg_versions_276b2795b1c6a219: function(arg0) {
            const ret = arg0.versions;
            return ret;
        },
        __wbindgen_generic_0000000000000001: function(arg0, arg1) {
            // Cast intrinsic for `Ref(Slice(U8)) -> NamedExternref("Uint8Array")`.
            const ret = getArrayU8FromWasm0(arg0, arg1);
            return ret;
        },
        __wbindgen_generic_0000000000000002: function(arg0, arg1) {
            // Cast intrinsic for `Ref(String) -> Externref`.
            const ret = getStringFromWasm0(arg0, arg1);
            return ret;
        },
        __wbindgen_init_externref_table: function() {
            const table = wasm.__wbindgen_externrefs;
            const offset = table.grow(4);
            table.set(0, undefined);
            table.set(offset + 0, undefined);
            table.set(offset + 1, null);
            table.set(offset + 2, true);
            table.set(offset + 3, false);
        },
    };
    return {
        __proto__: null,
        "./links_web_client_bg.js": import0,
    };
}

const WebClientIdentityFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_webclientidentity_free(ptr, 1));
const WebFileEncryptionFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_webfileencryption_free(ptr, 1));
const WebLargeFileDecryptorFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_weblargefiledecryptor_free(ptr, 1));
const WebLargeFileEncryptorFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_weblargefileencryptor_free(ptr, 1));
const WebLargeFileMetadataFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_weblargefilemetadata_free(ptr, 1));
const WebMessagingCoreFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_webmessagingcore_free(ptr, 1));
const WebRtcSignalDeliveryFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_webrtcsignaldelivery_free(ptr, 1));
const WebSelfSovereignIdentityFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_webselfsovereignidentity_free(ptr, 1));

function addToExternrefTable0(obj) {
    const idx = wasm.__externref_table_alloc();
    wasm.__wbindgen_externrefs.set(idx, obj);
    return idx;
}

function getArrayU8FromWasm0(ptr, len) {
    ptr = ptr >>> 0;
    return getUint8ArrayMemory0().subarray(ptr / 1, ptr / 1 + len);
}

function getStringFromWasm0(ptr, len) {
    return decodeText(ptr >>> 0, len);
}

let cachedUint8ArrayMemory0 = null;
function getUint8ArrayMemory0() {
    if (cachedUint8ArrayMemory0 === null || cachedUint8ArrayMemory0.byteLength === 0) {
        cachedUint8ArrayMemory0 = new Uint8Array(wasm.memory.buffer);
    }
    return cachedUint8ArrayMemory0;
}

function handleError(f, args) {
    try {
        return f.apply(this, args);
    } catch (e) {
        const idx = addToExternrefTable0(e);
        wasm.__wbindgen_exn_store(idx);
    }
}

function isLikeNone(x) {
    return x === undefined || x === null;
}

function passArray8ToWasm0(arg, malloc) {
    const ptr = malloc(arg.length * 1, 1) >>> 0;
    getUint8ArrayMemory0().set(arg, ptr / 1);
    WASM_VECTOR_LEN = arg.length;
    return ptr;
}

function passStringToWasm0(arg, malloc, realloc) {
    if (realloc === undefined) {
        const buf = cachedTextEncoder.encode(arg);
        const ptr = malloc(buf.length, 1) >>> 0;
        getUint8ArrayMemory0().subarray(ptr, ptr + buf.length).set(buf);
        WASM_VECTOR_LEN = buf.length;
        return ptr;
    }

    let len = arg.length;
    let ptr = malloc(len, 1) >>> 0;

    const mem = getUint8ArrayMemory0();

    let offset = 0;

    for (; offset < len; offset++) {
        const code = arg.charCodeAt(offset);
        if (code > 0x7F) break;
        mem[ptr + offset] = code;
    }
    if (offset !== len) {
        if (offset !== 0) {
            arg = arg.slice(offset);
        }
        ptr = realloc(ptr, len, len = offset + arg.length * 3, 1) >>> 0;
        const view = getUint8ArrayMemory0().subarray(ptr + offset, ptr + len);
        const ret = cachedTextEncoder.encodeInto(arg, view);

        offset += ret.written;
        ptr = realloc(ptr, len, offset, 1) >>> 0;
    }

    WASM_VECTOR_LEN = offset;
    return ptr;
}

function takeFromExternrefTable0(idx) {
    const value = wasm.__wbindgen_externrefs.get(idx);
    wasm.__externref_table_dealloc(idx);
    return value;
}

let cachedTextDecoder = new TextDecoder('utf-8', { ignoreBOM: true, fatal: true });
cachedTextDecoder.decode();
const MAX_SAFARI_DECODE_BYTES = 2146435072;
let numBytesDecoded = 0;
function decodeText(ptr, len) {
    numBytesDecoded += len;
    if (numBytesDecoded >= MAX_SAFARI_DECODE_BYTES) {
        cachedTextDecoder = new TextDecoder('utf-8', { ignoreBOM: true, fatal: true });
        cachedTextDecoder.decode();
        numBytesDecoded = len;
    }
    return cachedTextDecoder.decode(getUint8ArrayMemory0().subarray(ptr, ptr + len));
}

const cachedTextEncoder = new TextEncoder();

if (!('encodeInto' in cachedTextEncoder)) {
    cachedTextEncoder.encodeInto = function (arg, view) {
        const buf = cachedTextEncoder.encode(arg);
        view.set(buf);
        return {
            read: arg.length,
            written: buf.length
        };
    };
}

let WASM_VECTOR_LEN = 0;

let wasmModule, wasmInstance, wasm;
function __wbg_finalize_init(instance, module) {
    wasmInstance = instance;
    wasm = instance.exports;
    wasmModule = module;
    cachedUint8ArrayMemory0 = null;
    wasm.__wbindgen_start();
    return wasm;
}

async function __wbg_load(module, imports) {
    if (typeof Response === 'function' && module instanceof Response) {
        if (!module.ok) {
            throw new Error(`failed to fetch Wasm: ${module.status} ${module.statusText} fetching '${module.url}'`);
        }

        if (typeof WebAssembly.instantiateStreaming === 'function') {
            try {
                return await WebAssembly.instantiateStreaming(module, imports);
            } catch (e) {
                const validResponse = expectedResponseType(module.type);

                if (validResponse && module.headers.get('Content-Type') !== 'application/wasm') {
                    console.warn("`WebAssembly.instantiateStreaming` failed because your server does not serve Wasm with `application/wasm` MIME type. Falling back to `WebAssembly.instantiate` which is slower. Original error:\n", e);

                } else { throw e; }
            }
        }

        const bytes = await module.arrayBuffer();
        return await WebAssembly.instantiate(bytes, imports);
    } else {
        const instance = await WebAssembly.instantiate(module, imports);

        if (instance instanceof WebAssembly.Instance) {
            return { instance, module };
        } else {
            return instance;
        }
    }

    function expectedResponseType(type) {
        switch (type) {
            case 'basic': case 'cors': case 'default': return true;
        }
        return false;
    }
}

function initSync(module) {
    if (wasm !== undefined) return wasm;


    if (module !== undefined) {
        if (Object.getPrototypeOf(module) === Object.prototype) {
            ({module} = module)
        } else {
            console.warn('using deprecated parameters for `initSync()`; pass a single object instead')
        }
    }

    const imports = __wbg_get_imports();
    if (!(module instanceof WebAssembly.Module)) {
        module = new WebAssembly.Module(module);
    }
    const instance = new WebAssembly.Instance(module, imports);
    return __wbg_finalize_init(instance, module);
}

async function __wbg_init(module_or_path) {
    if (wasm !== undefined) return wasm;


    if (module_or_path !== undefined) {
        if (Object.getPrototypeOf(module_or_path) === Object.prototype) {
            ({module_or_path} = module_or_path)
        } else {
            console.warn('using deprecated parameters for the initialization function; pass a single object instead')
        }
    }

    if (module_or_path === undefined) {
        module_or_path = new URL('links_web_client_bg.wasm', import.meta.url);
    }
    const imports = __wbg_get_imports();

    if (typeof module_or_path === 'string' || (typeof Request === 'function' && module_or_path instanceof Request) || (typeof URL === 'function' && module_or_path instanceof URL)) {
        module_or_path = fetch(module_or_path);
    }

    const { instance, module } = await __wbg_load(await module_or_path, imports);

    return __wbg_finalize_init(instance, module);
}

export { initSync, __wbg_init as default };
