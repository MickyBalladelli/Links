#include <jni.h>
#include <stdlib.h>
#include <string.h>
#include "links_identity.h"

/* Stack-only context, valid on the entering thread until the Rust call returns. */
typedef struct {
    JNIEnv *env;
    jobject vault;
    jmethodID store, load, delete_seed;
} VaultContext;

/* Wipe managed seed arrays even with a pending Java exception. Preserve the
 * original exception after cleanup; no JNI critical/pinned arrays are used. */
static void wipe_seed(JNIEnv *env, jbyteArray bytes) {
    if (!bytes) return;
    jthrowable pending = (*env)->ExceptionOccurred(env);
    if (pending) (*env)->ExceptionClear(env);
    jsize length = (*env)->GetArrayLength(env, bytes);
    const jbyte zeros[32] = {0};
    for (jsize offset = 0; offset < length;) {
        jsize count = length - offset < 32 ? length - offset : 32;
        (*env)->SetByteArrayRegion(env, bytes, offset, count, zeros);
        if ((*env)->ExceptionCheck(env)) break;
        offset += count;
    }
    if (pending) {
        if ((*env)->ExceptionCheck(env)) (*env)->ExceptionClear(env);
        (*env)->Throw(env, pending);
        (*env)->DeleteLocalRef(env, pending);
    }
    (*env)->DeleteLocalRef(env, bytes);
}
static jstring handle_string(JNIEnv *env, const uint8_t *handle) {
    char text[37];
    memcpy(text, handle, 36);
    text[36] = '\0';
    return (*env)->NewStringUTF(env, text);
}
static int32_t store_seed(void *opaque, const uint8_t *seed, uint8_t *handle) {
    VaultContext *ctx = opaque;
    JNIEnv *env = ctx->env;
    jbyteArray bytes = (*env)->NewByteArray(env, 32);
    if (!bytes) return LINKS_PROVIDER;
    (*env)->SetByteArrayRegion(env, bytes, 0, 32, (const jbyte *)seed);
    jstring result = NULL;
    if (!(*env)->ExceptionCheck(env))
        result = (jstring)(*env)->CallObjectMethod(env, ctx->vault, ctx->store, bytes);
    wipe_seed(env, bytes);
    if ((*env)->ExceptionCheck(env) || !result) return LINKS_PROVIDER;
    int32_t status = LINKS_PROVIDER;
    if ((*env)->GetStringUTFLength(env, result) == 36) {
        const char *text = (*env)->GetStringUTFChars(env, result, NULL);
        if (text) {
            memcpy(handle, text, 36);
            (*env)->ReleaseStringUTFChars(env, result, text);
            status = LINKS_OK;
        }
    }
    (*env)->DeleteLocalRef(env, result);
    return status;
}
static int32_t load_seed(void *opaque, const uint8_t *handle, uint8_t *seed) {
    VaultContext *ctx = opaque;
    JNIEnv *env = ctx->env;
    jstring text = handle_string(env, handle);
    if (!text) return LINKS_PROVIDER;
    jbyteArray result = (jbyteArray)(*env)->CallObjectMethod(env, ctx->vault, ctx->load, text);
    (*env)->DeleteLocalRef(env, text);
    int32_t status = LINKS_PROVIDER;
    if (!(*env)->ExceptionCheck(env) && result && (*env)->GetArrayLength(env, result) == 32) {
        (*env)->GetByteArrayRegion(env, result, 0, 32, (jbyte *)seed);
        if (!(*env)->ExceptionCheck(env)) status = LINKS_OK;
    }
    wipe_seed(env, result);
    return status;
}
static int32_t delete_seed(void *opaque, const uint8_t *handle) {
    VaultContext *ctx = opaque;
    JNIEnv *env = ctx->env;
    /* A failed create readback can call delete with a pending load exception. */
    jthrowable pending = (*env)->ExceptionOccurred(env);
    if (pending) (*env)->ExceptionClear(env);
    jstring text = handle_string(env, handle);
    if (text) {
        (*env)->CallVoidMethod(env, ctx->vault, ctx->delete_seed, text);
        (*env)->DeleteLocalRef(env, text);
    }
    int32_t status = (*env)->ExceptionCheck(env) ? LINKS_PROVIDER : LINKS_OK;
    if (pending) {
        if ((*env)->ExceptionCheck(env)) (*env)->ExceptionClear(env);
        (*env)->Throw(env, pending);
        (*env)->DeleteLocalRef(env, pending);
    }
    return status;
}
static void throw_status(JNIEnv *env, int32_t status) {
    if (status == LINKS_OK || (*env)->ExceptionCheck(env)) return;
    jclass error = (*env)->FindClass(env, "java/security/GeneralSecurityException");
    if (error) {
        (*env)->ThrowNew(env, error, status == LINKS_INVALID ? "Invalid identity input" :
            status == LINKS_AUTHENTICATION ? "Identity authentication failed" : "Hardware identity operation failed");
        (*env)->DeleteLocalRef(env, error);
    }
}
static int setup(JNIEnv *env, jobject vault, VaultContext *ctx, LinksVaultCallbacks *callbacks) {
    if (!vault) { throw_status(env, LINKS_INVALID); return 0; }
    ctx->env = env;
    ctx->vault = vault;
    jclass cls = (*env)->GetObjectClass(env, vault);
    if (!cls) return 0;
    ctx->store = (*env)->GetMethodID(env, cls, "storeSeed", "([B)Ljava/lang/String;");
    if (!(*env)->ExceptionCheck(env)) ctx->load = (*env)->GetMethodID(env, cls, "loadSeed", "(Ljava/lang/String;)[B");
    if (!(*env)->ExceptionCheck(env)) ctx->delete_seed = (*env)->GetMethodID(env, cls, "deleteSeed", "(Ljava/lang/String;)V");
    (*env)->DeleteLocalRef(env, cls);
    if ((*env)->ExceptionCheck(env)) return 0;
    *callbacks = (LinksVaultCallbacks){1, ctx, store_seed, load_seed, delete_seed};
    return 1;
}
static int read_fixed(JNIEnv *env, jbyteArray input, uint8_t *output, jsize size) {
    if (!input || (*env)->GetArrayLength(env, input) != size) {
        throw_status(env, LINKS_INVALID); return 0;
    }
    (*env)->GetByteArrayRegion(env, input, 0, size, (jbyte *)output);
    return !(*env)->ExceptionCheck(env);
}
static int read_bounded(JNIEnv *env, jbyteArray input, uint8_t *output, jsize maximum, jsize *length) {
    if (!input) {
        throw_status(env, LINKS_INVALID);
        return 0;
    }
    jsize actual = (*env)->GetArrayLength(env, input);
    if (actual > maximum) {
        throw_status(env, LINKS_INVALID);
        return 0;
    }
    if (actual) (*env)->GetByteArrayRegion(env, input, 0, actual, (jbyte *)output);
    if ((*env)->ExceptionCheck(env)) return 0;
    *length = actual;
    return 1;
}
static int read_variable(JNIEnv *env, jbyteArray input, jbyte **output, jsize *length, jsize maximum) {
    if (!input) {
        throw_status(env, LINKS_INVALID);
        return 0;
    }
    *length = (*env)->GetArrayLength(env, input);
    if (*length > maximum) {
        throw_status(env, LINKS_INVALID);
        return 0;
    }
    *output = (*env)->GetByteArrayElements(env, input, NULL);
    if (!*output && *length != 0) {
        throw_status(env, LINKS_PROVIDER);
        return 0;
    }
    return !(*env)->ExceptionCheck(env);
}
static void release_variable(JNIEnv *env, jbyteArray input, jbyte *output) {
    if (input && output) (*env)->ReleaseByteArrayElements(env, input, output, JNI_ABORT);
}
static jbyteArray result_array(JNIEnv *env, int32_t status, const uint8_t *data, jsize size) {
    throw_status(env, status);
    if (status != LINKS_OK || (*env)->ExceptionCheck(env)) return NULL;
    jbyteArray output = (*env)->NewByteArray(env, size);
    if (output) (*env)->SetByteArrayRegion(env, output, 0, size, (const jbyte *)data);
    return output;
}
JNIEXPORT jbyteArray JNICALL Java_ai_links_identity_NativeIdentityBridge_create(JNIEnv *env, jclass cls, jobject vault) {
    (void)cls;
    VaultContext ctx = {0}; LinksVaultCallbacks callbacks;
    if (!setup(env, vault, &ctx, &callbacks)) return NULL;
    uint8_t reference[68] = {0};
    /* Allocate before creating persistent keys so allocation failure cannot orphan
     * an otherwise successful identity without returning its handle. */
    jbyteArray output = (*env)->NewByteArray(env, 68);
    if (!output) return NULL;
    int32_t status = links_identity_create(&callbacks, reference, reference + 36);
    throw_status(env, status);
    if (status != LINKS_OK || (*env)->ExceptionCheck(env)) return NULL;
    (*env)->SetByteArrayRegion(env, output, 0, 68, (const jbyte *)reference);
    return output;
}
JNIEXPORT jbyteArray JNICALL Java_ai_links_identity_NativeIdentityBridge_restoreFromRecovery(
        JNIEnv *env, jclass cls, jobject vault, jbyteArray phrase_input, jbyteArray passphrase_input) {
    (void)cls;
    VaultContext ctx = {0}; LinksVaultCallbacks callbacks;
    if (!setup(env, vault, &ctx, &callbacks)) return NULL;
    jbyte *phrase = NULL, *passphrase = NULL;
    jsize phrase_len = 0, passphrase_len = 0;
    if (!read_variable(env, phrase_input, &phrase, &phrase_len, LINKS_IDENTITY_MAX_RECOVERY_PHRASE)) return NULL;
    if (!read_variable(env, passphrase_input, &passphrase, &passphrase_len,
            LINKS_IDENTITY_MAX_RECOVERY_PASSPHRASE)) {
        release_variable(env, phrase_input, phrase);
        return NULL;
    }
    uint8_t reference[68] = {0};
    int32_t status = links_identity_restore_from_mnemonic(&callbacks,
            (const uint8_t *)phrase, (size_t)phrase_len,
            (const uint8_t *)passphrase, (size_t)passphrase_len,
            reference, reference + 36);
    release_variable(env, passphrase_input, passphrase);
    release_variable(env, phrase_input, phrase);
    return result_array(env, status, reference, 68);
}
JNIEXPORT jbyteArray JNICALL Java_ai_links_identity_NativeIdentityBridge_backupWithPasskey(
        JNIEnv *env, jclass cls, jobject vault, jbyteArray handle_input,
        jbyteArray backup_input, jbyteArray device_input, jbyteArray credential_input,
        jbyteArray salt_input, jbyteArray prf_input) {
    (void)cls;
    VaultContext ctx = {0}; LinksVaultCallbacks callbacks;
    uint8_t handle[36], backup[16], device[16], salt[32], prf[32];
    if (!read_fixed(env, handle_input, handle, 36)
            || !read_fixed(env, backup_input, backup, 16)
            || !read_fixed(env, device_input, device, 16)
            || !read_fixed(env, salt_input, salt, 32)
            || !read_fixed(env, prf_input, prf, 32)
            || !setup(env, vault, &ctx, &callbacks)) return NULL;
    jbyte *credential = NULL; jsize credential_len = 0;
    if (!read_variable(env, credential_input, &credential, &credential_len,
            LINKS_IDENTITY_MAX_CREDENTIAL_ID)) return NULL;
    uint8_t envelope[LINKS_IDENTITY_MAX_BACKUP_ENVELOPE] = {0};
    size_t envelope_len = 0;
    int32_t status = links_identity_backup_with_passkey(&callbacks, handle, backup, device,
            (const uint8_t *)credential, (size_t)credential_len, salt, prf,
            envelope, sizeof(envelope), &envelope_len);
    release_variable(env, credential_input, credential);
    if (envelope_len > LINKS_IDENTITY_MAX_BACKUP_ENVELOPE) {
        throw_status(env, LINKS_PROVIDER);
        return NULL;
    }
    return result_array(env, status, envelope, (jsize)envelope_len);
}
JNIEXPORT jbyteArray JNICALL Java_ai_links_identity_NativeIdentityBridge_restoreFromPasskey(
        JNIEnv *env, jclass cls, jobject vault, jbyteArray backup_input, jbyteArray device_input,
        jbyteArray credential_input, jbyteArray envelope_input, jbyteArray prf_input) {
    (void)cls;
    VaultContext ctx = {0}; LinksVaultCallbacks callbacks;
    uint8_t backup[16], device[16], prf[32];
    if (!read_fixed(env, backup_input, backup, 16)
            || !read_fixed(env, device_input, device, 16)
            || !read_fixed(env, prf_input, prf, 32)
            || !setup(env, vault, &ctx, &callbacks)) return NULL;
    jbyte *credential = NULL, *envelope = NULL;
    jsize credential_len = 0, envelope_len = 0;
    if (!read_variable(env, credential_input, &credential, &credential_len,
            LINKS_IDENTITY_MAX_CREDENTIAL_ID)) return NULL;
    if (!read_variable(env, envelope_input, &envelope, &envelope_len,
            LINKS_IDENTITY_MAX_BACKUP_ENVELOPE)) {
        release_variable(env, credential_input, credential);
        return NULL;
    }
    uint8_t reference[68] = {0};
    int32_t status = links_identity_restore_from_passkey(&callbacks, backup, device,
            (const uint8_t *)credential, (size_t)credential_len,
            (const uint8_t *)envelope, (size_t)envelope_len, prf,
            reference, reference + 36);
    release_variable(env, envelope_input, envelope);
    release_variable(env, credential_input, credential);
    return result_array(env, status, reference, 68);
}
JNIEXPORT jbyteArray JNICALL Java_ai_links_identity_NativeIdentityBridge_publicKey(JNIEnv *env, jclass cls, jobject vault, jbyteArray input) {
    (void)cls;
    VaultContext ctx = {0}; LinksVaultCallbacks callbacks;
    uint8_t handle[36], public_key[32];
    if (!read_fixed(env, input, handle, 36) || !setup(env, vault, &ctx, &callbacks)) return NULL;
    int32_t status = links_identity_public_key(&callbacks, handle, public_key);
    return result_array(env, status, public_key, 32);
}
JNIEXPORT jbyteArray JNICALL Java_ai_links_identity_NativeIdentityBridge_sign(JNIEnv *env, jclass cls, jobject vault, jbyteArray input, jbyteArray public_input, jbyteArray message_input) {
    (void)cls;
    VaultContext ctx = {0}; LinksVaultCallbacks callbacks;
    uint8_t handle[36], public_key[32], signature[64];
    if (!read_fixed(env, input, handle, 36) || !read_fixed(env, public_input, public_key, 32)) return NULL;
    if (!message_input) { throw_status(env, LINKS_INVALID); return NULL; }
    jsize size = (*env)->GetArrayLength(env, message_input);
    if (size > LINKS_IDENTITY_MAX_MESSAGE) { throw_status(env, LINKS_INVALID); return NULL; }
    if (!setup(env, vault, &ctx, &callbacks)) return NULL;
    uint8_t *message = size ? malloc((size_t)size) : NULL;
    if (size && !message) { throw_status(env, LINKS_PROVIDER); return NULL; }
    if (size) (*env)->GetByteArrayRegion(env, message_input, 0, size, (jbyte *)message);
    int32_t status = LINKS_PROVIDER;
    if (!(*env)->ExceptionCheck(env)) status = links_identity_sign(&callbacks, handle, public_key, message, (size_t)size, signature);
    free(message);
    return result_array(env, status, signature, 64);
}
JNIEXPORT void JNICALL Java_ai_links_identity_NativeIdentityBridge_delete(JNIEnv *env, jclass cls, jobject vault, jbyteArray input) {
    (void)cls;
    VaultContext ctx = {0}; LinksVaultCallbacks callbacks;
    uint8_t handle[36];
    if (!read_fixed(env, input, handle, 36) || !setup(env, vault, &ctx, &callbacks)) return;
    throw_status(env, links_identity_delete(&callbacks, handle));
}
JNIEXPORT jbyteArray JNICALL Java_ai_links_identity_NativeIdentityBridge_phoneAuthTranscript(
        JNIEnv *env, jclass cls, jbyteArray phone_input, jbyteArray channel_input,
        jbyteArray device_input, jbyteArray node_input, jbyteArray public_input) {
    (void)cls;
    uint8_t phone[16], channel[8], device[16], node[16], public_key[32];
    jsize phone_len, channel_len;
    if (!read_bounded(env, phone_input, phone, 16, &phone_len)
            || !read_bounded(env, channel_input, channel, 8, &channel_len)
            || !read_fixed(env, device_input, device, 16)
            || !read_fixed(env, node_input, node, 16)
            || !read_fixed(env, public_input, public_key, 32)) return NULL;
    uint8_t output[LINKS_IDENTITY_MAX_TRANSCRIPT];
    size_t output_len = 0;
    int32_t status = links_phone_auth_transcript(phone, (size_t)phone_len, channel,
            (size_t)channel_len, device, node, public_key, output,
            sizeof(output), &output_len);
    if (output_len > LINKS_IDENTITY_MAX_TRANSCRIPT) {
        throw_status(env, LINKS_PROVIDER);
        return NULL;
    }
    return result_array(env, status, output, (jsize)output_len);
}
JNIEXPORT jbyteArray JNICALL Java_ai_links_identity_NativeIdentityBridge_enrollmentTranscript(
        JNIEnv *env, jclass cls, jbyteArray user_input, jbyteArray device_input,
        jbyteArray node_input, jbyteArray public_input, jbyteArray challenge_input,
        jbyteArray nonce_input, jlong expires_at_ms, jbyteArray credential_input) {
    (void)cls;
    uint8_t user[16], device[16], node[16], public_key[32], challenge[16], nonce[32];
    if (!read_fixed(env, user_input, user, 16)
            || !read_fixed(env, device_input, device, 16)
            || !read_fixed(env, node_input, node, 16)
            || !read_fixed(env, public_input, public_key, 32)
            || !read_fixed(env, challenge_input, challenge, 16)
            || !read_fixed(env, nonce_input, nonce, 32)) return NULL;
    uint8_t credential[LINKS_IDENTITY_MAX_TRANSCRIPT];
    jsize credential_len;
    if (!read_bounded(env, credential_input, credential, LINKS_IDENTITY_MAX_TRANSCRIPT,
            &credential_len)) return NULL;
    uint8_t output[LINKS_IDENTITY_MAX_TRANSCRIPT];
    size_t output_len = 0;
    int32_t status = links_enrollment_transcript(user, device, node, public_key,
            challenge, nonce, (uint64_t)expires_at_ms, credential,
            (size_t)credential_len, output, sizeof(output), &output_len);
    if (output_len > LINKS_IDENTITY_MAX_TRANSCRIPT) {
        throw_status(env, LINKS_PROVIDER);
        return NULL;
    }
    return result_array(env, status, output, (jsize)output_len);
}
