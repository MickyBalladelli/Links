#ifndef LINKS_IDENTITY_H
#define LINKS_IDENTITY_H
#include <stdint.h>
#include <stddef.h>
#ifdef __cplusplus
extern "C" {
#endif

enum { LINKS_OK = 0, LINKS_INVALID = 1, LINKS_UNAVAILABLE = 2,
       LINKS_AUTHENTICATION = 3, LINKS_PROVIDER = 4 };
#define LINKS_IDENTITY_MAX_MESSAGE (1024 * 1024)
#define LINKS_IDENTITY_MAX_TRANSCRIPT 1024

/* ABI v1, synchronous only, called on the entering thread. No pointers retained.
 * Native callbacks must enforce hardware backing and catch all exceptions.
 * store: seed[32] -> canonical lowercase UUID handle[36], no NUL terminator.
 * load: handle[36] -> seed[32]. delete: handle[36]. Return LINKS_* status.
 * On store failure, native code owns cleanup of any partial record/key.
 * Callbacks must wipe any seed copies, including on failure. They must not log,
 * persist plaintext, retain pointers, reenter this API, or unwind through C.
 * HardwareIdentityStore validates the seed against the enrolled key when signing.
 */
typedef struct {
    uint32_t abi_version;
    void *context;
    int32_t (*store)(void *, const uint8_t *seed, uint8_t *handle);
    int32_t (*load)(void *, const uint8_t *handle, uint8_t *seed);
    int32_t (*delete_seed)(void *, const uint8_t *handle);
} LinksVaultCallbacks;

/* Caller owns every buffer. Non-null pointers must be aligned/live and readable
 * or writable for the stated sizes. Outputs must not overlap each other or inputs.
 * Outputs are zeroed on error (except invalid/null output pointers). Table/context
 * live until return. Serialize operations through one app identity worker.
 * Persist only handle + public key (with the account binding) before enrollment.
 * There is deliberately no plaintext-seed export in this API.
 */
int32_t links_identity_create(const LinksVaultCallbacks *, uint8_t *handle36, uint8_t *public32);
int32_t links_identity_public_key(const LinksVaultCallbacks *, const uint8_t *handle36, uint8_t *public32);
int32_t links_identity_sign(const LinksVaultCallbacks *, const uint8_t *handle36,
                            const uint8_t *expected_public32, const uint8_t *message,
                            size_t message_len, uint8_t *signature64);
int32_t links_identity_delete(const LinksVaultCallbacks *, const uint8_t *handle36);
int32_t links_phone_auth_transcript(const uint8_t *phone, size_t phone_len,
                                    const uint8_t *channel, size_t channel_len,
                                    const uint8_t *device16, const uint8_t *node16,
                                    const uint8_t *public32, uint8_t *output,
                                    size_t output_capacity, size_t *output_len);
int32_t links_enrollment_transcript(const uint8_t *user16, const uint8_t *device16,
                                    const uint8_t *node16, const uint8_t *public32,
                                    const uint8_t *challenge16, const uint8_t *nonce32,
                                    uint64_t expires_at_ms, const uint8_t *credential,
                                    size_t credential_len, uint8_t *output,
                                    size_t output_capacity, size_t *output_len);
#ifdef __cplusplus
}
#endif
#endif
