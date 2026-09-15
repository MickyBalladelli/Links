#ifndef LINKS_DESKTOP_CLIENT_H
#define LINKS_DESKTOP_CLIENT_H

#include <stddef.h>
#include <stdint.h>

typedef struct LinksDesktopCore LinksDesktopCore;

typedef int32_t (*LinksDesktopSignCallback)(void *context, const uint8_t *bytes,
                                             size_t length, uint8_t *signature64);
typedef int32_t (*LinksDesktopStoreSecretCallback)(void *context, const uint8_t *key,
                                                    size_t key_length, const uint8_t *secret,
                                                    size_t secret_length);
typedef int32_t (*LinksDesktopLoadSecretCallback)(void *context, const uint8_t *key,
                                                   size_t key_length, uint8_t *output,
                                                   size_t capacity, size_t *output_length);
typedef int32_t (*LinksDesktopDeleteSecretCallback)(void *context, const uint8_t *key,
                                                     size_t key_length);
typedef int32_t (*LinksDesktopStateCallback)(void *context, uint8_t *output,
                                              size_t capacity, size_t *output_length);
typedef int32_t (*LinksDesktopSaveStateCallback)(void *context, const uint8_t *bytes,
                                                  size_t length);
typedef int32_t (*LinksDesktopSendFrameCallback)(void *context, const uint8_t *bytes,
                                                  size_t length);
typedef int32_t (*LinksDesktopTextCallback)(void *context, const uint8_t *conversation,
                                             size_t conversation_length, const uint8_t *sender,
                                             size_t sender_length, const uint8_t *text,
                                             size_t text_length, uint64_t sequence_id,
                                             uint64_t sent_at_ms);

typedef struct LinksDesktopCoreCallbacks {
    uint32_t abi_version;
    void *context;
    LinksDesktopSignCallback sign;
    LinksDesktopStoreSecretCallback store_secret;
    LinksDesktopLoadSecretCallback load_secret;
    LinksDesktopDeleteSecretCallback delete_secret;
    LinksDesktopStateCallback load_state;
    LinksDesktopSaveStateCallback save_state;
    LinksDesktopSendFrameCallback send_frame;
    LinksDesktopTextCallback on_text;
    uint8_t identity_public_key[32];
} LinksDesktopCoreCallbacks;

enum {
    LINKS_DESKTOP_OK = 0,
    LINKS_DESKTOP_INVALID = 1,
    LINKS_DESKTOP_UNAVAILABLE = 2,
    LINKS_DESKTOP_AUTHENTICATION = 3,
    LINKS_DESKTOP_PROVIDER = 4,
    LINKS_DESKTOP_STALE_CURSOR = 5
};

int32_t links_desktop_core_create(const uint8_t *user_id, size_t user_id_length,
                                  const uint8_t *device_id, size_t device_id_length,
                                  const uint8_t *credential, size_t credential_length,
                                  const LinksDesktopCoreCallbacks *callbacks,
                                  LinksDesktopCore **output);
void links_desktop_core_destroy(LinksDesktopCore *core);
int32_t links_desktop_core_durable_cursor(const LinksDesktopCore *core, uint64_t *cursor);
int32_t links_desktop_core_pending_outbox_count(const LinksDesktopCore *core, size_t *count);
int32_t links_desktop_core_pending_retry_count(const LinksDesktopCore *core, size_t *count);
int32_t links_desktop_core_create_hello(const LinksDesktopCore *core, const uint8_t *token,
                                        size_t token_length, uint64_t cursor, uint8_t *output,
                                        size_t capacity, size_t *length);
int32_t links_desktop_core_handle_server_frame(LinksDesktopCore *core, const uint8_t *frame,
                                                size_t frame_length);
int32_t links_desktop_core_retry_outbox(LinksDesktopCore *core);
int32_t links_desktop_core_reset_replay_cursor(LinksDesktopCore *core);
int32_t links_desktop_core_generate_mls_key_package(LinksDesktopCore *core, uint8_t *output,
                                                    size_t capacity, size_t *length);
int32_t links_desktop_core_generate_prekey_upload(LinksDesktopCore *core, uint32_t curve_count,
                                                   uint32_t kem_count, uint8_t *output,
                                                   size_t capacity, size_t *length);
int32_t links_desktop_core_set_recipient(
    LinksDesktopCore *core, const uint8_t *user_id, size_t user_id_length,
    const uint8_t *device_id, size_t device_id_length, const uint8_t *identity_public_key,
    size_t identity_public_key_length, const uint8_t *prekey_bundle, size_t prekey_bundle_length,
    const uint8_t *mls_credential, size_t mls_credential_length, const uint8_t *mls_key_package,
    size_t mls_key_package_length);
int32_t links_desktop_core_initialize_direct(LinksDesktopCore *core,
                                             const uint8_t *conversation_id,
                                             size_t conversation_id_length,
                                             const uint8_t *recipient_user_id,
                                             size_t recipient_user_id_length);
int32_t links_desktop_core_reset_direct(LinksDesktopCore *core,
                                        const uint8_t *conversation_id,
                                        size_t conversation_id_length,
                                        const uint8_t *recipient_user_id,
                                        size_t recipient_user_id_length);
int32_t links_desktop_core_send_text(LinksDesktopCore *core, const uint8_t *conversation_id,
                                     size_t conversation_id_length, const uint8_t *recipient_user_id,
                                     size_t recipient_user_id_length, const uint8_t *text,
                                     size_t text_length);

#endif
