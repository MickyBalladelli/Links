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
    mls::{MlsCredentialVerifier, MlsEngine, OpenMlsEngine, RustCryptoProvider},
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
use std::{collections::{HashMap, HashSet}, ffi::c_void, panic::AssertUnwindSafe, slice, sync::{Arc, RwLock}};
use uuid::Uuid;
use zeroize::Zeroizing;

pub const LINKS_DESKTOP_OK: i32 = 0;
pub const LINKS_DESKTOP_INVALID: i32 = 1;
pub const LINKS_DESKTOP_UNAVAILABLE: i32 = 2;
pub const LINKS_DESKTOP_AUTHENTICATION: i32 = 3;
pub const LINKS_DESKTOP_PROVIDER: i32 = 4;
pub const LINKS_DESKTOP_STALE_CURSOR: i32 = 5;
const ABI_VERSION: u32 = 1;
const MAX_OUTPUT_BYTES: usize = protocol::MAX_FRAME_BYTES;

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
}

impl MlsCredentialVerifier for CallbackVerifier {
    fn verify(&self, binding: &links_identity::DeviceBinding) -> Result<(), CoreError> {
        let bindings = self.bindings.read().map_err(|_| CoreError::Provider)?;
        let Some(expected) = bindings.get(&binding.device_id) else {
            return Err(CoreError::Authentication);
        };
        if expected != binding {
            return Err(CoreError::Authentication);
        }
        Ok(())
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
    if length > MAX_OUTPUT_BYTES {
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
            bindings: bindings.values().map(|binding| PersistedBinding {
                user_id: binding.user_id.as_bytes().to_vec(),
                device_id: binding.device_id.as_bytes().to_vec(),
                mls_node_id: binding.mls_node_id.as_bytes().to_vec(),
                public_key: binding.public_key.to_vec(),
            }).collect(),
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
        self.bootstrap_outbox = state.bootstrap_outbox;
        self.discard_next_batch = state.discard_next_batch;
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

    fn key_package(&mut self) -> Result<Vec<u8>, CoreError> {
        let package = self.client.mls_mut().generate_key_package()?;
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
                        .reset_direct_group_from_welcome(
                            &bootstrap.conversation_id,
                            &bootstrap.welcome,
                        )?;
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
            }.map_err(local_frame_error),
            v1::server_frame::Body::Batch(batch) => self.handle_batch(batch).map_err(local_frame_error),
            v1::server_frame::Body::CompressedBatch(batch) => {
                let batch = protocol::decompress_sync_batch(&batch)?;
                self.handle_batch(batch).map_err(local_frame_error)
            }
            v1::server_frame::Body::WebRtcSignal(_) => Err(CoreError::Provider),
        }
    }

    fn handle_batch(&mut self, batch: v1::SyncBatch) -> Result<(), CoreError> {
        protocol::validate_sync_batch(&batch)?;
        if batch.recipient_device_id != self.client.device_id() || batch.after_cursor != self.cursor {
            return Err(CoreError::InvalidSync);
        }
        let storage_backup = self
            .client
            .mls()
            .provider()
            .storage()
            .values
            .read()
            .map_err(|_| CoreError::Provider)?
            .clone();
        let mut rendered = Vec::new();
        for item in &batch.items {
            let Some(entry) = item.entry.as_ref() else { return Err(CoreError::InvalidSync) };
            if let v1::queue_item::Entry::Envelope(envelope) = entry {
                match self.client.open_envelope_authenticated(envelope, now_ms()) {
                    Ok(message) => {
                        let sender_user_id = message.sender_user_id;
                        let message = message.message;
                        if let Some(v1::message::Content::Text(text)) = message.content {
                            rendered.push((
                                message.conversation_id,
                                sender_user_id,
                                message.sender_device_id,
                                text,
                                message.sequence_id,
                                message.sent_at_ms,
                            ));
                        }
                    }
                    Err(CoreError::Authentication) => {
                        {
                            let storage = self.client.mls_mut().provider_mut().storage();
                            let mut values = storage.values.write().map_err(|_| CoreError::Provider)?;
                            *values = storage_backup;
                        }
                        if self.discard_next_batch {
                            self.discard_next_batch = batch.next_cursor < batch.high_watermark;
                            self.cursor = batch.next_cursor;
                            self.save()?;
                            let ack = self.encode_client_frame(v1::client_frame::Body::Ack(
                                v1::QueueAck {
                                    through_cursor: self.cursor,
                                },
                            ))?;
                            self.send_frame(&ack)?;
                            if self.cursor < batch.high_watermark {
                                let replay = self.encode_client_frame(
                                    v1::client_frame::Body::Replay(v1::Replay {
                                        after_cursor: self.cursor,
                                        limit: protocol::MAX_BATCH_ITEMS as u32,
                                    }),
                                )?;
                                self.send_frame(&replay)?;
                            }
                            return Ok(());
                        }
                        self.pending_batch = Some(batch);
                        return Ok(())
                    }
                    Err(error) => return Err(error),
                }
            }
        }
        self.cursor = batch.next_cursor;
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
            let verifier = CallbackVerifier { bindings: Arc::clone(&bindings) };
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
