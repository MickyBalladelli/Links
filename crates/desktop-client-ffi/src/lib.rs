//! Small, panic-safe C ABI for the native desktop client.
//!
//! The ABI deliberately exposes opaque handles and callback boundaries only.
//! Swift owns the profile vault, encrypted state file, bearer token, socket,
//! and UI. Rust owns MLS, sealed-sender envelopes, cursor ordering, and the
//! exact encrypted outbox bytes.
#![deny(unsafe_op_in_unsafe_fn)]

use links_client_core::{
    crypto::{RecipientKeyDirectory, SealedSenderCrypto, SealedSenderKeyResolver},
    envelopes::{ClientCore, FanoutRecipient},
    identity::LocalIdentity,
    mls::{mls_handshake_kind, MlsCredentialVerifier, MlsEngine, MlsHandshake, OpenMlsEngine,
          RustCryptoProvider},
    prekeys::{generate_profile, generate_upload, LocalPreKeyProfile, PreKeySecretStore,
              PreKeySigner, SecretKind},
    protocol::{self, v1},
    send::RecipientDevice,
    sequences::ConversationSequence,
    CoreError,
};
use openmls::prelude::GroupId;
use openmls_rust_crypto::MemoryStorage;
use prost::Message;
use std::{collections::{HashMap, HashSet}, ffi::c_void, panic::AssertUnwindSafe, slice, sync::{atomic::{AtomicBool, Ordering}, Arc, RwLock}};
use uuid::Uuid;
use zeroize::Zeroizing;

pub const LINKS_DESKTOP_OK: i32 = 0;
pub const LINKS_DESKTOP_INVALID: i32 = 1;
pub const LINKS_DESKTOP_UNAVAILABLE: i32 = 2;
pub const LINKS_DESKTOP_AUTHENTICATION: i32 = 3;
pub const LINKS_DESKTOP_PROVIDER: i32 = 4;
pub const LINKS_DESKTOP_STALE_CURSOR: i32 = 5;
const ABI_VERSION: u32 = 2;
const MAX_OUTPUT_BYTES: usize = protocol::MAX_FRAME_BYTES;
/// Persisted core state holds MLS storage, recipients and both outboxes, so it
/// is not bounded by one frame. Matches the host encrypted state store limit.
const MAX_STATE_BYTES: usize = 64 * 1024 * 1024;
/// Users added by one invite; the MLS group itself is capped separately.
const MAX_GROUP_USER_BATCH: usize = 50;

pub type SignCallback = unsafe extern "C" fn(
    *mut c_void,
    *const u8,
    usize,
    *mut u8,
) -> i32;
pub type StoreSecretCallback = unsafe extern "C" fn(
    *mut c_void,
    *const u8,
    usize,
    *const u8,
    usize,
) -> i32;
pub type LoadSecretCallback = unsafe extern "C" fn(
    *mut c_void,
    *const u8,
    usize,
    *mut u8,
    usize,
    *mut usize,
) -> i32;
pub type DeleteSecretCallback = unsafe extern "C" fn(*mut c_void, *const u8, usize) -> i32;
pub type StateCallback = unsafe extern "C" fn(
    *mut c_void,
    *mut u8,
    usize,
    *mut usize,
) -> i32;
pub type SaveStateCallback = unsafe extern "C" fn(*mut c_void, *const u8, usize) -> i32;
pub type SendFrameCallback = unsafe extern "C" fn(*mut c_void, *const u8, usize) -> i32;
pub type TextCallback = unsafe extern "C" fn(
    *mut c_void,
    *const u8,
    usize,
    *const u8,
    usize,
    *const u8,
    usize,
    *const u8,
    usize,
    u64,
    u64,
) -> i32;
/// (context, conversation, kind, sender user, payload). Kinds are the
/// `GROUP_EVENT_*` constants; payload is the UTF-8 group name for renames.
pub type GroupCallback = unsafe extern "C" fn(
    *mut c_void,
    *const u8,
    usize,
    u32,
    *const u8,
    usize,
    *const u8,
    usize,
) -> i32;

pub const GROUP_EVENT_JOINED: u32 = 1;
pub const GROUP_EVENT_RENAMED: u32 = 2;
pub const GROUP_EVENT_MEMBERS_CHANGED: u32 = 3;
pub const GROUP_EVENT_REMOVED: u32 = 4;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct LinksDesktopCoreCallbacks {
    pub abi_version: u32,
    pub context: *mut c_void,
    pub sign: Option<SignCallback>,
    pub store_secret: Option<StoreSecretCallback>,
    pub load_secret: Option<LoadSecretCallback>,
    pub delete_secret: Option<DeleteSecretCallback>,
    pub load_state: Option<StateCallback>,
    pub save_state: Option<SaveStateCallback>,
    pub send_frame: Option<SendFrameCallback>,
    pub on_text: Option<TextCallback>,
    pub identity_public_key: [u8; 32],
    pub on_group: Option<GroupCallback>,
}

#[derive(Clone, PartialEq, Message)]
struct PersistedState {
    #[prost(uint64, tag = "1")]
    cursor: u64,
    #[prost(message, repeated, tag = "2")]
    storage: Vec<StorageEntry>,
    #[prost(message, repeated, tag = "3")]
    sequences: Vec<SequenceEntry>,
    #[prost(bytes, optional, tag = "4")]
    prekey_profile: Option<Vec<u8>>,
    #[prost(bytes, repeated, tag = "5")]
    outbox: Vec<Vec<u8>>,
    #[prost(message, repeated, tag = "6")]
    bindings: Vec<PersistedBinding>,
    #[prost(bytes, repeated, tag = "7")]
    bootstrap_outbox: Vec<Vec<u8>>,
    #[prost(bool, tag = "8")]
    discard_next_batch: bool,
    // Tag 10 held KeyPackages without the LastResort capability, which
    // senders reject. Those are ignored so a valid one is published.
    #[prost(bytes, optional, tag = "11")]
    published_key_package: Option<Vec<u8>>,
    #[prost(message, repeated, tag = "9")]
    recipients: Vec<PersistedRecipient>,
    #[prost(string, repeated, tag = "12")]
    groups: Vec<String>,
}

#[derive(Clone, PartialEq, Message)]
struct PersistedBinding {
    #[prost(bytes, tag = "1")]
    user_id: Vec<u8>,
    #[prost(bytes, tag = "2")]
    device_id: Vec<u8>,
    #[prost(bytes, tag = "3")]
    mls_node_id: Vec<u8>,
    #[prost(bytes, tag = "4")]
    public_key: Vec<u8>,
}

#[derive(Clone, PartialEq, Message)]
struct PersistedRecipient {
    #[prost(string, tag = "1")]
    user_id: String,
    #[prost(string, tag = "2")]
    device_id: String,
    #[prost(bytes, tag = "3")]
    identity_public_key: Vec<u8>,
    #[prost(bytes, tag = "4")]
    prekey_bundle: Vec<u8>,
    #[prost(bytes, tag = "5")]
    mls_credential: Vec<u8>,
    #[prost(bytes, tag = "6")]
    mls_key_package: Vec<u8>,
}

#[derive(Clone, PartialEq, Message)]
struct StorageEntry {
    #[prost(bytes, tag = "1")]
    key: Vec<u8>,
    #[prost(bytes, tag = "2")]
    value: Vec<u8>,
}

#[derive(Clone, PartialEq, Message)]
struct SequenceEntry {
    #[prost(string, tag = "1")]
    conversation_id: String,
    #[prost(uint64, tag = "2")]
    last_sequence_id: u64,
}

#[derive(Clone)]
struct CallbackSigner {
    callbacks: LinksDesktopCoreCallbacks,
}

impl CallbackSigner {
    fn public_key_impl(&self) -> Result<[u8; 32], CoreError> {
        if self.callbacks.identity_public_key.iter().all(|byte| *byte == 0) {
            return Err(CoreError::Authentication);
        }
        Ok(self.callbacks.identity_public_key)
    }

    fn sign_impl(&self, transcript: &[u8]) -> Result<[u8; 64], CoreError> {
        let callback = self.callbacks.sign.ok_or(CoreError::Provider)?;
        if transcript.len() > protocol::MAX_MESSAGE_BYTES {
            return Err(CoreError::Authentication);
        }
        let mut signature = [0u8; 64];
        let status = unsafe {
            callback(
                self.callbacks.context,
                transcript.as_ptr(),
                transcript.len(),
                signature.as_mut_ptr(),
            )
        };
        callback_status(status)?;
        Ok(signature)
    }
}

impl PreKeySigner for CallbackSigner {
    fn public_key(&self) -> Result<[u8; 32], CoreError> {
        self.public_key_impl()
    }

    fn sign(&self, transcript: &[u8]) -> Result<[u8; 64], CoreError> {
        self.sign_impl(transcript)
    }
}

#[derive(Clone)]
struct CallbackSecrets {
    callbacks: LinksDesktopCoreCallbacks,
}

impl CallbackSecrets {
    fn key(kind: SecretKind, id: u64) -> String {
        let name = match kind {
            SecretKind::IdentityDh => "identity-dh",
            SecretKind::SignedCurve => "signed-curve",
            SecretKind::OneTimeCurve => "one-time-curve",
            SecretKind::LastResortKem => "last-resort-kem",
            SecretKind::OneTimeKem => "one-time-kem",
        };
        format!("prekey/{name}/{id}")
    }

    fn store(&self, kind: SecretKind, id: u64, secret: &[u8]) -> Result<(), CoreError> {
        let callback = self.callbacks.store_secret.ok_or(CoreError::Provider)?;
        let key = Self::key(kind, id);
        let status = unsafe {
            callback(
                self.callbacks.context,
                key.as_ptr(),
                key.len(),
                secret.as_ptr(),
                secret.len(),
            )
        };
        callback_status(status)
    }

    fn load(&self, kind: SecretKind, id: u64, expected: usize) -> Result<Zeroizing<Vec<u8>>, CoreError> {
        let callback = self.callbacks.load_secret.ok_or(CoreError::Provider)?;
        let key = Self::key(kind, id);
        let mut value = Zeroizing::new(vec![0u8; 64]);
        let mut length = 0usize;
        let status = unsafe {
            callback(
                self.callbacks.context,
                key.as_ptr(),
                key.len(),
                value.as_mut_ptr(),
                value.len(),
                &mut length,
            )
        };
        callback_status(status)?;
        if length != expected {
            return Err(CoreError::Authentication);
        }
        value.truncate(length);
        Ok(value)
    }

    fn delete(&self, kind: SecretKind, id: u64) -> Result<(), CoreError> {
        let callback = self.callbacks.delete_secret.ok_or(CoreError::Provider)?;
        let key = Self::key(kind, id);
        let status = unsafe { callback(self.callbacks.context, key.as_ptr(), key.len()) };
        callback_status(status)
    }
}

impl PreKeySecretStore for CallbackSecrets {
    fn store_x25519(&mut self, kind: SecretKind, id: u64, seed: &[u8; 32]) -> Result<(), CoreError> {
        self.store(kind, id, seed)
    }

    fn store_ml_kem_768(&mut self, kind: SecretKind, id: u64, seed: &[u8; 64]) -> Result<(), CoreError> {
        self.store(kind, id, seed)
    }

    fn delete(&mut self, kind: SecretKind, id: u64) -> Result<(), CoreError> {
        CallbackSecrets::delete(self, kind, id)
    }
}

struct CallbackResolver {
    callbacks: LinksDesktopCoreCallbacks,
    local_device_id: String,
    identity_revision: u64,
    recipient_keys: HashMap<String, [u8; 32]>,
}

impl SealedSenderKeyResolver for CallbackResolver {
    fn recipient_public_key(&self, recipient_device_id: &str) -> Result<[u8; 32], CoreError> {
        self.recipient_keys
            .get(recipient_device_id)
            .copied()
            .ok_or(CoreError::Authentication)
    }

    fn local_private_key(&self, recipient_device_id: &str) -> Result<Zeroizing<[u8; 32]>, CoreError> {
        if recipient_device_id != self.local_device_id || self.identity_revision == 0 {
            return Err(CoreError::Authentication);
        }
        let secrets = CallbackSecrets { callbacks: self.callbacks };
        let bytes = secrets.load(SecretKind::IdentityDh, self.identity_revision, 32)?;
        let seed: [u8; 32] = bytes.as_slice().try_into().map_err(|_| CoreError::Authentication)?;
        Ok(Zeroizing::new(seed))
    }
}

impl RecipientKeyDirectory for CallbackResolver {
    fn install_recipient_public_key(&mut self, recipient_device_id: &str, public_key: [u8; 32]) -> Result<(), CoreError> {
        protocol::validate_id(recipient_device_id)?;
        if public_key.iter().all(|byte| *byte == 0) {
            return Err(CoreError::Authentication);
        }
        self.recipient_keys.insert(recipient_device_id.to_owned(), public_key);
        Ok(())
    }
}

#[derive(Clone)]
struct CallbackVerifier {
    bindings: Arc<RwLock<HashMap<Uuid, links_identity::DeviceBinding>>>,
    /// Set only while applying a group Welcome or Commit. A joining device
    /// cannot know every member in advance, so it accepts first-seen
    /// credentials from that authenticated handshake. A credential that
    /// contradicts a known binding is still rejected, and the host checks
    /// each member against the directory before sending to them.
    trust_new: Arc<AtomicBool>,
}

impl MlsCredentialVerifier for CallbackVerifier {
    fn verify(&self, binding: &links_identity::DeviceBinding) -> Result<(), CoreError> {
        let mut bindings = self.bindings.write().map_err(|_| CoreError::Provider)?;
        let Some(expected) = bindings.get(&binding.device_id) else {
            if self.trust_new.load(Ordering::SeqCst) {
                bindings.insert(binding.device_id, binding.clone());
                return Ok(());
            }
            return Err(CoreError::Authentication);
        };
        if expected != binding {
            return Err(CoreError::Authentication);
        }
        Ok(())
    }
}

/// Clears the verifier's first-seen trust even when the handshake fails.
struct TrustNewGuard(Arc<AtomicBool>);

impl TrustNewGuard {
    fn new(flag: &Arc<AtomicBool>) -> Self {
        flag.store(true, Ordering::SeqCst);
        Self(Arc::clone(flag))
    }
}

impl Drop for TrustNewGuard {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

struct RecipientRecord {
    user_id: String,
    device_id: String,
    identity_public_key: [u8; 32],
    prekey_bundle: v1::PreKeyBundle,
    mls_credential: Vec<u8>,
    mls_key_package: Vec<u8>,
}

type Mls = OpenMlsEngine<RustCryptoProvider<MemoryStorage>, CallbackSigner, CallbackVerifier>;
type Client = ClientCore<SealedSenderCrypto<CallbackResolver>, Mls>;

pub struct LinksDesktopCore {
    callbacks: LinksDesktopCoreCallbacks,
    client: Client,
    mls_credential: Vec<u8>,
    bindings: Arc<RwLock<HashMap<Uuid, links_identity::DeviceBinding>>>,
    secrets: CallbackSecrets,
    profile: Option<LocalPreKeyProfile>,
    cursor: u64,
    sequences: HashMap<String, u64>,
    outbox: Vec<Vec<u8>>,
    bootstrap_outbox: Vec<Vec<u8>>,
    recipients: HashMap<String, Vec<RecipientRecord>>,
    pending_batch: Option<v1::SyncBatch>,
    discard_next_batch: bool,
    published_key_package: Option<Vec<u8>>,
    /// Many-to-many conversations. Only these accept mailbox commits, so a
    /// peer cannot grow a direct chat into a group.
    groups: HashSet<String>,
    trust_new: Arc<AtomicBool>,
}

enum MailboxItem {
    Message { message: v1::Message, sender_user_id: String },
    Event(GroupEvent),
    Ignored,
}

enum GroupEvent {
    Joined(String),
    Renamed { conversation_id: String, sender_user_id: String, name: String },
    MembersChanged(String),
    Removed(String),
}

unsafe fn input<'a>(pointer: *const u8, length: usize, maximum: usize) -> Result<&'a [u8], i32> {
    if length > maximum || (pointer.is_null() && length != 0) {
        return Err(LINKS_DESKTOP_INVALID);
    }
    if length == 0 {
        return Ok(&[]);
    }
    Ok(unsafe { slice::from_raw_parts(pointer, length) })
}

fn callback_status(status: i32) -> Result<(), CoreError> {
    match status {
        LINKS_DESKTOP_OK => Ok(()),
        LINKS_DESKTOP_UNAVAILABLE => Err(CoreError::CryptoUnavailable),
        LINKS_DESKTOP_AUTHENTICATION => Err(CoreError::Authentication),
        LINKS_DESKTOP_STALE_CURSOR => Err(CoreError::InvalidSync),
        _ => Err(CoreError::Provider),
    }
}

fn status(error: CoreError) -> i32 {
    match error {
        CoreError::Authentication => LINKS_DESKTOP_AUTHENTICATION,
        CoreError::CryptoUnavailable => LINKS_DESKTOP_UNAVAILABLE,
        CoreError::InvalidSync => LINKS_DESKTOP_STALE_CURSOR,
        _ => LINKS_DESKTOP_PROVIDER,
    }
}

// Authentication from a received payload is local crypto/data validation.
// Only a server unauthenticated error should make Swift clear the bearer session.
fn local_frame_error(error: CoreError) -> CoreError {
    match error {
        CoreError::Authentication => CoreError::Provider,
        error => error,
    }
}

fn boundary(work: impl FnOnce() -> i32) -> i32 {
    match std::panic::catch_unwind(AssertUnwindSafe(work)) {
        Ok(code) => code,
        Err(_) => LINKS_DESKTOP_PROVIDER,
    }
}

unsafe fn write_output(bytes: &[u8], pointer: *mut u8, capacity: usize, length: *mut usize) -> i32 {
    if length.is_null() || bytes.len() > MAX_OUTPUT_BYTES {
        return LINKS_DESKTOP_INVALID;
    }
    unsafe { length.write(bytes.len()) };
    if pointer.is_null() || capacity < bytes.len() {
        return LINKS_DESKTOP_INVALID;
    }
    unsafe { pointer.copy_from_nonoverlapping(bytes.as_ptr(), bytes.len()) };
    LINKS_DESKTOP_OK
}

fn parse_binding(credential: &[u8]) -> Result<links_identity::DeviceBinding, CoreError> {
    if credential.len() < 4 {
        return Err(CoreError::Authentication);
    }
    // TLS variable-length vectors reserve the top two bits of a two-byte
    // length prefix for the encoded-width marker. A 96-byte MLS identity is
    // therefore encoded as 0x4060, not the literal integer 0x4060.
    if credential[2] & 0xc0 != 0x40 {
        return Err(CoreError::Authentication);
    }
    let length = usize::from(u16::from_be_bytes([credential[2], credential[3]]) & 0x3fff);
    if credential.len() != length + 4 {
        return Err(CoreError::Authentication);
    }
    links_identity::parse_mls_basic_identity(&credential[4..])
        .map_err(|_| CoreError::Authentication)
}

fn load_state(callbacks: LinksDesktopCoreCallbacks) -> Result<Option<PersistedState>, CoreError> {
    let Some(callback) = callbacks.load_state else { return Ok(None) };
    let mut length = 0usize;
    callback_status(unsafe { callback(callbacks.context, std::ptr::null_mut(), 0, &mut length) })?;
    if length == 0 {
        return Ok(None);
    }
    if length > MAX_STATE_BYTES {
        return Err(CoreError::Provider);
    }
    let mut bytes = vec![0u8; length];
    callback_status(unsafe { callback(callbacks.context, bytes.as_mut_ptr(), bytes.len(), &mut length) })?;
    bytes.truncate(length);
    PersistedState::decode(bytes.as_slice()).map(Some).map_err(|_| CoreError::Provider)
}

impl LinksDesktopCore {
    fn snapshot(&self) -> Result<Vec<u8>, CoreError> {
        let storage = self.client.mls().provider().storage();
        let values = storage.values.read().map_err(|_| CoreError::Provider)?;
        let bindings = self.bindings.read().map_err(|_| CoreError::Provider)?;
        let state = PersistedState {
            cursor: self.cursor,
            storage: values.iter().map(|(key, value)| StorageEntry {
                key: key.clone(),
                value: value.clone(),
            }).collect(),
            sequences: self.sequences.iter().map(|(conversation_id, last_sequence_id)| SequenceEntry {
                conversation_id: conversation_id.clone(),
                last_sequence_id: *last_sequence_id,
            }).collect(),
            prekey_profile: self.profile.as_ref().map(|profile| profile.profile.encode_to_vec()),
            outbox: self.outbox.clone(),
            bootstrap_outbox: self.bootstrap_outbox.clone(),
            discard_next_batch: self.discard_next_batch,
            published_key_package: self.published_key_package.clone(),
            groups: self.groups.iter().cloned().collect(),
            bindings: bindings.values().map(|binding| PersistedBinding {
                user_id: binding.user_id.as_bytes().to_vec(),
                device_id: binding.device_id.as_bytes().to_vec(),
                mls_node_id: binding.mls_node_id.as_bytes().to_vec(),
                public_key: binding.public_key.to_vec(),
            }).collect(),
            recipients: self.recipients.values().flat_map(|records| records.iter().map(|record| {
                PersistedRecipient {
                    user_id: record.user_id.clone(),
                    device_id: record.device_id.clone(),
                    identity_public_key: record.identity_public_key.to_vec(),
                    prekey_bundle: record.prekey_bundle.encode_to_vec(),
                    mls_credential: record.mls_credential.clone(),
                    mls_key_package: record.mls_key_package.clone(),
                }
            })).collect(),
        };
        Ok(state.encode_to_vec())
    }

    fn save(&self) -> Result<(), CoreError> {
        let callback = self.callbacks.save_state.ok_or(CoreError::Provider)?;
        let bytes = self.snapshot()?;
        callback_status(unsafe { callback(self.callbacks.context, bytes.as_ptr(), bytes.len()) })
    }

    fn restore_state(&mut self, state: PersistedState) -> Result<(), CoreError> {
        if state.cursor > protocol::MAX_CURSOR
            || state.storage.len() > 100_000
            || state.outbox.len() > 100
            || state.bootstrap_outbox.len() > 100
            || state.recipients.len() > 10_000
            || state.outbox.iter().chain(state.bootstrap_outbox.iter()).any(|frame| {
                frame.is_empty() || frame.len() > MAX_OUTPUT_BYTES
            })
        {
            return Err(CoreError::InvalidSync);
        }
        let storage = self.client.mls_mut().provider_mut().storage();
        let mut values = storage.values.write().map_err(|_| CoreError::Provider)?;
        values.clear();
        for entry in state.storage {
            if entry.key.len() > MAX_OUTPUT_BYTES || entry.value.len() > MAX_OUTPUT_BYTES {
                return Err(CoreError::Provider);
            }
            values.insert(entry.key, entry.value);
        }
        drop(values);
        self.cursor = state.cursor;
        self.sequences.clear();
        for sequence in state.sequences {
            let restored = ConversationSequence::restore(
                sequence.conversation_id.clone(),
                self.client.device_id().to_owned(),
                sequence.last_sequence_id,
            )?;
            self.sequences.insert(restored.conversation_id().to_owned(), restored.last_sequence_id());
        }
        self.outbox = state.outbox;
        self.profile = state.prekey_profile.map(|bytes| {
            v1::PreKeyProfile::decode(bytes.as_slice())
                .map_err(|_| CoreError::Provider)
                .and_then(|profile| {
                    Ok(LocalPreKeyProfile {
                        device_id: self.client.device_id().to_owned(),
                        revision: 1,
                        profile,
                    })
                })
        }).transpose()?;
        if let Some(profile) = &self.profile {
            self.client.crypto_mut().resolver_mut().identity_revision = profile.revision;
        }
        for persisted in state.bindings {
            if persisted.user_id.len() != 16
                || persisted.device_id.len() != 16
                || persisted.mls_node_id.len() != 16
                || persisted.public_key.len() != 32
            {
                return Err(CoreError::Authentication);
            }
            let binding = links_identity::DeviceBinding {
                user_id: Uuid::from_slice(&persisted.user_id).map_err(|_| CoreError::Authentication)?,
                device_id: Uuid::from_slice(&persisted.device_id).map_err(|_| CoreError::Authentication)?,
                mls_node_id: Uuid::from_slice(&persisted.mls_node_id).map_err(|_| CoreError::Authentication)?,
                public_key: persisted.public_key.as_slice().try_into().map_err(|_| CoreError::Authentication)?,
            };
            links_identity::validate_public_key(&binding.public_key)
                .map_err(|_| CoreError::Authentication)?;
            self.insert_binding(binding)?;
        }
        self.recipients.clear();
        for persisted in state.recipients {
            if protocol::validate_id(&persisted.user_id).is_err()
                || protocol::validate_id(&persisted.device_id).is_err()
                || persisted.identity_public_key.len() != 32
                || persisted.prekey_bundle.len() > protocol::MAX_PREKEY_UPLOAD_BYTES
                || persisted.mls_credential.len() > 1024
                || persisted.mls_key_package.len() > protocol::MAX_FRAME_BYTES
            {
                return Err(CoreError::Authentication);
            }
            let identity_public_key: [u8; 32] = persisted.identity_public_key
                .as_slice()
                .try_into()
                .map_err(|_| CoreError::Authentication)?;
            let prekey_bundle = v1::PreKeyBundle::decode(persisted.prekey_bundle.as_slice())
                .map_err(|_| CoreError::Authentication)?;
            self.set_recipient(RecipientRecord {
                user_id: persisted.user_id,
                device_id: persisted.device_id,
                identity_public_key,
                prekey_bundle,
                mls_credential: persisted.mls_credential,
                mls_key_package: persisted.mls_key_package,
            })?;
        }
        self.bootstrap_outbox = state.bootstrap_outbox;
        self.discard_next_batch = state.discard_next_batch;
        self.published_key_package = state
            .published_key_package
            .filter(|package| !package.is_empty() && package.len() <= MAX_OUTPUT_BYTES);
        self.groups = state
            .groups
            .into_iter()
            .filter(|group| protocol::validate_id(group).is_ok())
            .collect();
        self.recover_group_bindings()?;
        Ok(())
    }

    fn insert_binding(&self, binding: links_identity::DeviceBinding) -> Result<(), CoreError> {
        let mut bindings = self.bindings.write().map_err(|_| CoreError::Provider)?;
        if let Some(existing) = bindings.get(&binding.device_id) {
            if existing != &binding {
                return Err(CoreError::Authentication);
            }
        } else {
            bindings.insert(binding.device_id, binding);
        }
        Ok(())
    }

    fn recover_group_bindings(&self) -> Result<(), CoreError> {
        let storage = self.client.mls().provider().storage();
        let values = storage.values.read().map_err(|_| CoreError::Provider)?;
        let group_state_label = b"GroupState";
        let mut group_ids = HashSet::new();
        for key in values.keys() {
            if key.len() <= group_state_label.len() + 2
                || !key.starts_with(group_state_label)
                || key[key.len() - 2..] != [0, 1]
            {
                continue;
            }
            let serialized = &key[group_state_label.len()..key.len() - 2];
            let group_id = serde_json::from_slice::<GroupId>(serialized)
                .map_err(|_| CoreError::Provider)?;
            if group_id.as_slice().len() == 16 {
                group_ids.insert(Uuid::from_slice(group_id.as_slice()).map_err(|_| CoreError::Provider)?.to_string());
            }
        }
        drop(values);
        for conversation_id in group_ids {
            for binding in self.client.mls().group_member_bindings(&conversation_id)? {
                self.insert_binding(binding)?;
            }
        }
        Ok(())
    }

fn send_frame(&self, frame: &[u8]) -> Result<(), CoreError> {
        let callback = self.callbacks.send_frame.ok_or(CoreError::Provider)?;
        callback_status(unsafe { callback(self.callbacks.context, frame.as_ptr(), frame.len()) })
    }

    fn encode_client_frame(&self, body: v1::client_frame::Body) -> Result<Vec<u8>, CoreError> {
        let frame = v1::ClientFrame {
            request_id: Uuid::new_v4().to_string(),
            body: Some(body),
        };
        protocol::validate_id(&frame.request_id)?;
        let bytes = frame.encode_to_vec();
        if bytes.len() > protocol::MAX_FRAME_BYTES {
            return Err(CoreError::Protocol(protocol::ProtocolError::TooLarge));
        }
        Ok(bytes)
    }

    /// Publish one last-resort KeyPackage per core state. Generating a new one
    /// on every connection grew the persisted MLS storage without bound.
    fn key_package(&mut self) -> Result<Vec<u8>, CoreError> {
        // Republish when the saved package would be rejected by peers, so a
        // bad package from an older build cannot block new conversations.
        if let Some(package) = &self.published_key_package {
            if self.client.mls().accepts_key_package(package) {
                return Ok(package.clone());
            }
        }
        let package = self.client.mls_mut().generate_key_package()?;
        self.published_key_package = Some(package.clone());
        self.save()?;
        Ok(package)
    }

    fn generate_prekey_upload(&mut self, curve_count: u32, kem_count: u32) -> Result<Vec<u8>, CoreError> {
        if curve_count as usize > protocol::MAX_ONE_TIME_PREKEYS || kem_count as usize > protocol::MAX_ONE_TIME_PREKEYS {
            return Err(CoreError::Authentication);
        }
        if self.profile.is_none() {
            let profile = generate_profile(
                self.client.device_id().to_owned(),
                1,
                &CallbackSigner { callbacks: self.callbacks },
                &mut self.secrets,
            )?;
            self.profile = Some(profile);
            self.client.crypto_mut().resolver_mut().identity_revision = 1;
        }
        let profile = self.profile.as_ref().ok_or(CoreError::Provider)?;
        let upload = generate_upload(
            profile,
            curve_count,
            kem_count,
            &CallbackSigner { callbacks: self.callbacks },
            &mut self.secrets,
        )?;
        self.save()?;
        Ok(upload.encode_to_vec())
    }

    fn set_recipient(&mut self, record: RecipientRecord) -> Result<(), CoreError> {
        let binding = parse_binding_from_prekey_credential(&record)?;
        let recipient = RecipientDevice::new(
            record.user_id.clone(),
            record.device_id.clone(),
            record.identity_public_key,
            record.prekey_bundle.clone(),
            record.mls_key_package.clone(),
        )?;
        let sealed_key = recipient.verify()?;
        self.insert_binding(binding)?;
        self.client.crypto_mut().resolver_mut().install_recipient_public_key(
            &record.device_id,
            sealed_key,
        )?;
        self.recipients.entry(record.user_id.clone()).or_default().retain(|item| item.device_id != record.device_id);
        self.recipients.entry(record.user_id.clone()).or_default().push(record);
        Ok(())
    }

    fn initialize_direct_group(
        &mut self,
        conversation_id: &str,
        recipient_user_id: &str,
        reset_group: bool,
    ) -> Result<(), CoreError> {
        protocol::validate_id(conversation_id)?;
        protocol::validate_id(recipient_user_id)?;
        if recipient_user_id == self.client.user_id() {
            return Err(CoreError::Authentication);
        }
        let records = self
            .recipients
            .get(recipient_user_id)
            .ok_or(CoreError::Authentication)?;
        if records.is_empty() || records.len() > protocol::MAX_FANOUT_DEVICES {
            return Err(CoreError::Authentication);
        }
        let packages = records
            .iter()
            .map(|record| record.mls_key_package.as_slice())
            .collect::<Vec<_>>();
        let pending = if reset_group {
            Some(self.client.mls_mut().reset_direct_group(conversation_id, &packages)?)
        } else {
            self.client.mls_mut().ensure_direct_group(conversation_id, &packages)?
        };
        let Some(pending) = pending else { return Ok(()) };
        let welcome = pending.welcome.as_ref().ok_or(CoreError::Provider)?;
        let mut bootstraps = Vec::with_capacity(records.len());
        for record in records {
            let bootstrap = self.encode_client_frame(v1::client_frame::Body::MlsBootstrap(
                v1::MlsBootstrap {
                    conversation_id: conversation_id.to_owned(),
                    recipient_device_id: record.device_id.clone(),
                    commit: pending.commit.clone(),
                    welcome: welcome.clone(),
                    sender_mls_credential: self.mls_credential.clone(),
                    sender_identity_public_key: self.callbacks.identity_public_key.to_vec(),
                    reset_group,
                },
            ))?;
            bootstraps.push(bootstrap);
        }
        self.bootstrap_outbox.extend(bootstraps.iter().cloned());
        self.save()?;
        for bootstrap in bootstraps {
            self.send_frame(&bootstrap)?;
        }
        self.client.mls_mut().merge_pending_direct_commit(conversation_id)?;
        self.save()
    }

    fn reset_direct_group(
        &mut self,
        conversation_id: &str,
        recipient_user_id: &str,
    ) -> Result<(), CoreError> {
        self.initialize_direct_group(conversation_id, recipient_user_id, true)
    }

    fn send_text(&mut self, conversation_id: &str, recipient_user_id: &str, text: &str) -> Result<(), CoreError> {
        protocol::validate_id(conversation_id)?;
        protocol::validate_id(recipient_user_id)?;
        if recipient_user_id == self.client.user_id() || text.is_empty() || text.len() > protocol::MAX_MESSAGE_BYTES {
            return Err(CoreError::Authentication);
        }
        self.initialize_direct_group(conversation_id, recipient_user_id, false)?;
        let records = self.recipients.get(recipient_user_id).ok_or(CoreError::Authentication)?;
        if records.is_empty() || records.len() > protocol::MAX_FANOUT_DEVICES {
            return Err(CoreError::Authentication);
        }
        let recipients = records.iter().map(|record| {
            let recipient = RecipientDevice::new(
                record.user_id.clone(),
                record.device_id.clone(),
                record.identity_public_key,
                record.prekey_bundle.clone(),
                record.mls_key_package.clone(),
            )?;
            let sealed_key = recipient.verify()?;
            self.client.crypto_mut().install_recipient_public_key(&record.device_id, sealed_key)?;
            Ok((FanoutRecipient::new(record.device_id.clone())?, record.mls_key_package.as_slice()))
        }).collect::<Result<Vec<_>, CoreError>>()?;
        let now_ms = now_ms();
        let message_id = Uuid::new_v4().to_string();
        let mut sequence = ConversationSequence::restore(
            conversation_id.to_owned(),
            self.client.device_id().to_owned(),
            self.sequences.get(conversation_id).copied().unwrap_or(0),
        )?;
        let fanout = recipients.iter().map(|(recipient, _)| recipient.clone()).collect::<Vec<_>>();
        let message = v1::Message {
            message_id,
            conversation_id: conversation_id.to_owned(),
            sender_device_id: self.client.device_id().to_owned(),
            sent_at_ms: now_ms,
            sequence_id: 0,
            content: Some(v1::message::Content::Text(text.to_owned())),
        };
        let expires = now_ms.checked_add(protocol::MAX_RETENTION_MS).ok_or(CoreError::Provider)?;
        let (message, envelopes) = self.client.seal_next_message_for_devices(message, &mut sequence, &fanout, expires, now_ms)?;
        let mut frames = Vec::with_capacity(envelopes.len());
        for envelope in envelopes {
            protocol::validate_envelope(&envelope)?;
            frames.push(self.encode_client_frame(v1::client_frame::Body::Send(envelope))?);
        }
        self.sequences.insert(conversation_id.to_owned(), sequence.last_sequence_id());
        self.outbox.extend(frames.iter().cloned());
        self.save()?;
        for frame in frames {
            self.send_frame(&frame)?;
        }
        let _ = message;
        Ok(())
    }

    fn require_group(&self, conversation_id: &str) -> Result<(), CoreError> {
        protocol::validate_id(conversation_id)?;
        if self.groups.contains(conversation_id) {
            Ok(())
        } else {
            Err(CoreError::Authentication)
        }
    }

    /// Install sealed-sender keys and build one envelope identity per device.
    /// Every device needs a directory-verified recipient record first.
    fn fanout_for_devices(&mut self, device_ids: &[String]) -> Result<Vec<FanoutRecipient>, CoreError> {
        let mut fanout = Vec::with_capacity(device_ids.len());
        for device_id in device_ids {
            let record = self
                .recipients
                .values()
                .flatten()
                .find(|record| &record.device_id == device_id)
                .ok_or(CoreError::Authentication)?;
            let recipient = RecipientDevice::new(
                record.user_id.clone(),
                record.device_id.clone(),
                record.identity_public_key,
                record.prekey_bundle.clone(),
                record.mls_key_package.clone(),
            )?;
            let sealed_key = recipient.verify()?;
            self.client
                .crypto_mut()
                .install_recipient_public_key(device_id, sealed_key)?;
            fanout.push(FanoutRecipient::new(device_id.clone())?);
        }
        Ok(fanout)
    }

    /// Queue, persist and send already-sealed envelopes.
    fn send_envelopes(&mut self, envelopes: Vec<v1::Envelope>) -> Result<(), CoreError> {
        let mut frames = Vec::with_capacity(envelopes.len());
        for envelope in envelopes {
            protocol::validate_envelope(&envelope)?;
            frames.push(self.encode_client_frame(v1::client_frame::Body::Send(envelope))?);
        }
        self.outbox.extend(frames.iter().cloned());
        self.save()?;
        for frame in frames {
            self.send_frame(&frame)?;
        }
        Ok(())
    }

    fn other_member_devices(&self, conversation_id: &str) -> Result<Vec<String>, CoreError> {
        let own = self.client.device_id().to_owned();
        Ok(self
            .client
            .mls()
            .group_member_leaves(conversation_id)?
            .into_iter()
            .map(|(_, binding)| binding.device_id.to_string())
            .filter(|device| device != &own)
            .collect())
    }

    fn create_group(&mut self, conversation_id: &str) -> Result<(), CoreError> {
        protocol::validate_id(conversation_id)?;
        self.client.mls_mut().create_group_for(conversation_id)?;
        self.groups.insert(conversation_id.to_owned());
        self.save()
    }

    /// Add every recorded device of these users. Existing members receive the
    /// commit and new devices the Welcome, both through the ordered mailbox.
    fn add_group_members(&mut self, conversation_id: &str, user_ids: &[String]) -> Result<(), CoreError> {
        self.require_group(conversation_id)?;
        if user_ids.is_empty() || user_ids.len() > MAX_GROUP_USER_BATCH {
            return Err(CoreError::Authentication);
        }
        let existing = self.other_member_devices(conversation_id)?;
        let mut new_devices = Vec::new();
        let mut packages = Vec::new();
        for user_id in user_ids {
            protocol::validate_id(user_id)?;
            if user_id == self.client.user_id() {
                return Err(CoreError::Authentication);
            }
            let records = self.recipients.get(user_id).ok_or(CoreError::Authentication)?;
            for record in records {
                if !existing.contains(&record.device_id) && !new_devices.contains(&record.device_id) {
                    new_devices.push(record.device_id.clone());
                    packages.push(record.mls_key_package.clone());
                }
            }
        }
        if packages.is_empty() {
            return Ok(());
        }
        let package_refs = packages.iter().map(Vec::as_slice).collect::<Vec<_>>();
        let pending = self
            .client
            .mls_mut()
            .add_group_members(conversation_id, &package_refs)?;
        let welcome = pending.welcome.clone().ok_or(CoreError::Provider)?;
        let now = now_ms();
        let expires = now.checked_add(protocol::MAX_RETENTION_MS).ok_or(CoreError::Provider)?;
        let mut envelopes = Vec::new();
        if !existing.is_empty() {
            let fanout = self.fanout_for_devices(&existing)?;
            envelopes.extend(self.client.seal_handshake_for_devices(&pending.commit, &fanout, expires, now)?);
        }
        let fanout = self.fanout_for_devices(&new_devices)?;
        envelopes.extend(self.client.seal_handshake_for_devices(&welcome, &fanout, expires, now)?);
        self.client.mls_mut().merge_pending_commit(conversation_id)?;
        self.send_envelopes(envelopes)
    }

    /// Remove every device of one user. The removed devices also receive the
    /// commit so they learn they left the group.
    fn remove_group_member(&mut self, conversation_id: &str, user_id: &str) -> Result<(), CoreError> {
        self.require_group(conversation_id)?;
        protocol::validate_id(user_id)?;
        if user_id == self.client.user_id() {
            return Err(CoreError::Authentication);
        }
        let own = self.client.device_id().to_owned();
        let leaves = self.client.mls().group_member_leaves(conversation_id)?;
        let removed = leaves
            .iter()
            .filter(|(_, binding)| binding.user_id.to_string() == user_id)
            .map(|(leaf, _)| *leaf)
            .collect::<Vec<_>>();
        if removed.is_empty() {
            return Ok(());
        }
        let recipients = leaves
            .iter()
            .map(|(_, binding)| binding.device_id.to_string())
            .filter(|device| device != &own)
            .collect::<Vec<_>>();
        let pending = self.client.mls_mut().remove_members(conversation_id, &removed)?;
        let now = now_ms();
        let expires = now.checked_add(protocol::MAX_RETENTION_MS).ok_or(CoreError::Provider)?;
        let fanout = self.fanout_for_devices(&recipients)?;
        let envelopes = self.client.seal_handshake_for_devices(&pending.commit, &fanout, expires, now)?;
        self.client.mls_mut().merge_pending_commit(conversation_id)?;
        self.send_envelopes(envelopes)
    }

    fn leave_group(&mut self, conversation_id: &str) -> Result<(), CoreError> {
        protocol::validate_id(conversation_id)?;
        self.client.mls_mut().delete_group(conversation_id)?;
        self.groups.remove(conversation_id);
        self.save()
    }

    fn group_member_users(&self, conversation_id: &str) -> Result<Vec<String>, CoreError> {
        self.require_group(conversation_id)?;
        let mut users = Vec::new();
        for (_, binding) in self.client.mls().group_member_leaves(conversation_id)? {
            let user = binding.user_id.to_string();
            if !users.contains(&user) {
                users.push(user);
            }
        }
        Ok(users)
    }

    /// Members with at least one device this core cannot seal to yet. The
    /// host fetches those users from the directory before sending.
    fn group_users_missing_recipients(&self, conversation_id: &str) -> Result<Vec<String>, CoreError> {
        self.require_group(conversation_id)?;
        let own = self.client.device_id().to_owned();
        let mut missing = Vec::new();
        for (_, binding) in self.client.mls().group_member_leaves(conversation_id)? {
            let device = binding.device_id.to_string();
            let user = binding.user_id.to_string();
            let known = self
                .recipients
                .get(&user)
                .is_some_and(|records| records.iter().any(|record| record.device_id == device));
            if device != own && !known && !missing.contains(&user) {
                missing.push(user);
            }
        }
        Ok(missing)
    }

    fn send_group_content(&mut self, conversation_id: &str, content: v1::message::Content) -> Result<(), CoreError> {
        self.require_group(conversation_id)?;
        let devices = self.other_member_devices(conversation_id)?;
        if devices.is_empty() {
            return Err(CoreError::Authentication);
        }
        let fanout = self.fanout_for_devices(&devices)?;
        let now = now_ms();
        let mut sequence = ConversationSequence::restore(
            conversation_id.to_owned(),
            self.client.device_id().to_owned(),
            self.sequences.get(conversation_id).copied().unwrap_or(0),
        )?;
        let message = v1::Message {
            message_id: Uuid::new_v4().to_string(),
            conversation_id: conversation_id.to_owned(),
            sender_device_id: self.client.device_id().to_owned(),
            sent_at_ms: now,
            sequence_id: 0,
            content: Some(content),
        };
        let expires = now.checked_add(protocol::MAX_RETENTION_MS).ok_or(CoreError::Provider)?;
        let (_, envelopes) = self
            .client
            .seal_next_message_for_devices(message, &mut sequence, &fanout, expires, now)?;
        self.sequences.insert(conversation_id.to_owned(), sequence.last_sequence_id());
        self.send_envelopes(envelopes)
    }

    fn send_group_text(&mut self, conversation_id: &str, text: &str) -> Result<(), CoreError> {
        if text.is_empty() || text.len() > protocol::MAX_MESSAGE_BYTES {
            return Err(CoreError::Authentication);
        }
        self.send_group_content(conversation_id, v1::message::Content::Text(text.to_owned()))
    }

    fn send_group_name(&mut self, conversation_id: &str, name: &str) -> Result<(), CoreError> {
        let control = v1::MlsControl {
            protocol_version: protocol::VERSION,
            body: Some(v1::mls_control::Body::GroupInfo(v1::GroupInfo {
                name: name.trim().to_owned(),
            })),
        };
        protocol::validate_mls_control(&control)?;
        self.send_group_content(conversation_id, v1::message::Content::MlsControl(control))
    }

    /// Apply a mailbox Welcome or Commit. Returns the event to report, or
    /// None when the handshake does not apply to this device.
    fn apply_group_handshake(&mut self, raw: &[u8]) -> Result<Option<GroupEvent>, CoreError> {
        match mls_handshake_kind(raw)? {
            MlsHandshake::Welcome => {
                let _trust = TrustNewGuard::new(&self.trust_new);
                let conversation_id = self.client.mls_mut().join_group_from_welcome(raw)?;
                self.groups.insert(conversation_id.clone());
                Ok(Some(GroupEvent::Joined(conversation_id)))
            }
            MlsHandshake::Commit { conversation_id } if self.groups.contains(&conversation_id) => {
                let still_member = {
                    let _trust = TrustNewGuard::new(&self.trust_new);
                    self.client.mls_mut().process_group_commit(&conversation_id, raw)?
                };
                if still_member {
                    Ok(Some(GroupEvent::MembersChanged(conversation_id)))
                } else {
                    self.client.mls_mut().delete_group(&conversation_id)?;
                    self.groups.remove(&conversation_id);
                    Ok(Some(GroupEvent::Removed(conversation_id)))
                }
            }
            _ => Err(CoreError::Authentication),
        }
    }

    fn emit_group_events(&self, events: Vec<GroupEvent>) -> Result<(), CoreError> {
        let Some(callback) = self.callbacks.on_group else { return Ok(()) };
        for event in events {
            let (conversation, kind, sender, payload) = match &event {
                GroupEvent::Joined(id) => (id.as_str(), GROUP_EVENT_JOINED, "", ""),
                GroupEvent::Renamed { conversation_id, sender_user_id, name } => (
                    conversation_id.as_str(),
                    GROUP_EVENT_RENAMED,
                    sender_user_id.as_str(),
                    name.as_str(),
                ),
                GroupEvent::MembersChanged(id) => (id.as_str(), GROUP_EVENT_MEMBERS_CHANGED, "", ""),
                GroupEvent::Removed(id) => (id.as_str(), GROUP_EVENT_REMOVED, "", ""),
            };
            callback_status(unsafe {
                callback(
                    self.callbacks.context,
                    conversation.as_ptr(), conversation.len(),
                    kind,
                    sender.as_ptr(), sender.len(),
                    payload.as_ptr(), payload.len(),
                )
            })?;
        }
        Ok(())
    }

    fn handle_frame(&mut self, bytes: &[u8]) -> Result<(), CoreError> {
        let frame = v1::ServerFrame::decode(bytes).map_err(|_| CoreError::Protocol(protocol::ProtocolError::Malformed))?;
        protocol::validate_id(&frame.request_id)?;
        let Some(body) = frame.body else { return Err(CoreError::InvalidSync) };
        match body {
            v1::server_frame::Body::Welcome(_) => Ok(()),
            v1::server_frame::Body::Accepted(accepted) => {
                self.outbox.retain(|frame| {
                    match v1::ClientFrame::decode(frame.as_slice()).ok() {
                        Some(frame) => match frame.body {
                            Some(v1::client_frame::Body::Send(envelope)) => {
                                envelope.envelope_id != accepted.envelope_id
                            }
                            _ => true,
                        },
                        None => true,
                    }
                });
                self.bootstrap_outbox.retain(|frame| {
                    v1::ClientFrame::decode(frame.as_slice())
                        .map(|frame| frame.request_id != accepted.envelope_id)
                        .unwrap_or(true)
                });
                self.save()
            }
            v1::server_frame::Body::Error(error) => match error.code {
                2 => Err(CoreError::Authentication),
                5 => Err(CoreError::InvalidSync),
                6 => Ok(()),
                _ => Err(CoreError::Provider),
            },
            v1::server_frame::Body::MlsBootstrap(bootstrap) => {
                self.handle_mls_bootstrap(bootstrap).map_err(local_frame_error)
            }
            v1::server_frame::Body::Batch(batch) => self.handle_batch(batch).map_err(local_frame_error),
            v1::server_frame::Body::CompressedBatch(batch) => {
                let batch = protocol::decompress_sync_batch(&batch)?;
                self.handle_batch(batch).map_err(local_frame_error)
            }
            v1::server_frame::Body::WebRtcSignal(_) => Err(CoreError::Provider),
        }
    }

    fn handle_mls_bootstrap(&mut self, bootstrap: v1::MlsBootstrap) -> Result<(), CoreError> {
        protocol::validate_id(&bootstrap.conversation_id)?;
        if bootstrap.recipient_device_id != self.client.device_id()
            || bootstrap.commit.is_empty()
            || bootstrap.welcome.is_empty()
            || bootstrap.sender_identity_public_key.len() != 32
        {
            return Err(CoreError::Authentication);
        }
        let binding = parse_binding(&bootstrap.sender_mls_credential)?;
        let sender_public_key: [u8; 32] = bootstrap.sender_identity_public_key
            .as_slice()
            .try_into()
            .map_err(|_| CoreError::Authentication)?;
        let local_device = self
            .client
            .device_id()
            .parse::<Uuid>()
            .map_err(|_| CoreError::Authentication)?;
        if binding.device_id == local_device || binding.public_key != sender_public_key {
            return Err(CoreError::Authentication);
        }
        self.insert_binding(binding.clone())?;
        if bootstrap.reset_group {
            self.client
                .mls_mut()
                .reset_direct_group_from_welcome(&bootstrap.conversation_id, &bootstrap.welcome)?;
        } else {
            self.client
                .mls_mut()
                .join_direct_group(&bootstrap.conversation_id, &bootstrap.welcome)?;
        }
        self.save()?;
        if bootstrap.reset_group {
            self.discard_pending_batch()?;
        }
        self.retry_pending_batch()
    }

    /// Open one mailbox envelope. Group handshakes are applied here; failures
    /// surface as Authentication so the caller skips the item.
    fn open_mailbox_envelope(&mut self, envelope: &v1::Envelope) -> Result<MailboxItem, CoreError> {
        let raw = self.client.open_envelope_raw(envelope, now_ms())?;
        match mls_handshake_kind(raw.as_bytes()).map_err(|_| CoreError::Authentication)? {
            MlsHandshake::Application { .. } => {
                let opened = self.client.decrypt_opened(raw.as_bytes())?;
                Ok(MailboxItem::Message {
                    message: opened.message,
                    sender_user_id: opened.sender_user_id,
                })
            }
            MlsHandshake::Proposal { .. } => Ok(MailboxItem::Ignored),
            MlsHandshake::Welcome | MlsHandshake::Commit { .. } => self
                .apply_group_handshake(raw.as_bytes())
                .map(|event| event.map_or(MailboxItem::Ignored, MailboxItem::Event))
                .map_err(|_| CoreError::Authentication),
        }
    }

    fn handle_batch(&mut self, batch: v1::SyncBatch) -> Result<(), CoreError> {
        protocol::validate_sync_batch(&batch)?;
        if batch.recipient_device_id != self.client.device_id() || batch.after_cursor > self.cursor {
            return Err(CoreError::InvalidSync);
        }
        // Adding a member sends the commit, then the group name, before the
        // recipient's ack updates the gateway checkpoint. The second delivery
        // repeats the commit. Those items are already applied.
        let mut rendered = Vec::new();
        let mut events = Vec::new();
        let mut expected = self.cursor;
        for item in &batch.items {
            if item.cursor <= self.cursor {
                continue;
            }
            expected = expected.checked_add(1).ok_or(CoreError::InvalidSync)?;
            if item.cursor != expected {
                return Err(CoreError::InvalidSync);
            }
            let Some(entry) = item.entry.as_ref() else { return Err(CoreError::InvalidSync) };
            if let v1::queue_item::Entry::Envelope(envelope) = entry {
                let storage_backup = self
                    .client
                    .mls()
                    .provider()
                    .storage()
                    .values
                    .read()
                    .map_err(|_| CoreError::Provider)?
                    .clone();
                match self.open_mailbox_envelope(envelope) {
                    Ok(MailboxItem::Event(event)) => events.push(event),
                    Ok(MailboxItem::Ignored) => {}
                    Ok(MailboxItem::Message { message, sender_user_id }) => {
                        match message.content {
                            Some(v1::message::Content::Text(text)) => rendered.push((
                                message.conversation_id,
                                sender_user_id,
                                message.sender_device_id,
                                text,
                                message.sequence_id,
                                message.sent_at_ms,
                            )),
                            Some(v1::message::Content::MlsControl(v1::MlsControl {
                                body: Some(v1::mls_control::Body::GroupInfo(info)),
                                ..
                            })) if self.groups.contains(&message.conversation_id) => {
                                events.push(GroupEvent::Renamed {
                                    conversation_id: message.conversation_id,
                                    sender_user_id,
                                    name: info.name,
                                });
                            }
                            _ => {}
                        }
                    }
                    // The gateway delivers every stored welcome before the
                    // mailbox, so an envelope that still cannot be opened
                    // never will. Skipping it keeps later messages flowing.
                    Err(CoreError::Authentication) => {
                        let storage = self.client.mls_mut().provider_mut().storage();
                        let mut values = storage.values.write().map_err(|_| CoreError::Provider)?;
                        *values = storage_backup;
                    }
                    Err(error) => return Err(error),
                }
            }
        }
        if expected == self.cursor {
            if self.cursor < batch.high_watermark {
                let replay = self.encode_client_frame(v1::client_frame::Body::Replay(v1::Replay {
                    after_cursor: self.cursor,
                    limit: protocol::MAX_BATCH_ITEMS as u32,
                }))?;
                self.send_frame(&replay)?;
            }
            return Ok(());
        }
        if expected != batch.next_cursor {
            return Err(CoreError::InvalidSync);
        }
        self.cursor = expected;
        self.save()?;
        let ack = self.encode_client_frame(v1::client_frame::Body::Ack(v1::QueueAck { through_cursor: self.cursor }))?;
        self.send_frame(&ack)?;
        if self.cursor < batch.high_watermark {
            let replay = self.encode_client_frame(v1::client_frame::Body::Replay(v1::Replay {
                after_cursor: self.cursor,
                limit: protocol::MAX_BATCH_ITEMS as u32,
            }))?;
            self.send_frame(&replay)?;
        }
        self.emit_group_events(events)?;
        if let Some(callback) = self.callbacks.on_text {
            for (conversation, sender_user, sender, text, sequence, sent_at) in rendered {
                let conversation_bytes = conversation.as_bytes();
                let sender_user_bytes = sender_user.as_bytes();
                let sender_bytes = sender.as_bytes();
                let text_bytes = text.as_bytes();
                callback_status(unsafe {
                    callback(
                        self.callbacks.context,
                        conversation_bytes.as_ptr(), conversation_bytes.len(),
                        sender_user_bytes.as_ptr(), sender_user_bytes.len(),
                        sender_bytes.as_ptr(), sender_bytes.len(),
                        text_bytes.as_ptr(), text_bytes.len(), sequence, sent_at,
                    )
                })?;
            }
        }
        Ok(())
    }

    fn retry_pending_batch(&mut self) -> Result<(), CoreError> {
        let Some(batch) = self.pending_batch.take() else { return Ok(()) };
        match self.handle_batch(batch.clone()) {
            Ok(()) => Ok(()),
            Err(error) => {
                self.pending_batch = Some(batch);
                Err(error)
            }
        }
    }

    fn discard_pending_batch(&mut self) -> Result<(), CoreError> {
        let Some(batch) = self.pending_batch.take() else {
            self.discard_next_batch = true;
            self.save()?;
            return Ok(())
        };
        if batch.after_cursor != self.cursor || batch.next_cursor < self.cursor {
            return Err(CoreError::InvalidSync);
        }
        self.cursor = batch.next_cursor;
        self.discard_next_batch = batch.next_cursor < batch.high_watermark;
        self.save()?;
        let ack = self.encode_client_frame(v1::client_frame::Body::Ack(
            v1::QueueAck {
                through_cursor: self.cursor,
            },
        ))?;
        self.send_frame(&ack)?;
        if self.cursor < batch.high_watermark {
            let replay = self.encode_client_frame(v1::client_frame::Body::Replay(
                v1::Replay {
                    after_cursor: self.cursor,
                    limit: protocol::MAX_BATCH_ITEMS as u32,
                },
            ))?;
            self.send_frame(&replay)?;
        }
        Ok(())
    }
}

fn parse_binding_from_prekey_credential(record: &RecipientRecord) -> Result<links_identity::DeviceBinding, CoreError> {
    let binding = parse_binding(&record.mls_credential)?;
    if binding.user_id.to_string() != record.user_id
        || binding.device_id.to_string() != record.device_id
        || binding.public_key != record.identity_public_key
    {
        return Err(CoreError::Authentication);
    }
    Ok(binding)
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_millis().min(u128::from(u64::MAX)) as u64)
}

#[no_mangle]
pub unsafe extern "C" fn links_desktop_core_create(
    user_id: *const u8,
    user_id_length: usize,
    device_id: *const u8,
    device_id_length: usize,
    credential: *const u8,
    credential_length: usize,
    callbacks: *const LinksDesktopCoreCallbacks,
    output: *mut *mut LinksDesktopCore,
) -> i32 {
    boundary(|| {
        if output.is_null() || callbacks.is_null() {
            return LINKS_DESKTOP_INVALID;
        }
        let result = (|| -> Result<*mut LinksDesktopCore, i32> {
            let callbacks = unsafe { *callbacks };
            if callbacks.abi_version != ABI_VERSION
                || callbacks.sign.is_none()
                || callbacks.store_secret.is_none()
                || callbacks.load_secret.is_none()
                || callbacks.delete_secret.is_none()
                || callbacks.load_state.is_none()
                || callbacks.save_state.is_none()
                || callbacks.send_frame.is_none()
            {
                return Err(LINKS_DESKTOP_INVALID);
            }
            let user = String::from_utf8(unsafe { input(user_id, user_id_length, 64) }?.to_vec())
                .map_err(|_| LINKS_DESKTOP_INVALID)?;
            let device = String::from_utf8(unsafe { input(device_id, device_id_length, 64) }?.to_vec())
                .map_err(|_| LINKS_DESKTOP_INVALID)?;
            let credential = unsafe { input(credential, credential_length, 1024) }?;
            let local_binding = parse_binding(credential).map_err(status)?;
            let expected_user = Uuid::parse_str(&user).map_err(|_| LINKS_DESKTOP_INVALID)?;
            let expected_device = Uuid::parse_str(&device).map_err(|_| LINKS_DESKTOP_INVALID)?;
            if local_binding.user_id != expected_user || local_binding.device_id != expected_device
                || local_binding.public_key != callbacks.identity_public_key
            {
                return Err(LINKS_DESKTOP_AUTHENTICATION);
            }
            let bindings = Arc::new(RwLock::new(HashMap::from([(local_binding.device_id, local_binding.clone())])));
            let signer = CallbackSigner { callbacks };
            let trust_new = Arc::new(AtomicBool::new(false));
            let verifier = CallbackVerifier {
                bindings: Arc::clone(&bindings),
                trust_new: Arc::clone(&trust_new),
            };
            let provider = RustCryptoProvider::new(MemoryStorage::default());
            let mls = OpenMlsEngine::new(provider, signer.clone(), credential, verifier)
                .map_err(status)?;
            let local = LocalIdentity::new(user, device.clone()).map_err(status)?;
            let resolver = CallbackResolver {
                callbacks,
                local_device_id: device,
                identity_revision: 0,
                recipient_keys: HashMap::new(),
            };
            let client = ClientCore::new(local, SealedSenderCrypto::new(resolver), mls);
            let mut core = LinksDesktopCore {
                callbacks,
                client,
                mls_credential: credential.to_vec(),
                bindings,
                secrets: CallbackSecrets { callbacks },
                profile: None,
                cursor: 0,
                sequences: HashMap::new(),
                outbox: Vec::new(),
                bootstrap_outbox: Vec::new(),
                recipients: HashMap::new(),
                pending_batch: None,
                discard_next_batch: false,
                published_key_package: None,
                groups: HashSet::new(),
                trust_new,
            };
            if let Some(state) = load_state(callbacks).map_err(status)? {
                core.restore_state(state).map_err(status)?;
                core.save().map_err(status)?;
            }
            Ok(Box::into_raw(Box::new(core)))
        })();
        match result {
            Ok(pointer) => {
                unsafe { output.write(pointer) };
                LINKS_DESKTOP_OK
            }
            Err(code) => code,
        }
    })
}

#[no_mangle]
pub unsafe extern "C" fn links_desktop_core_destroy(core: *mut LinksDesktopCore) {
    if !core.is_null() {
        unsafe { drop(Box::from_raw(core)) };
    }
}

#[no_mangle]
pub unsafe extern "C" fn links_desktop_core_durable_cursor(core: *const LinksDesktopCore, cursor: *mut u64) -> i32 {
    boundary(|| {
        if core.is_null() || cursor.is_null() { return LINKS_DESKTOP_INVALID; }
        unsafe { cursor.write((*core).cursor) };
        LINKS_DESKTOP_OK
    })
}

#[no_mangle]
pub unsafe extern "C" fn links_desktop_core_pending_outbox_count(core: *const LinksDesktopCore, count: *mut usize) -> i32 {
    boundary(|| {
        if core.is_null() || count.is_null() { return LINKS_DESKTOP_INVALID; }
        unsafe { count.write((*core).outbox.len()) };
        LINKS_DESKTOP_OK
    })
}

#[no_mangle]
pub unsafe extern "C" fn links_desktop_core_pending_retry_count(core: *const LinksDesktopCore, count: *mut usize) -> i32 {
    boundary(|| {
        if core.is_null() || count.is_null() { return LINKS_DESKTOP_INVALID; }
        unsafe { count.write((*core).outbox.len().saturating_add((*core).bootstrap_outbox.len())) };
        LINKS_DESKTOP_OK
    })
}

#[no_mangle]
pub unsafe extern "C" fn links_desktop_core_create_hello(
    core: *const LinksDesktopCore,
    token: *const u8,
    token_length: usize,
    cursor: u64,
    output: *mut u8,
    capacity: usize,
    length: *mut usize,
) -> i32 {
    boundary(|| {
        if core.is_null() { return LINKS_DESKTOP_INVALID; }
        let result = (|| -> Result<Vec<u8>, CoreError> {
            let token = unsafe { input(token, token_length, 4096) }.map_err(|_| CoreError::Authentication)?;
            if token.is_empty() || cursor > protocol::MAX_CURSOR || cursor != unsafe { (*core).cursor } {
                return Err(CoreError::Authentication);
            }
            let frame = v1::ClientFrame {
                request_id: Uuid::new_v4().to_string(),
                body: Some(v1::client_frame::Body::Hello(v1::Hello {
                    protocol_version: protocol::VERSION,
                    device_id: unsafe { (*core).client.device_id().to_owned() },
                    device_access_token: token.to_vec(),
                    last_seen_cursor: cursor,
                    supported_sync_compression: vec![],
                })),
            };
            Ok(frame.encode_to_vec())
        })();
        match result {
            Ok(bytes) => unsafe { write_output(&bytes, output, capacity, length) },
            Err(error) => status(error),
        }
    })
}

#[no_mangle]
pub unsafe extern "C" fn links_desktop_core_handle_server_frame(core: *mut LinksDesktopCore, frame: *const u8, frame_length: usize) -> i32 {
    boundary(|| {
        if core.is_null() { return LINKS_DESKTOP_INVALID; }
        let result = (|| -> Result<(), CoreError> {
            let frame = unsafe { input(frame, frame_length, protocol::MAX_FRAME_BYTES) }.map_err(|_| CoreError::Provider)?;
            unsafe { (&mut *core).handle_frame(frame) }
        })();
        result.map_or_else(status, |_| LINKS_DESKTOP_OK)
    })
}

#[no_mangle]
pub unsafe extern "C" fn links_desktop_core_retry_outbox(core: *mut LinksDesktopCore) -> i32 {
    boundary(|| {
        if core.is_null() { return LINKS_DESKTOP_INVALID; }
        let result = (|| -> Result<(), CoreError> {
            let mut frames = unsafe { (&*core).outbox.clone() };
            frames.extend(unsafe { (&*core).bootstrap_outbox.clone() });
            for frame in frames { unsafe { (&*core).send_frame(&frame)?; } }
            Ok(())
        })();
        result.map_or_else(status, |_| LINKS_DESKTOP_OK)
    })
}

#[no_mangle]
pub unsafe extern "C" fn links_desktop_core_reset_replay_cursor(core: *mut LinksDesktopCore) -> i32 {
    boundary(|| {
        if core.is_null() { return LINKS_DESKTOP_INVALID; }
        let result = (|| -> Result<(), CoreError> {
            unsafe { (*core).cursor = 0; (*core).save() }
        })();
        result.map_or_else(status, |_| LINKS_DESKTOP_OK)
    })
}

#[no_mangle]
pub unsafe extern "C" fn links_desktop_core_generate_mls_key_package(
    core: *mut LinksDesktopCore,
    output: *mut u8,
    capacity: usize,
    length: *mut usize,
) -> i32 {
    boundary(|| {
        if core.is_null() { return LINKS_DESKTOP_INVALID; }
        let result = unsafe { (&mut *core).key_package() };
        match result { Ok(bytes) => unsafe { write_output(&bytes, output, capacity, length) }, Err(error) => status(error) }
    })
}

#[no_mangle]
pub unsafe extern "C" fn links_desktop_core_generate_prekey_upload(
    core: *mut LinksDesktopCore,
    curve_count: u32,
    kem_count: u32,
    output: *mut u8,
    capacity: usize,
    length: *mut usize,
) -> i32 {
    boundary(|| {
        if core.is_null() { return LINKS_DESKTOP_INVALID; }
        let result = unsafe { (&mut *core).generate_prekey_upload(curve_count, kem_count) };
        match result { Ok(bytes) => unsafe { write_output(&bytes, output, capacity, length) }, Err(error) => status(error) }
    })
}

#[no_mangle]
pub unsafe extern "C" fn links_desktop_core_has_recipient(
    core: *const LinksDesktopCore,
    user_id: *const u8,
    user_id_length: usize,
    output: *mut u8,
) -> i32 {
    boundary(|| {
        if core.is_null() || output.is_null() {
            return LINKS_DESKTOP_INVALID;
        }
        let user_bytes = match unsafe { input(user_id, user_id_length, 64) } {
            Ok(bytes) => bytes,
            Err(_) => return LINKS_DESKTOP_INVALID,
        };
        let user = match String::from_utf8(user_bytes.to_vec()) {
            Ok(user) => user,
            Err(_) => return LINKS_DESKTOP_INVALID,
        };
        if protocol::validate_id(&user).is_err() {
            return LINKS_DESKTOP_INVALID;
        }
        let has_recipient = unsafe {
            (&*core)
                .recipients
                .get(&user)
                .map(|records| !records.is_empty())
                .unwrap_or(false)
        };
        unsafe { output.write(u8::from(has_recipient)) };
        LINKS_DESKTOP_OK
    })
}

#[no_mangle]
pub unsafe extern "C" fn links_desktop_core_set_recipient(
    core: *mut LinksDesktopCore,
    user_id: *const u8,
    user_id_length: usize,
    device_id: *const u8,
    device_id_length: usize,
    identity_public_key: *const u8,
    identity_public_key_length: usize,
    prekey_bundle: *const u8,
    prekey_bundle_length: usize,
    mls_credential: *const u8,
    mls_credential_length: usize,
    mls_key_package: *const u8,
    mls_key_package_length: usize,
) -> i32 {
    boundary(|| {
        if core.is_null() { return LINKS_DESKTOP_INVALID; }
        let result = (|| -> Result<(), CoreError> {
            let user = String::from_utf8(unsafe { input(user_id, user_id_length, 64) }.map_err(|_| CoreError::Authentication)?.to_vec()).map_err(|_| CoreError::Authentication)?;
            let device = String::from_utf8(unsafe { input(device_id, device_id_length, 64) }.map_err(|_| CoreError::Authentication)?.to_vec()).map_err(|_| CoreError::Authentication)?;
            let public_key: [u8; 32] = unsafe { input(identity_public_key, identity_public_key_length, 32) }.map_err(|_| CoreError::Authentication)?.try_into().map_err(|_| CoreError::Authentication)?;
            let bundle = v1::PreKeyBundle::decode(unsafe { input(prekey_bundle, prekey_bundle_length, protocol::MAX_PREKEY_UPLOAD_BYTES) }.map_err(|_| CoreError::Authentication)?).map_err(|_| CoreError::Authentication)?;
            let credential = unsafe { input(mls_credential, mls_credential_length, 1024) }.map_err(|_| CoreError::Authentication)?.to_vec();
            let package = unsafe { input(mls_key_package, mls_key_package_length, protocol::MAX_FRAME_BYTES) }.map_err(|_| CoreError::Authentication)?.to_vec();
            let record = RecipientRecord { user_id: user, device_id: device, identity_public_key: public_key, prekey_bundle: bundle, mls_credential: credential, mls_key_package: package };
            unsafe { (&mut *core).set_recipient(record) }
        })();
        result.map_or_else(status, |_| LINKS_DESKTOP_OK)
    })
}

#[no_mangle]
pub unsafe extern "C" fn links_desktop_core_initialize_direct(
    core: *mut LinksDesktopCore,
    conversation_id: *const u8,
    conversation_id_length: usize,
    recipient_user_id: *const u8,
    recipient_user_id_length: usize,
) -> i32 {
    boundary(|| {
        if core.is_null() {
            return LINKS_DESKTOP_INVALID;
        }
        let result = (|| -> Result<(), CoreError> {
            let conversation = String::from_utf8(
                unsafe { input(conversation_id, conversation_id_length, 64) }
                    .map_err(|_| CoreError::Authentication)?
                    .to_vec(),
            )
            .map_err(|_| CoreError::Authentication)?;
            let recipient = String::from_utf8(
                unsafe { input(recipient_user_id, recipient_user_id_length, 64) }
                    .map_err(|_| CoreError::Authentication)?
                    .to_vec(),
            )
            .map_err(|_| CoreError::Authentication)?;
            unsafe { (&mut *core).initialize_direct_group(&conversation, &recipient, false) }
        })();
        result.map_or_else(status, |_| LINKS_DESKTOP_OK)
    })
}

#[no_mangle]
pub unsafe extern "C" fn links_desktop_core_reset_direct(
    core: *mut LinksDesktopCore,
    conversation_id: *const u8,
    conversation_id_length: usize,
    recipient_user_id: *const u8,
    recipient_user_id_length: usize,
) -> i32 {
    boundary(|| {
        if core.is_null() {
            return LINKS_DESKTOP_INVALID;
        }
        let result = (|| -> Result<(), CoreError> {
            let conversation = String::from_utf8(
                unsafe { input(conversation_id, conversation_id_length, 64) }
                    .map_err(|_| CoreError::Authentication)?
                    .to_vec(),
            )
            .map_err(|_| CoreError::Authentication)?;
            let recipient = String::from_utf8(
                unsafe { input(recipient_user_id, recipient_user_id_length, 64) }
                    .map_err(|_| CoreError::Authentication)?
                    .to_vec(),
            )
            .map_err(|_| CoreError::Authentication)?;
            unsafe { (&mut *core).reset_direct_group(&conversation, &recipient) }
        })();
        result.map_or_else(status, |_| LINKS_DESKTOP_OK)
    })
}

#[no_mangle]
pub unsafe extern "C" fn links_desktop_core_send_text(
    core: *mut LinksDesktopCore,
    conversation_id: *const u8,
    conversation_id_length: usize,
    recipient_user_id: *const u8,
    recipient_user_id_length: usize,
    text: *const u8,
    text_length: usize,
) -> i32 {
    boundary(|| {
        if core.is_null() { return LINKS_DESKTOP_INVALID; }
        let result = (|| -> Result<(), CoreError> {
            let conversation = String::from_utf8(unsafe { input(conversation_id, conversation_id_length, 64) }.map_err(|_| CoreError::Authentication)?.to_vec()).map_err(|_| CoreError::Authentication)?;
            let recipient = String::from_utf8(unsafe { input(recipient_user_id, recipient_user_id_length, 64) }.map_err(|_| CoreError::Authentication)?.to_vec()).map_err(|_| CoreError::Authentication)?;
            let text = String::from_utf8(unsafe { input(text, text_length, protocol::MAX_MESSAGE_BYTES) }.map_err(|_| CoreError::Authentication)?.to_vec()).map_err(|_| CoreError::Authentication)?;
            unsafe { (&mut *core).send_text(&conversation, &recipient, &text) }
        })();
        result.map_or_else(status, |_| LINKS_DESKTOP_OK)
    })
}

unsafe fn input_string(pointer: *const u8, length: usize, maximum: usize) -> Result<String, CoreError> {
    let bytes = unsafe { input(pointer, length, maximum) }.map_err(|_| CoreError::Authentication)?;
    String::from_utf8(bytes.to_vec()).map_err(|_| CoreError::Authentication)
}

unsafe fn group_call(
    core: *mut LinksDesktopCore,
    conversation_id: *const u8,
    conversation_id_length: usize,
    work: impl FnOnce(&mut LinksDesktopCore, String) -> Result<(), CoreError>,
) -> i32 {
    boundary(|| {
        if core.is_null() { return LINKS_DESKTOP_INVALID; }
        let result = (|| -> Result<(), CoreError> {
            let conversation = unsafe { input_string(conversation_id, conversation_id_length, 64) }?;
            work(unsafe { &mut *core }, conversation)
        })();
        result.map_or_else(status, |_| LINKS_DESKTOP_OK)
    })
}

unsafe fn group_list_call(
    core: *const LinksDesktopCore,
    conversation_id: *const u8,
    conversation_id_length: usize,
    output: *mut u8,
    capacity: usize,
    length: *mut usize,
    work: impl FnOnce(&LinksDesktopCore, &str) -> Result<Vec<String>, CoreError>,
) -> i32 {
    boundary(|| {
        if core.is_null() { return LINKS_DESKTOP_INVALID; }
        let result = (|| -> Result<Vec<u8>, CoreError> {
            let conversation = unsafe { input_string(conversation_id, conversation_id_length, 64) }?;
            Ok(work(unsafe { &*core }, &conversation)?.join("\n").into_bytes())
        })();
        match result {
            Ok(bytes) => unsafe { write_output(&bytes, output, capacity, length) },
            Err(error) => status(error),
        }
    })
}

#[no_mangle]
pub unsafe extern "C" fn links_desktop_core_create_group(
    core: *mut LinksDesktopCore,
    conversation_id: *const u8,
    conversation_id_length: usize,
) -> i32 {
    unsafe { group_call(core, conversation_id, conversation_id_length, |core, conversation| core.create_group(&conversation)) }
}

/// `user_ids` is a newline-separated list of account IDs whose devices were
/// already registered with `links_desktop_core_set_recipient`.
#[no_mangle]
pub unsafe extern "C" fn links_desktop_core_add_group_members(
    core: *mut LinksDesktopCore,
    conversation_id: *const u8,
    conversation_id_length: usize,
    user_ids: *const u8,
    user_ids_length: usize,
) -> i32 {
    unsafe {
        group_call(core, conversation_id, conversation_id_length, |core, conversation| {
            let list = input_string(user_ids, user_ids_length, 64 * MAX_GROUP_USER_BATCH)?;
            let users = list.split('\n').filter(|user| !user.is_empty()).map(str::to_owned).collect::<Vec<_>>();
            core.add_group_members(&conversation, &users)
        })
    }
}

#[no_mangle]
pub unsafe extern "C" fn links_desktop_core_remove_group_member(
    core: *mut LinksDesktopCore,
    conversation_id: *const u8,
    conversation_id_length: usize,
    user_id: *const u8,
    user_id_length: usize,
) -> i32 {
    unsafe {
        group_call(core, conversation_id, conversation_id_length, |core, conversation| {
            let user = input_string(user_id, user_id_length, 64)?;
            core.remove_group_member(&conversation, &user)
        })
    }
}

#[no_mangle]
pub unsafe extern "C" fn links_desktop_core_leave_group(
    core: *mut LinksDesktopCore,
    conversation_id: *const u8,
    conversation_id_length: usize,
) -> i32 {
    unsafe { group_call(core, conversation_id, conversation_id_length, |core, conversation| core.leave_group(&conversation)) }
}

#[no_mangle]
pub unsafe extern "C" fn links_desktop_core_send_group_text(
    core: *mut LinksDesktopCore,
    conversation_id: *const u8,
    conversation_id_length: usize,
    text: *const u8,
    text_length: usize,
) -> i32 {
    unsafe {
        group_call(core, conversation_id, conversation_id_length, |core, conversation| {
            let text = input_string(text, text_length, protocol::MAX_MESSAGE_BYTES)?;
            core.send_group_text(&conversation, &text)
        })
    }
}

#[no_mangle]
pub unsafe extern "C" fn links_desktop_core_set_group_name(
    core: *mut LinksDesktopCore,
    conversation_id: *const u8,
    conversation_id_length: usize,
    name: *const u8,
    name_length: usize,
) -> i32 {
    unsafe {
        group_call(core, conversation_id, conversation_id_length, |core, conversation| {
            let name = input_string(name, name_length, 4 * protocol::MAX_GROUP_NAME_CHARS)?;
            core.send_group_name(&conversation, &name)
        })
    }
}

/// Newline-separated account IDs of every current member, including self.
#[no_mangle]
pub unsafe extern "C" fn links_desktop_core_group_members(
    core: *const LinksDesktopCore,
    conversation_id: *const u8,
    conversation_id_length: usize,
    output: *mut u8,
    capacity: usize,
    length: *mut usize,
) -> i32 {
    unsafe {
        group_list_call(core, conversation_id, conversation_id_length, output, capacity, length, |core, conversation| {
            core.group_member_users(conversation)
        })
    }
}

/// Newline-separated account IDs with a device this core cannot seal to yet.
#[no_mangle]
pub unsafe extern "C" fn links_desktop_core_group_missing_recipients(
    core: *const LinksDesktopCore,
    conversation_id: *const u8,
    conversation_id_length: usize,
    output: *mut u8,
    capacity: usize,
    length: *mut usize,
) -> i32 {
    unsafe {
        group_list_call(core, conversation_id, conversation_id_length, output, capacity, length, |core, conversation| {
            core.group_users_missing_recipients(conversation)
        })
    }
}

#[cfg(test)]
mod group_tests {
    use super::*;
    use links_identity::IdentitySeed;
    use std::collections::VecDeque;

    #[derive(Default)]
    struct Host {
        seed: Option<IdentitySeed>,
        secrets: HashMap<Vec<u8>, Vec<u8>>,
        state: Vec<u8>,
        sent: Vec<Vec<u8>>,
        texts: Vec<(String, String, String)>,
        events: Vec<(String, u32, String)>,
    }

    unsafe fn host<'a>(context: *mut c_void) -> &'a mut Host {
        unsafe { &mut *(context as *mut Host) }
    }

    unsafe fn bytes<'a>(pointer: *const u8, length: usize) -> &'a [u8] {
        if length == 0 { &[] } else { unsafe { slice::from_raw_parts(pointer, length) } }
    }

    unsafe extern "C" fn sign(context: *mut c_void, data: *const u8, length: usize, output: *mut u8) -> i32 {
        let host = unsafe { host(context) };
        let signature = host.seed.as_ref().unwrap().sign(unsafe { bytes(data, length) });
        unsafe { output.copy_from_nonoverlapping(signature.as_ptr(), 64) };
        LINKS_DESKTOP_OK
    }

    unsafe extern "C" fn store_secret(context: *mut c_void, key: *const u8, key_length: usize, secret: *const u8, secret_length: usize) -> i32 {
        let host = unsafe { host(context) };
        host.secrets.insert(unsafe { bytes(key, key_length) }.to_vec(), unsafe { bytes(secret, secret_length) }.to_vec());
        LINKS_DESKTOP_OK
    }

    unsafe extern "C" fn load_secret(context: *mut c_void, key: *const u8, key_length: usize, output: *mut u8, capacity: usize, length: *mut usize) -> i32 {
        let host = unsafe { host(context) };
        let Some(secret) = host.secrets.get(unsafe { bytes(key, key_length) }) else { return LINKS_DESKTOP_PROVIDER };
        unsafe { length.write(secret.len()) };
        if output.is_null() || capacity < secret.len() { return LINKS_DESKTOP_INVALID; }
        unsafe { output.copy_from_nonoverlapping(secret.as_ptr(), secret.len()) };
        LINKS_DESKTOP_OK
    }

    unsafe extern "C" fn delete_secret(context: *mut c_void, key: *const u8, key_length: usize) -> i32 {
        unsafe { host(context) }.secrets.remove(unsafe { bytes(key, key_length) });
        LINKS_DESKTOP_OK
    }

    unsafe extern "C" fn load_state(context: *mut c_void, output: *mut u8, capacity: usize, length: *mut usize) -> i32 {
        let host = unsafe { host(context) };
        unsafe { length.write(host.state.len()) };
        if output.is_null() { return LINKS_DESKTOP_OK; }
        if capacity < host.state.len() { return LINKS_DESKTOP_INVALID; }
        unsafe { output.copy_from_nonoverlapping(host.state.as_ptr(), host.state.len()) };
        LINKS_DESKTOP_OK
    }

    unsafe extern "C" fn save_state(context: *mut c_void, data: *const u8, length: usize) -> i32 {
        unsafe { host(context) }.state = unsafe { bytes(data, length) }.to_vec();
        LINKS_DESKTOP_OK
    }

    unsafe extern "C" fn send_frame(context: *mut c_void, data: *const u8, length: usize) -> i32 {
        unsafe { host(context) }.sent.push(unsafe { bytes(data, length) }.to_vec());
        LINKS_DESKTOP_OK
    }

    unsafe extern "C" fn on_text(
        context: *mut c_void,
        conversation: *const u8, conversation_length: usize,
        sender_user: *const u8, sender_user_length: usize,
        _sender: *const u8, _sender_length: usize,
        text: *const u8, text_length: usize,
        _sequence: u64, _sent_at: u64,
    ) -> i32 {
        let host = unsafe { host(context) };
        let text_of = |pointer, length| String::from_utf8(unsafe { bytes(pointer, length) }.to_vec()).unwrap();
        host.texts.push((
            text_of(conversation, conversation_length),
            text_of(sender_user, sender_user_length),
            text_of(text, text_length),
        ));
        LINKS_DESKTOP_OK
    }

    unsafe extern "C" fn on_group(
        context: *mut c_void,
        conversation: *const u8, conversation_length: usize,
        kind: u32,
        _sender: *const u8, _sender_length: usize,
        payload: *const u8, payload_length: usize,
    ) -> i32 {
        let host = unsafe { host(context) };
        let text_of = |pointer, length| String::from_utf8(unsafe { bytes(pointer, length) }.to_vec()).unwrap();
        host.events.push((text_of(conversation, conversation_length), kind, text_of(payload, payload_length)));
        LINKS_DESKTOP_OK
    }

    struct Device {
        core: *mut LinksDesktopCore,
        host: Box<Host>,
        user_id: String,
        device_id: String,
        public_key: [u8; 32],
        credential: Vec<u8>,
        bundle: Vec<u8>,
        key_package: Vec<u8>,
        cursor: u64,
    }

    fn random_uuid() -> Uuid {
        let mut raw = [0u8; 16];
        getrandom_fill(&mut raw);
        raw[6] = (raw[6] & 0x0f) | 0x40;
        raw[8] = (raw[8] & 0x3f) | 0x80;
        Uuid::from_bytes(raw)
    }

    fn getrandom_fill(bytes: &mut [u8]) {
        let seed = IdentitySeed::generate().unwrap();
        bytes.copy_from_slice(&seed.expose_for_wrapping()[..bytes.len()]);
    }

    fn output_of(call: impl Fn(*mut u8, usize, *mut usize) -> i32) -> Vec<u8> {
        let mut buffer = vec![0u8; 2 * 1024 * 1024];
        let mut length = 0usize;
        assert_eq!(call(buffer.as_mut_ptr(), buffer.len(), &mut length), LINKS_DESKTOP_OK);
        buffer.truncate(length);
        buffer
    }

    impl Device {
        fn new() -> Self {
            let seed = IdentitySeed::generate().unwrap();
            let public_key = seed.public_key();
            let user = random_uuid();
            let device = random_uuid();
            let credential = links_identity::DeviceBinding {
                user_id: user,
                device_id: device,
                mls_node_id: random_uuid(),
                public_key,
            }
            .mls_credential()
            .unwrap();
            let mut host = Box::new(Host { seed: Some(seed), ..Host::default() });
            let callbacks = LinksDesktopCoreCallbacks {
                abi_version: ABI_VERSION,
                context: (&mut *host) as *mut Host as *mut c_void,
                sign: Some(sign),
                store_secret: Some(store_secret),
                load_secret: Some(load_secret),
                delete_secret: Some(delete_secret),
                load_state: Some(load_state),
                save_state: Some(save_state),
                send_frame: Some(send_frame),
                on_text: Some(on_text),
                identity_public_key: public_key,
                on_group: Some(on_group),
            };
            let user_id = user.to_string();
            let device_id = device.to_string();
            let mut core = std::ptr::null_mut();
            let status = unsafe {
                links_desktop_core_create(
                    user_id.as_ptr(), user_id.len(),
                    device_id.as_ptr(), device_id.len(),
                    credential.as_ptr(), credential.len(),
                    &callbacks, &mut core,
                )
            };
            assert_eq!(status, LINKS_DESKTOP_OK);
            let upload = output_of(|out, cap, len| unsafe {
                links_desktop_core_generate_prekey_upload(core, 2, 2, out, cap, len)
            });
            let upload = v1::PreKeyUpload::decode(upload.as_slice()).unwrap();
            let bundle = v1::PreKeyBundle {
                protocol_version: protocol::VERSION,
                device_id: device_id.clone(),
                profile_revision: upload.profile_revision,
                profile: upload.profile.clone(),
                one_time_curve_prekey: upload.one_time_curve_prekeys.first().cloned(),
                kem_prekey: upload.one_time_kem_prekeys.first().cloned(),
            }
            .encode_to_vec();
            let key_package = output_of(|out, cap, len| unsafe {
                links_desktop_core_generate_mls_key_package(core, out, cap, len)
            });
            Self { core, host, user_id, device_id, public_key, credential, bundle, key_package, cursor: 0 }
        }

        fn know(&self, peer: &Device) {
            let status = unsafe {
                links_desktop_core_set_recipient(
                    self.core,
                    peer.user_id.as_ptr(), peer.user_id.len(),
                    peer.device_id.as_ptr(), peer.device_id.len(),
                    peer.public_key.as_ptr(), peer.public_key.len(),
                    peer.bundle.as_ptr(), peer.bundle.len(),
                    peer.credential.as_ptr(), peer.credential.len(),
                    peer.key_package.as_ptr(), peer.key_package.len(),
                )
            };
            assert_eq!(status, LINKS_DESKTOP_OK);
        }

        fn members(&self, group: &str) -> Vec<String> {
            let raw = output_of(|out, cap, len| unsafe {
                links_desktop_core_group_members(self.core, group.as_ptr(), group.len(), out, cap, len)
            });
            let mut users = String::from_utf8(raw).unwrap().split('\n').map(str::to_owned).collect::<Vec<_>>();
            users.sort();
            users
        }

        fn missing(&self, group: &str) -> Vec<String> {
            let raw = output_of(|out, cap, len| unsafe {
                links_desktop_core_group_missing_recipients(self.core, group.as_ptr(), group.len(), out, cap, len)
            });
            String::from_utf8(raw).unwrap().split('\n').filter(|s| !s.is_empty()).map(str::to_owned).collect()
        }

        fn send_text(&self, group: &str, text: &str) -> i32 {
            unsafe { links_desktop_core_send_group_text(self.core, group.as_ptr(), group.len(), text.as_ptr(), text.len()) }
        }
    }

    /// Minimal gateway: routes Send envelopes into per-device mailboxes and
    /// delivers them as ordered sync batches.
    #[derive(Default)]
    struct Mailboxes {
        queues: HashMap<String, VecDeque<v1::Envelope>>,
        next_cursor: HashMap<String, u64>,
    }

    impl Mailboxes {
        fn collect(&mut self, device: &mut Device) {
            for frame in device.host.sent.drain(..) {
                let frame = v1::ClientFrame::decode(frame.as_slice()).unwrap();
                if let Some(v1::client_frame::Body::Send(envelope)) = frame.body {
                    self.queues.entry(envelope.recipient_device_id.clone()).or_default().push_back(envelope);
                }
            }
        }

        fn deliver(&mut self, device: &mut Device) {
            let queue = self.queues.entry(device.device_id.clone()).or_default();
            if queue.is_empty() {
                return;
            }
            let start = *self.next_cursor.get(&device.device_id).unwrap_or(&0);
            let mut items = Vec::new();
            let mut cursor = start;
            while let Some(envelope) = queue.pop_front() {
                cursor += 1;
                items.push(v1::QueueItem { cursor, entry: Some(v1::queue_item::Entry::Envelope(envelope)) });
            }
            self.next_cursor.insert(device.device_id.clone(), cursor);
            let frame = v1::ServerFrame {
                request_id: Uuid::new_v4().to_string(),
                body: Some(v1::server_frame::Body::Batch(v1::SyncBatch {
                    recipient_device_id: device.device_id.clone(),
                    after_cursor: device.cursor,
                    next_cursor: cursor,
                    high_watermark: cursor,
                    items,
                })),
            }
            .encode_to_vec();
            let status = unsafe { links_desktop_core_handle_server_frame(device.core, frame.as_ptr(), frame.len()) };
            assert_eq!(status, LINKS_DESKTOP_OK);
            device.cursor = cursor;
            device.host.sent.clear();
        }
    }

    #[test]
    fn group_invite_message_rename_and_remove_across_devices() {
        let mut alice = Device::new();
        let mut bob = Device::new();
        let mut carol = Device::new();
        let mut mail = Mailboxes::default();
        let group = Uuid::new_v4().to_string();

        assert_eq!(unsafe { links_desktop_core_create_group(alice.core, group.as_ptr(), group.len()) }, LINKS_DESKTOP_OK);
        alice.know(&bob);
        alice.know(&carol);
        let invitees = format!("{}\n{}", bob.user_id, carol.user_id);
        let status = unsafe {
            links_desktop_core_add_group_members(alice.core, group.as_ptr(), group.len(), invitees.as_ptr(), invitees.len())
        };
        assert_eq!(status, LINKS_DESKTOP_OK);
        let name = "Weekend plans";
        assert_eq!(
            unsafe { links_desktop_core_set_group_name(alice.core, group.as_ptr(), group.len(), name.as_ptr(), name.len()) },
            LINKS_DESKTOP_OK
        );
        assert_eq!(alice.send_text(&group, "hello both"), LINKS_DESKTOP_OK);
        mail.collect(&mut alice);
        mail.deliver(&mut bob);
        mail.deliver(&mut carol);

        for member in [&bob, &carol] {
            assert!(member.host.events.contains(&(group.clone(), GROUP_EVENT_JOINED, String::new())));
            assert!(member.host.events.contains(&(group.clone(), GROUP_EVENT_RENAMED, name.to_owned())));
            assert_eq!(member.host.texts, vec![(group.clone(), alice.user_id.clone(), "hello both".to_owned())]);
        }
        let mut everyone = vec![alice.user_id.clone(), bob.user_id.clone(), carol.user_id.clone()];
        everyone.sort();
        assert_eq!(bob.members(&group), everyone);

        // A joiner must register the other members before it can send.
        assert_eq!(bob.send_text(&group, "hi"), LINKS_DESKTOP_AUTHENTICATION);
        let mut missing = bob.missing(&group);
        missing.sort();
        let mut expected = vec![alice.user_id.clone(), carol.user_id.clone()];
        expected.sort();
        assert_eq!(missing, expected);
        bob.know(&alice);
        bob.know(&carol);
        assert!(bob.missing(&group).is_empty());
        assert_eq!(bob.send_text(&group, "hi from bob"), LINKS_DESKTOP_OK);
        mail.collect(&mut bob);
        mail.deliver(&mut alice);
        mail.deliver(&mut carol);
        assert_eq!(alice.host.texts, vec![(group.clone(), bob.user_id.clone(), "hi from bob".to_owned())]);
        assert!(carol.host.texts.contains(&(group.clone(), bob.user_id.clone(), "hi from bob".to_owned())));

        let status = unsafe {
            links_desktop_core_remove_group_member(alice.core, group.as_ptr(), group.len(), carol.user_id.as_ptr(), carol.user_id.len())
        };
        assert_eq!(status, LINKS_DESKTOP_OK);
        assert_eq!(alice.send_text(&group, "carol left"), LINKS_DESKTOP_OK);
        mail.collect(&mut alice);
        mail.deliver(&mut bob);
        mail.deliver(&mut carol);
        assert!(bob.host.events.contains(&(group.clone(), GROUP_EVENT_MEMBERS_CHANGED, String::new())));
        assert!(bob.host.texts.contains(&(group.clone(), alice.user_id.clone(), "carol left".to_owned())));
        assert!(carol.host.events.contains(&(group.clone(), GROUP_EVENT_REMOVED, String::new())));
        assert!(!carol.host.texts.iter().any(|(_, _, text)| text == "carol left"));
        let mut remaining = vec![alice.user_id.clone(), bob.user_id.clone()];
        remaining.sort();
        assert_eq!(bob.members(&group), remaining);

        for device in [&alice, &bob, &carol] {
            unsafe { links_desktop_core_destroy(device.core) };
        }
    }

    #[test]
    fn overlapping_group_delivery_is_not_a_stale_cursor() {
        let mut alice = Device::new();
        let mut bob = Device::new();
        let mut mail = Mailboxes::default();
        let group = Uuid::new_v4().to_string();
        assert_eq!(unsafe { links_desktop_core_create_group(alice.core, group.as_ptr(), group.len()) }, LINKS_DESKTOP_OK);
        alice.know(&bob);
        let status = unsafe {
            links_desktop_core_add_group_members(
                alice.core, group.as_ptr(), group.len(), bob.user_id.as_ptr(), bob.user_id.len(),
            )
        };
        assert_eq!(status, LINKS_DESKTOP_OK);
        let name = "Weekend";
        assert_eq!(
            unsafe {
                links_desktop_core_set_group_name(alice.core, group.as_ptr(), group.len(), name.as_ptr(), name.len())
            },
            LINKS_DESKTOP_OK
        );
        mail.collect(&mut alice);
        let queued = mail.queues.remove(&bob.device_id).unwrap().into_iter().collect::<Vec<_>>();
        assert!(queued.len() >= 2);

        let deliver = |device: &mut Device, after: u64, envelopes: &[v1::Envelope]| {
            let mut cursor = after;
            let items = envelopes
                .iter()
                .map(|envelope| {
                    cursor += 1;
                    v1::QueueItem {
                        cursor,
                        entry: Some(v1::queue_item::Entry::Envelope(envelope.clone())),
                    }
                })
                .collect::<Vec<_>>();
            let next = cursor;
            let frame = v1::ServerFrame {
                request_id: Uuid::new_v4().to_string(),
                body: Some(v1::server_frame::Body::Batch(v1::SyncBatch {
                    recipient_device_id: device.device_id.clone(),
                    after_cursor: after,
                    next_cursor: next,
                    high_watermark: next,
                    items,
                })),
            }
            .encode_to_vec();
            unsafe { links_desktop_core_handle_server_frame(device.core, frame.as_ptr(), frame.len()) }
        };

        assert_eq!(deliver(&mut bob, 0, &queued[..1]), LINKS_DESKTOP_OK);
        assert!(bob.host.events.iter().any(|(_, kind, _)| *kind == GROUP_EVENT_JOINED));
        // The gateway still checkpoints at the unacknowledged cursor, so this
        // batch repeats the welcome and appends the group name.
        assert_eq!(deliver(&mut bob, 0, &queued), LINKS_DESKTOP_OK);
        assert_eq!(
            bob.host.events.iter().filter(|(_, kind, _)| *kind == GROUP_EVENT_JOINED).count(),
            1
        );
        assert!(bob.host.events.contains(&(group.clone(), GROUP_EVENT_RENAMED, name.to_owned())));

        unsafe {
            links_desktop_core_destroy(alice.core);
            links_desktop_core_destroy(bob.core);
        }
    }
}
