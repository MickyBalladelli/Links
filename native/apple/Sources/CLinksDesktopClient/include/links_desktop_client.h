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
                                             size_t conversation_length, const uint8_t *sender_user,
                                             size_t sender_user_length, const uint8_t *sender,
                                             size_t sender_length, const uint8_t *text,
                                             size_t text_length, uint64_t sequence_id,
                                             uint64_t sent_at_ms);
/* kind: LINKS_DESKTOP_GROUP_*; payload is the UTF-8 name for RENAMED. */
typedef int32_t (*LinksDesktopGroupCallback)(void *context, const uint8_t *conversation,
                                              size_t conversation_length, uint32_t kind,
                                              const uint8_t *sender_user,
                                              size_t sender_user_length, const uint8_t *payload,
                                              size_t payload_length);
typedef int32_t (*LinksDesktopImageCallback)(void *context, const uint8_t *conversation,
                                              size_t conversation_length,
                                              const uint8_t *sender_user,
                                              size_t sender_user_length,
                                              const uint8_t *sender_device,
                                              size_t sender_device_length,
                                              const uint8_t *metadata,
                                              size_t metadata_length, uint64_t sequence_id,
                                              uint64_t sent_at_ms);

enum {
    LINKS_DESKTOP_GROUP_JOINED = 1,
    LINKS_DESKTOP_GROUP_RENAMED = 2,
    LINKS_DESKTOP_GROUP_MEMBERS_CHANGED = 3,
    LINKS_DESKTOP_GROUP_REMOVED = 4
};

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
    LinksDesktopGroupCallback on_group;
    LinksDesktopImageCallback on_image;
    LinksDesktopImageCallback on_file;
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
int32_t links_desktop_core_has_recipient(const LinksDesktopCore *core,
                                         const uint8_t *user_id, size_t user_id_length,
                                         uint8_t *output);
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
int32_t links_desktop_core_encode_image_blur_hash(const uint8_t *rgb_pixels,
                                                  size_t rgb_pixels_length, uint32_t width,
                                                  uint32_t height, uint8_t *output,
                                                  size_t capacity, size_t *length);
int32_t links_desktop_core_encrypt_image(const uint8_t *image, size_t image_length,
                                         const uint8_t *attachment_id,
                                         size_t attachment_id_length,
                                         const uint8_t *mime_type, size_t mime_type_length,
                                         uint32_t width, uint32_t height,
                                         const uint8_t *blur_hash, size_t blur_hash_length,
                                         uint8_t *metadata_output, size_t metadata_capacity,
                                         size_t *metadata_length, uint8_t *ciphertext_output,
                                         size_t ciphertext_capacity, size_t *ciphertext_length);
int32_t links_desktop_core_decrypt_image(const uint8_t *metadata, size_t metadata_length,
                                         const uint8_t *ciphertext, size_t ciphertext_length,
                                         uint8_t *plaintext_output, size_t plaintext_capacity,
                                         size_t *plaintext_length);
int32_t links_desktop_core_send_image(LinksDesktopCore *core,
                                      const uint8_t *conversation_id,
                                      size_t conversation_id_length,
                                      const uint8_t *recipient_user_id,
                                      size_t recipient_user_id_length,
                                      const uint8_t *metadata, size_t metadata_length);

int32_t links_desktop_core_create_group(LinksDesktopCore *core, const uint8_t *conversation_id,
                                        size_t conversation_id_length);
/* user_ids: newline-separated account IDs already registered as recipients. */
int32_t links_desktop_core_add_group_members(LinksDesktopCore *core,
                                             const uint8_t *conversation_id,
                                             size_t conversation_id_length,
                                             const uint8_t *user_ids, size_t user_ids_length);
int32_t links_desktop_core_remove_group_member(LinksDesktopCore *core,
                                               const uint8_t *conversation_id,
                                               size_t conversation_id_length,
                                               const uint8_t *user_id, size_t user_id_length);
int32_t links_desktop_core_disband_group(LinksDesktopCore *core, const uint8_t *conversation_id,
                                         size_t conversation_id_length);
int32_t links_desktop_core_leave_group(LinksDesktopCore *core, const uint8_t *conversation_id,
                                       size_t conversation_id_length);
int32_t links_desktop_core_send_group_text(LinksDesktopCore *core,
                                           const uint8_t *conversation_id,
                                           size_t conversation_id_length, const uint8_t *text,
                                           size_t text_length);
int32_t links_desktop_core_send_group_image(LinksDesktopCore *core,
                                            const uint8_t *conversation_id,
                                            size_t conversation_id_length,
                                            const uint8_t *metadata,
                                            size_t metadata_length);
int32_t links_desktop_core_encrypt_file(const uint8_t *file, size_t file_length,
                                        const uint8_t *attachment_id, size_t attachment_id_length,
                                        const uint8_t *mime_type, size_t mime_type_length,
                                        const uint8_t *file_name, size_t file_name_length,
                                        uint8_t *metadata_output, size_t metadata_capacity,
                                        size_t *metadata_length, uint8_t *ciphertext_output,
                                        size_t ciphertext_capacity, size_t *ciphertext_length);
int32_t links_desktop_core_decrypt_file(const uint8_t *metadata, size_t metadata_length,
                                        const uint8_t *ciphertext, size_t ciphertext_length,
                                        uint8_t *plaintext_output, size_t plaintext_capacity,
                                        size_t *plaintext_length);
int32_t links_desktop_core_send_file(LinksDesktopCore *core, const uint8_t *conversation_id,
                                     size_t conversation_id_length,
                                     const uint8_t *recipient_user_id,
                                     size_t recipient_user_id_length, const uint8_t *metadata,
                                     size_t metadata_length);
int32_t links_desktop_core_send_group_file(LinksDesktopCore *core,
                                           const uint8_t *conversation_id,
                                           size_t conversation_id_length,
                                           const uint8_t *metadata, size_t metadata_length);
int32_t links_desktop_core_set_group_name(LinksDesktopCore *core,
                                          const uint8_t *conversation_id,
                                          size_t conversation_id_length, const uint8_t *name,
                                          size_t name_length);
/* Newline-separated account IDs. */
int32_t links_desktop_core_group_members(const LinksDesktopCore *core,
                                         const uint8_t *conversation_id,
                                         size_t conversation_id_length, uint8_t *output,
                                         size_t capacity, size_t *length);
int32_t links_desktop_core_group_missing_recipients(const LinksDesktopCore *core,
                                                    const uint8_t *conversation_id,
                                                    size_t conversation_id_length,
                                                    uint8_t *output, size_t capacity,
                                                    size_t *length);

#endif
