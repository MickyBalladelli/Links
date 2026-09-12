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
