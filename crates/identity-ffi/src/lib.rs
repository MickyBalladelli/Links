//! Synchronous native vault bridge. No seeds or retained pointers cross the public
//! create/sign/delete API. Callbacks alone see temporary, zeroized seed buffers.
//! The C contract is native/apple/Sources/CLinksIdentity/links_identity.h.
#![deny(unsafe_op_in_unsafe_fn)]

use links_client_core::{
    identity::{HardwareIdentityStore, HardwareSeedVault, IdentityStore, KeyHandle},
    passkey_backup::{PasskeyBackupEnvelope, PasskeyBackupSalt},
    CoreError,
};
use std::{ffi::c_void, panic::AssertUnwindSafe, slice, str};
use uuid::Uuid;
use zeroize::Zeroizing;

pub const OK: i32 = 0;
pub const INVALID: i32 = 1;
pub const UNAVAILABLE: i32 = 2;
pub const AUTHENTICATION: i32 = 3;
pub const PROVIDER: i32 = 4;
pub const MAX_MESSAGE: usize = 1024 * 1024;
pub const MAX_TRANSCRIPT: usize = 1024;
pub const MAX_RECOVERY_PHRASE: usize = 512;
pub const MAX_RECOVERY_PASSPHRASE: usize = 256;
pub const MAX_CREDENTIAL_ID: usize = 1024;
pub const MAX_BACKUP_ENVELOPE: usize = 1152;
const HANDLE_LEN: usize = 36;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct VaultCallbacks {
    pub abi_version: u32,
    pub context: *mut c_void,
    pub store: Option<unsafe extern "C" fn(*mut c_void, *const u8, *mut u8) -> i32>,
    pub load: Option<unsafe extern "C" fn(*mut c_void, *const u8, *mut u8) -> i32>,
    pub delete: Option<unsafe extern "C" fn(*mut c_void, *const u8) -> i32>,
}

struct NativeVault(VaultCallbacks);
fn callback_result(status: i32) -> Result<(), CoreError> {
    match status {
        OK => Ok(()),
        UNAVAILABLE => Err(CoreError::CryptoUnavailable),
        AUTHENTICATION => Err(CoreError::Authentication),
        _ => Err(CoreError::Provider),
    }
}
fn valid_handle(bytes: &[u8]) -> bool {
    bytes.len() == HANDLE_LEN
        && bytes.iter().enumerate().all(|(i, b)| {
            if [8, 13, 18, 23].contains(&i) {
                *b == b'-'
            } else {
                b.is_ascii_digit() || (b'a'..=b'f').contains(b)
            }
        })
}
impl HardwareSeedVault for NativeVault {
    fn store_seed(&mut self, seed: &[u8; 32]) -> Result<KeyHandle, CoreError> {
        let mut handle = [0; HANDLE_LEN];
        // SAFETY: callbacks are validated at entry; both buffers live for the call.
        callback_result(unsafe {
            (self.0.store.unwrap())(self.0.context, seed.as_ptr(), handle.as_mut_ptr())
        })?;
        if !valid_handle(&handle) {
            return Err(CoreError::Provider);
        }
        KeyHandle::new(handle.to_vec())
    }
    fn load_seed(&self, handle: &KeyHandle) -> Result<Zeroizing<[u8; 32]>, CoreError> {
        let mut seed = Zeroizing::new([0; 32]);
        // SAFETY: handles are canonical and buffer is writable for 32 bytes.
        callback_result(unsafe {
            (self.0.load.unwrap())(
                self.0.context,
                handle.as_bytes().as_ptr(),
                seed.as_mut_ptr(),
            )
        })?;
        Ok(seed)
    }
    fn delete_seed(&mut self, handle: &KeyHandle) -> Result<(), CoreError> {
        // SAFETY: validated callback and canonical 36-byte handle.
        callback_result(unsafe {
            (self.0.delete.unwrap())(self.0.context, handle.as_bytes().as_ptr())
        })
    }
}

unsafe fn store(
    callbacks: *const VaultCallbacks,
) -> Result<HardwareIdentityStore<NativeVault>, i32> {
    if callbacks.is_null() {
        return Err(INVALID);
    }
    // SAFETY: caller supplies a live, aligned callback table for this operation.
    let callbacks = unsafe { *callbacks };
    if callbacks.abi_version != 1
        || callbacks.store.is_none()
        || callbacks.load.is_none()
        || callbacks.delete.is_none()
    {
        return Err(INVALID);
    }
    Ok(HardwareIdentityStore::new(NativeVault(callbacks)))
}
unsafe fn handle(bytes: *const u8) -> Result<KeyHandle, i32> {
    if bytes.is_null() {
        return Err(INVALID);
    }
    // SAFETY: caller guarantees 36 readable bytes, without a NUL terminator.
    let bytes = unsafe { slice::from_raw_parts(bytes, HANDLE_LEN) };
    if !valid_handle(bytes) {
        return Err(INVALID);
    }
    KeyHandle::new(bytes.to_vec()).map_err(|_| INVALID)
}
fn status(error: CoreError) -> i32 {
    match error {
        CoreError::Authentication => AUTHENTICATION,
        CoreError::CryptoUnavailable => UNAVAILABLE,
        _ => PROVIDER,
    }
}
fn boundary(work: impl FnOnce() -> Result<(), i32>) -> i32 {
    // Never unwind a Rust panic across C/Swift/JNI. Foreign callbacks must catch
    // their own exceptions; abort/OOM cannot be recovered by this boundary.
    match std::panic::catch_unwind(AssertUnwindSafe(work)) {
        Ok(Ok(())) => OK,
        Ok(Err(code)) => code,
        Err(_) => PROVIDER,
    }
}

unsafe fn read_fixed<const N: usize>(input: *const u8) -> Result<[u8; N], i32> {
    if input.is_null() {
        return Err(INVALID);
    }
    let mut output = [0; N];
    // SAFETY: callers of the exported functions provide N readable bytes.
    output.copy_from_slice(unsafe { slice::from_raw_parts(input, N) });
    Ok(output)
}

unsafe fn read_bytes<'a>(input: *const u8, length: usize) -> Result<&'a [u8], i32> {
    read_bytes_limited(input, length, MAX_TRANSCRIPT, false)
}

unsafe fn read_bytes_limited<'a>(
    input: *const u8,
    length: usize,
    maximum: usize,
    allow_empty_null: bool,
) -> Result<&'a [u8], i32> {
    if length > maximum || (input.is_null() && !(allow_empty_null && length == 0)) {
        return Err(INVALID);
    }
    // SAFETY: callers provide a readable buffer of the declared length.
    if length == 0 {
        return Ok(&[]);
    }
    Ok(unsafe { slice::from_raw_parts(input, length) })
}

unsafe fn write_transcript(
    transcript: Result<Vec<u8>, links_identity::IdentityError>,
    output: *mut u8,
    output_capacity: usize,
    output_length: *mut usize,
) -> Result<(), i32> {
    if output.is_null() || output_length.is_null() {
        return Err(INVALID);
    }
    let transcript = transcript.map_err(|_| INVALID)?;
    if transcript.is_empty()
        || transcript.len() > output_capacity
        || transcript.len() > MAX_TRANSCRIPT
    {
        return Err(INVALID);
    }
    // SAFETY: output capacity was checked against the generated transcript.
    unsafe {
        output.copy_from_nonoverlapping(transcript.as_ptr(), transcript.len());
        output_length.write(transcript.len());
    }
    Ok(())
}

/// Build the exact phone proof transcript used by AccountAuth.
///
/// # Safety
/// All input pointers reference their declared readable lengths. The output
/// buffer is writable for `output_capacity` bytes and `output_length` is live.
#[no_mangle]
pub unsafe extern "C" fn links_phone_auth_transcript(
    phone: *const u8,
    phone_len: usize,
    channel: *const u8,
    channel_len: usize,
    device_id: *const u8,
    mls_node_id: *const u8,
    public_key: *const u8,
    output: *mut u8,
    output_capacity: usize,
    output_length: *mut usize,
) -> i32 {
    if output_length.is_null() {
        return INVALID;
    }
    unsafe { output_length.write(0) };
    boundary(|| {
        let phone = unsafe { read_bytes(phone, phone_len)? };
        let channel = unsafe { read_bytes(channel, channel_len)? };
        let phone = str::from_utf8(phone).map_err(|_| INVALID)?;
        let channel = str::from_utf8(channel).map_err(|_| INVALID)?;
        let device_id = Uuid::from_bytes(unsafe { read_fixed(device_id)? });
        let mls_node_id = Uuid::from_bytes(unsafe { read_fixed(mls_node_id)? });
        let public_key = unsafe { read_fixed(public_key)? };
        unsafe {
            write_transcript(
                links_identity::phone_auth_transcript(
                    phone,
                    channel,
                    device_id,
                    mls_node_id,
                    &public_key,
                ),
                output,
                output_capacity,
                output_length,
            )
        }
    })
}

/// Build the nonce-bound enrollment transcript used by AccountAuth.finish.
///
/// # Safety
/// All fixed-size input pointers reference readable buffers. The credential
/// pointer references `credential_len` readable bytes. Output follows the
/// `links_phone_auth_transcript` contract.
#[no_mangle]
pub unsafe extern "C" fn links_enrollment_transcript(
    user_id: *const u8,
    device_id: *const u8,
    mls_node_id: *const u8,
    public_key: *const u8,
    challenge_id: *const u8,
    nonce: *const u8,
    expires_at_ms: u64,
    mls_credential: *const u8,
    credential_len: usize,
    output: *mut u8,
    output_capacity: usize,
    output_length: *mut usize,
) -> i32 {
    if output_length.is_null() {
        return INVALID;
    }
    unsafe { output_length.write(0) };
    boundary(|| {
        let binding = links_identity::DeviceBinding {
            user_id: Uuid::from_bytes(unsafe { read_fixed(user_id)? }),
            device_id: Uuid::from_bytes(unsafe { read_fixed(device_id)? }),
            mls_node_id: Uuid::from_bytes(unsafe { read_fixed(mls_node_id)? }),
            public_key: unsafe { read_fixed(public_key)? },
        };
        let challenge_id = Uuid::from_bytes(unsafe { read_fixed(challenge_id)? });
        let nonce = unsafe { read_fixed(nonce)? };
        let mls_credential = unsafe { read_bytes(mls_credential, credential_len)? };
        let expected_credential = binding.mls_credential().map_err(|_| INVALID)?;
        if mls_credential != expected_credential.as_slice() {
            return Err(INVALID);
        }
        unsafe {
            write_transcript(
                binding.enrollment_transcript(challenge_id, &nonce, expires_at_ms),
                output,
                output_capacity,
                output_length,
            )
        }
    })
}

/// Create and read back a hardware-wrapped identity before returning its reference.
/// # Safety
/// See the C header. Outputs are distinct writable buffers (36 and 32 bytes),
/// disjoint from inputs; callbacks and their context remain live until return.
#[no_mangle]
pub unsafe extern "C" fn links_identity_create(
    callbacks: *const VaultCallbacks,
    out_handle: *mut u8,
    out_public_key: *mut u8,
) -> i32 {
    if out_handle.is_null() || out_public_key.is_null() {
        return INVALID;
    }
    // SAFETY: caller guarantees valid, disjoint output buffers.
    unsafe {
        out_handle.write_bytes(0, HANDLE_LEN);
        out_public_key.write_bytes(0, 32);
    }
    boundary(|| {
        let mut store = unsafe { store(callbacks)? };
        let key = store.create_signing_key().map_err(status)?;
        let public_key = match store.public_key(&key) {
            Ok(public_key) => public_key,
            Err(error) => {
                // Creation failed: do not return an unusable identity. Native
                // cleanup is best effort; crash/orphan cleanup is a separate gate.
                let _ = store.delete_key(key);
                return Err(status(error));
            }
        };
        unsafe {
            out_handle.copy_from_nonoverlapping(key.as_bytes().as_ptr(), HANDLE_LEN);
            out_public_key.copy_from_nonoverlapping(public_key.as_ptr(), 32);
        }
        Ok(())
    })
}

/// Derive an identity from an explicit local BIP-39 recovery phrase and seal it
/// in the native hardware vault. The phrase and passphrase are never retained.
/// # Safety
/// Input buffers are readable for their declared lengths. Output buffers are
/// distinct writable buffers of 36 and 32 bytes.
#[no_mangle]
pub unsafe extern "C" fn links_identity_restore_from_mnemonic(
    callbacks: *const VaultCallbacks,
    phrase: *const u8,
    phrase_len: usize,
    passphrase: *const u8,
    passphrase_len: usize,
    out_handle: *mut u8,
    out_public_key: *mut u8,
) -> i32 {
    if out_handle.is_null() || out_public_key.is_null() {
        return INVALID;
    }
    unsafe {
        out_handle.write_bytes(0, HANDLE_LEN);
        out_public_key.write_bytes(0, 32);
    }
    boundary(|| {
        let phrase = unsafe {
            read_bytes_limited(phrase, phrase_len, MAX_RECOVERY_PHRASE, false)?
        };
        let passphrase = unsafe {
            read_bytes_limited(
                passphrase,
                passphrase_len,
                MAX_RECOVERY_PASSPHRASE,
                true,
            )?
        };
        let phrase = str::from_utf8(phrase).map_err(|_| INVALID)?;
        let passphrase = str::from_utf8(passphrase).map_err(|_| INVALID)?;
        let mnemonic = links_identity::RecoveryMnemonic::from_phrase(phrase)
            .map_err(|_| AUTHENTICATION)?;
        let mut store = unsafe { store(callbacks)? };
        let key = store
            .restore_from_recovery(&mnemonic, passphrase)
            .map_err(status)?;
        let public_key = match store.public_key(&key) {
            Ok(public_key) => public_key,
            Err(error) => {
                let _ = store.delete_key(key);
                return Err(status(error));
            }
        };
        unsafe {
            out_handle.copy_from_nonoverlapping(key.as_bytes().as_ptr(), HANDLE_LEN);
            out_public_key.copy_from_nonoverlapping(public_key.as_ptr(), 32);
        }
        Ok(())
    })
}

/// Seal the vault identity with a locally evaluated WebAuthn PRF result.
/// Only the opaque envelope leaves the native boundary.
/// # Safety
/// Fixed inputs and output pointers follow the sizes in the C header. The
/// credential and PRF buffers are readable for their declared lengths.
#[no_mangle]
pub unsafe extern "C" fn links_identity_backup_with_passkey(
    callbacks: *const VaultCallbacks,
    key_handle: *const u8,
    backup_id: *const u8,
    device_id: *const u8,
    credential_id: *const u8,
    credential_id_len: usize,
    salt: *const u8,
    prf_output: *const u8,
    output: *mut u8,
    output_capacity: usize,
    output_length: *mut usize,
) -> i32 {
    if output_length.is_null() {
        return INVALID;
    }
    unsafe { output_length.write(0) };
    boundary(|| {
        if output.is_null() {
            return Err(INVALID);
        }
        let key = unsafe { handle(key_handle)? };
        let backup_id = Uuid::from_bytes(unsafe { read_fixed(backup_id)? });
        let device_id = Uuid::from_bytes(unsafe { read_fixed(device_id)? });
        let credential_id = unsafe {
            read_bytes_limited(credential_id, credential_id_len, MAX_CREDENTIAL_ID, false)?
        };
        let salt = unsafe { read_fixed::<32>(salt)? };
        let prf_output = unsafe { read_fixed::<32>(prf_output)? };
        let store = unsafe { store(callbacks)? };
        let envelope = store
            .backup_with_passkey(
                &key,
                backup_id,
                device_id,
                credential_id,
                PasskeyBackupSalt::from_bytes(salt),
                &prf_output,
            )
            .map_err(status)?;
        if envelope.as_bytes().len() > MAX_BACKUP_ENVELOPE
            || envelope.as_bytes().len() > output_capacity
        {
            return Err(INVALID);
        }
        unsafe {
            output.copy_from_nonoverlapping(envelope.as_bytes().as_ptr(), envelope.as_bytes().len());
            output_length.write(envelope.as_bytes().len());
        }
        Ok(())
    })
}

/// Open an opaque passkey envelope after local WebAuthn PRF evaluation and
/// immediately reseal the recovered identity in the native hardware vault.
/// # Safety
/// Fixed inputs and output pointers follow the sizes in the C header. The
/// envelope, credential and PRF buffers are readable for their declared sizes.
#[no_mangle]
pub unsafe extern "C" fn links_identity_restore_from_passkey(
    callbacks: *const VaultCallbacks,
    backup_id: *const u8,
    device_id: *const u8,
    credential_id: *const u8,
    credential_id_len: usize,
    envelope: *const u8,
    envelope_len: usize,
    prf_output: *const u8,
    out_handle: *mut u8,
    out_public_key: *mut u8,
) -> i32 {
    if out_handle.is_null() || out_public_key.is_null() {
        return INVALID;
    }
    unsafe {
        out_handle.write_bytes(0, HANDLE_LEN);
        out_public_key.write_bytes(0, 32);
    }
    boundary(|| {
        let backup_id = Uuid::from_bytes(unsafe { read_fixed(backup_id)? });
        let device_id = Uuid::from_bytes(unsafe { read_fixed(device_id)? });
        let credential_id = unsafe {
            read_bytes_limited(credential_id, credential_id_len, MAX_CREDENTIAL_ID, false)?
        };
        let envelope = unsafe {
            read_bytes_limited(envelope, envelope_len, MAX_BACKUP_ENVELOPE, false)?
        };
        let prf_output = unsafe { read_fixed::<32>(prf_output)? };
        let envelope = PasskeyBackupEnvelope::try_from(envelope.to_vec()).map_err(status)?;
        let mut store = unsafe { store(callbacks)? };
        let key = store
            .restore_from_passkey(
                &envelope,
                backup_id,
                device_id,
                credential_id,
                &prf_output,
            )
            .map_err(status)?;
        let public_key = match store.public_key(&key) {
            Ok(public_key) => public_key,
            Err(error) => {
                let _ = store.delete_key(key);
                return Err(status(error));
            }
        };
        unsafe {
            out_handle.copy_from_nonoverlapping(key.as_bytes().as_ptr(), HANDLE_LEN);
            out_public_key.copy_from_nonoverlapping(public_key.as_ptr(), 32);
        }
        Ok(())
    })
}

/// Resolve an existing identity; missing keys never cause key generation.
/// # Safety
/// All pointers obey the C header sizes/lifetimes, including 36-byte handle and
/// disjoint 32-byte output. No callback may retain a pointer or throw through C.
#[no_mangle]
pub unsafe extern "C" fn links_identity_public_key(
    callbacks: *const VaultCallbacks,
    key_handle: *const u8,
    out_public_key: *mut u8,
) -> i32 {
    if out_public_key.is_null() {
        return INVALID;
    }
    unsafe { out_public_key.write_bytes(0, 32) };
    boundary(|| {
        let store = unsafe { store(callbacks)? };
        let key = unsafe { handle(key_handle)? };
        let public_key = store.public_key(&key).map_err(status)?;
        unsafe { out_public_key.copy_from_nonoverlapping(public_key.as_ptr(), 32) };
        Ok(())
    })
}

/// Sign with the enrolled public key, unwrapping the seed only for this call.
/// # Safety
/// Inputs are readable for their declared sizes (handle 36, expected key 32,
/// message message_len); output is a disjoint writable 64-byte buffer. Null
/// message is allowed only for length zero. Callback pointers follow the header.
#[no_mangle]
pub unsafe extern "C" fn links_identity_sign(
    callbacks: *const VaultCallbacks,
    key_handle: *const u8,
    expected_public_key: *const u8,
    message: *const u8,
    message_len: usize,
    out_signature: *mut u8,
) -> i32 {
    if out_signature.is_null() {
        return INVALID;
    }
    unsafe { out_signature.write_bytes(0, 64) };
    boundary(|| {
        if expected_public_key.is_null()
            || message_len > MAX_MESSAGE
            || (message_len != 0 && message.is_null())
        {
            return Err(INVALID);
        }
        let store = unsafe { store(callbacks)? };
        let key = unsafe { handle(key_handle)? };
        let expected = unsafe { &*expected_public_key.cast::<[u8; 32]>() };
        let message = if message_len == 0 {
            &[]
        } else {
            unsafe { slice::from_raw_parts(message, message_len) }
        };
        let signature = store
            .sign_checked(&key, expected, message)
            .map_err(status)?;
        unsafe { out_signature.copy_from_nonoverlapping(signature.as_ptr(), 64) };
        Ok(())
    })
}

/// Delete the wrapping key and encrypted record; repeated deletion is harmless.
/// # Safety
/// Callback table and 36-byte handle obey the C header pointer contract.
#[no_mangle]
pub unsafe extern "C" fn links_identity_delete(
    callbacks: *const VaultCallbacks,
    key_handle: *const u8,
) -> i32 {
    boundary(|| {
        let mut store = unsafe { store(callbacks)? };
        let key = unsafe { handle(key_handle)? };
        store.delete_key(key).map_err(status)
    })
}

#[cfg(test)]
mod tests;
