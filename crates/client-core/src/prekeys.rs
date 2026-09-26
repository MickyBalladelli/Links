//! Client-side PQXDH prekey provisioning and automatic inventory refill.

use crate::{pqxdh, protocol, protocol::v1, CoreError};
use async_trait::async_trait;
use std::collections::HashSet;
use zeroize::Zeroizing;

pub const DEFAULT_ONE_TIME_PREKEY_TARGET: u32 = 100;
pub const DEFAULT_ONE_TIME_PREKEY_LOW_WATERMARK: u32 = 20;

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub enum SecretKind {
    IdentityDh,
    SignedCurve,
    OneTimeCurve,
    LastResortKem,
    OneTimeKem,
}

/// Native implementations synchronously wrap these seeds with the platform
/// hardware vault. They must not retain the borrowed plaintext buffer.
pub trait PreKeySecretStore {
    fn store_x25519(&mut self, kind: SecretKind, id: u64, seed: &[u8; 32])
        -> Result<(), CoreError>;
    fn store_ml_kem_768(
        &mut self,
        kind: SecretKind,
        id: u64,
        seed: &[u8; 64],
    ) -> Result<(), CoreError>;
    fn delete(&mut self, kind: SecretKind, id: u64) -> Result<(), CoreError>;
}

/// Adapter over the enrolled hardware-backed Ed25519 identity key.
pub trait PreKeySigner {
    fn public_key(&self) -> Result<[u8; 32], CoreError>;
    fn sign(&self, transcript: &[u8]) -> Result<[u8; 64], CoreError>;
}

#[derive(Clone, PartialEq, Eq)]
pub struct LocalPreKeyProfile {
    pub device_id: String,
    pub revision: u64,
    pub profile: v1::PreKeyProfile,
}

/// Create a new X25519 identity, signed X25519 prekey and last-resort
/// ML-KEM-768 key. Rotation uses a strictly increasing revision.
pub fn generate_profile<S: PreKeySigner, V: PreKeySecretStore>(
    device_id: String,
    revision: u64,
    signer: &S,
    vault: &mut V,
) -> Result<LocalPreKeyProfile, CoreError> {
    protocol::validate_id(&device_id)?;
    if revision == 0 || revision > protocol::MAX_CURSOR {
        return Err(CoreError::Authentication);
    }
    let signing_key = signer.public_key()?;
    links_identity::validate_public_key(&signing_key).map_err(|_| CoreError::Authentication)?;
    let mut stored = Vec::new();

    let identity_seed = pqxdh::generate_x25519_seed()?;
    let identity = pqxdh::IdentityPrivateKey::from_seed(Zeroizing::new(*identity_seed));
    vault.store_x25519(SecretKind::IdentityDh, revision, &identity_seed)?;
    stored.push((SecretKind::IdentityDh, revision));
    let dh_key = identity.public_key();
    let binding_signature = signer.sign(&pqxdh::identity_binding_transcript(&dh_key));
    let binding_signature = match binding_signature {
        Ok(signature) => signature,
        Err(error) => return cleanup(vault, &stored, error),
    };

    let signed_id = match unique_id(&HashSet::new()) {
        Ok(id) => id,
        Err(error) => return cleanup(vault, &stored, error),
    };
    let signed_seed = match pqxdh::generate_x25519_seed() {
        Ok(seed) => seed,
        Err(error) => return cleanup(vault, &stored, error),
    };
    let signed_private =
        match pqxdh::CurvePreKey::from_seed(signed_id, Zeroizing::new(*signed_seed)) {
            Ok(key) => key,
            Err(error) => return cleanup(vault, &stored, error),
        };
    if let Err(error) = vault.store_x25519(SecretKind::SignedCurve, signed_id, &signed_seed) {
        return cleanup(vault, &stored, error);
    }
    stored.push((SecretKind::SignedCurve, signed_id));
    let signed_public = signed_private.public();
    let signed_signature = signer.sign(&pqxdh::signed_prekey_transcript(&dh_key, &signed_public));
    let signed_signature = match signed_signature {
        Ok(signature) => signature,
        Err(error) => return cleanup(vault, &stored, error),
    };

    let mut used = HashSet::new();
    used.insert(signed_id);
    let kem_id = match unique_id(&used) {
        Ok(id) => id,
        Err(error) => return cleanup(vault, &stored, error),
    };
    let kem_seed = match pqxdh::generate_ml_kem_768_seed() {
        Ok(seed) => seed,
        Err(error) => return cleanup(vault, &stored, error),
    };
    let kem_private = match pqxdh::KemPreKey::from_seed(kem_id, false, Zeroizing::new(*kem_seed)) {
        Ok(key) => key,
        Err(error) => return cleanup(vault, &stored, error),
    };
    if let Err(error) = vault.store_ml_kem_768(SecretKind::LastResortKem, kem_id, &kem_seed) {
        return cleanup(vault, &stored, error);
    }
    stored.push((SecretKind::LastResortKem, kem_id));
    let kem_public = kem_private.public();
    let kem_signature = signer.sign(&pqxdh::kem_prekey_transcript(&dh_key, &kem_public));
    let kem_signature = match kem_signature {
        Ok(signature) => signature,
        Err(error) => return cleanup(vault, &stored, error),
    };

    let profile = v1::PreKeyProfile {
        identity: Some(v1::PqxdhPublicIdentity {
            signing_key: signing_key.to_vec(),
            dh_key: dh_key.to_vec(),
            binding_signature: binding_signature.to_vec(),
        }),
        signed_prekey: Some(v1::SignedCurvePreKey {
            prekey: Some(curve_to_wire(&signed_public)),
            signature: signed_signature.to_vec(),
        }),
        last_resort_kem_prekey: Some(kem_to_wire(&kem_public, kem_signature)),
    };
    let empty = v1::PreKeyUpload {
        protocol_version: protocol::VERSION,
        device_id: device_id.clone(),
        profile_revision: revision,
        profile: Some(profile.clone()),
        one_time_curve_prekeys: vec![],
        one_time_kem_prekeys: vec![],
        upload_id: "00000000-0000-4000-8000-000000000001".into(),
    };
    if let Err(error) = protocol::validate_prekey_upload(&empty) {
        return cleanup(vault, &stored, error.into());
    }
    Ok(LocalPreKeyProfile {
        device_id,
        revision,
        profile,
    })
}

/// Generate and hardware-wrap a refill batch. Keep the returned protobuf bytes
/// durably until an idempotent upload succeeds.
pub fn generate_upload<S: PreKeySigner, V: PreKeySecretStore>(
    profile: &LocalPreKeyProfile,
    curve_count: u32,
    kem_count: u32,
    signer: &S,
    vault: &mut V,
) -> Result<v1::PreKeyUpload, CoreError> {
    if curve_count as usize > protocol::MAX_ONE_TIME_PREKEYS
        || kem_count as usize > protocol::MAX_ONE_TIME_PREKEYS
    {
        return Err(CoreError::Authentication);
    }
    let upload_id = random_uuid()?;
    let identity = profile
        .profile
        .identity
        .as_ref()
        .ok_or(CoreError::Authentication)?;
    let signing_key: [u8; 32] = identity
        .signing_key
        .as_slice()
        .try_into()
        .map_err(|_| CoreError::Authentication)?;
    let dh_key: [u8; 32] = identity
        .dh_key
        .as_slice()
        .try_into()
        .map_err(|_| CoreError::Authentication)?;
    if signer.public_key()? != signing_key {
        return Err(CoreError::Authentication);
    }

    let mut used = reserved_ids(&profile.profile)?;
    let mut stored = Vec::new();
    let mut curve = Vec::with_capacity(curve_count as usize);
    let mut kem = Vec::with_capacity(kem_count as usize);
    for _ in 0..curve_count {
        let id = match unique_id(&used) {
            Ok(id) => id,
            Err(error) => return cleanup(vault, &stored, error),
        };
        used.insert(id);
        let seed = match pqxdh::generate_x25519_seed() {
            Ok(seed) => seed,
            Err(error) => return cleanup(vault, &stored, error),
        };
        let private = match pqxdh::CurvePreKey::from_seed(id, Zeroizing::new(*seed)) {
            Ok(key) => key,
            Err(error) => return cleanup(vault, &stored, error),
        };
        if let Err(error) = vault.store_x25519(SecretKind::OneTimeCurve, id, &seed) {
            return cleanup(vault, &stored, error);
        }
        stored.push((SecretKind::OneTimeCurve, id));
        curve.push(curve_to_wire(&private.public()));
    }
    for _ in 0..kem_count {
        let id = match unique_id(&used) {
            Ok(id) => id,
            Err(error) => return cleanup(vault, &stored, error),
        };
        used.insert(id);
        let seed = match pqxdh::generate_ml_kem_768_seed() {
            Ok(seed) => seed,
            Err(error) => return cleanup(vault, &stored, error),
        };
        let private = match pqxdh::KemPreKey::from_seed(id, true, Zeroizing::new(*seed)) {
            Ok(key) => key,
            Err(error) => return cleanup(vault, &stored, error),
        };
        if let Err(error) = vault.store_ml_kem_768(SecretKind::OneTimeKem, id, &seed) {
            return cleanup(vault, &stored, error);
        }
        stored.push((SecretKind::OneTimeKem, id));
        let public = private.public();
        let signature = match signer.sign(&pqxdh::kem_prekey_transcript(&dh_key, &public)) {
            Ok(signature) => signature,
            Err(error) => return cleanup(vault, &stored, error),
        };
        kem.push(kem_to_wire(&public, signature));
    }
    let upload = v1::PreKeyUpload {
        protocol_version: protocol::VERSION,
        device_id: profile.device_id.clone(),
        profile_revision: profile.revision,
        profile: Some(profile.profile.clone()),
        one_time_curve_prekeys: curve,
        one_time_kem_prekeys: kem,
        upload_id,
    };
    if let Err(error) = protocol::validate_prekey_upload(&upload) {
        return cleanup(vault, &stored, error.into());
    }
    Ok(upload)
}

#[async_trait]
pub trait PreKeyApi: Send + Sync {
    async fn inventory(&self) -> Result<v1::PreKeyInventory, CoreError>;
    async fn upload(&self, upload: &v1::PreKeyUpload) -> Result<v1::PreKeyInventory, CoreError>;
}

/// Durable local retry slot. Save exact upload bytes before network transmission.
pub trait PendingPreKeyUploadStore {
    fn load(&self) -> Result<Option<v1::PreKeyUpload>, CoreError>;
    fn save(&mut self, upload: &v1::PreKeyUpload) -> Result<(), CoreError>;
    fn clear(&mut self, upload_id: &str) -> Result<(), CoreError>;
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum RefillOutcome {
    Unchanged,
    Uploaded {
        curve_prekeys: u32,
        kem_prekeys: u32,
    },
}

/// Retry a pending upload, then refill both pools whenever either reaches the
/// low watermark. Call after login and after receiving low-inventory hints.
pub async fn maintain_inventory<A, S, V, P>(
    api: &A,
    signer: &S,
    vault: &mut V,
    pending: &mut P,
    profile: &LocalPreKeyProfile,
    low_watermark: u32,
    target: u32,
) -> Result<RefillOutcome, CoreError>
where
    A: PreKeyApi,
    S: PreKeySigner,
    V: PreKeySecretStore,
    P: PendingPreKeyUploadStore,
{
    if low_watermark >= target || target as usize > protocol::MAX_ONE_TIME_PREKEYS {
        return Err(CoreError::Authentication);
    }
    if let Some(upload) = pending.load()? {
        protocol::validate_prekey_upload(&upload)?;
        if upload.device_id != profile.device_id || upload.profile_revision != profile.revision {
            return Err(CoreError::Authentication);
        }
        let curve = upload.one_time_curve_prekeys.len() as u32;
        let kem = upload.one_time_kem_prekeys.len() as u32;
        let inventory = api.upload(&upload).await?;
        validate_inventory(&inventory, &profile.device_id)?;
        if inventory.profile_revision != profile.revision {
            return Err(CoreError::Authentication);
        }
        pending.clear(&upload.upload_id)?;
        return Ok(RefillOutcome::Uploaded {
            curve_prekeys: curve,
            kem_prekeys: kem,
        });
    }
    let inventory = api.inventory().await?;
    validate_inventory(&inventory, &profile.device_id)?;
    if inventory.profile_revision > profile.revision {
        return Err(CoreError::Authentication);
    }
    if inventory.profile_revision == profile.revision
        && inventory.one_time_curve_prekeys > low_watermark
        && inventory.one_time_kem_prekeys > low_watermark
    {
        return Ok(RefillOutcome::Unchanged);
    }
    let curve_count = target.saturating_sub(inventory.one_time_curve_prekeys);
    let kem_count = target.saturating_sub(inventory.one_time_kem_prekeys);
    let upload = generate_upload(profile, curve_count, kem_count, signer, vault)?;
    pending.save(&upload)?;
    let result = api.upload(&upload).await?;
    validate_inventory(&result, &profile.device_id)?;
    if result.profile_revision != profile.revision {
        return Err(CoreError::Authentication);
    }
    pending.clear(&upload.upload_id)?;
    Ok(RefillOutcome::Uploaded {
        curve_prekeys: curve_count,
        kem_prekeys: kem_count,
    })
}

/// Convert a server-claimed wire bundle into the verified PQXDH input.
pub fn claimed_bundle(
    bundle: &v1::PreKeyBundle,
    expected_signing_key: &[u8; 32],
) -> Result<pqxdh::PreKeyBundle, CoreError> {
    protocol::validate_prekey_bundle(bundle)?;
    let profile = bundle.profile.as_ref().ok_or(CoreError::Authentication)?;
    let identity = profile.identity.as_ref().ok_or(CoreError::Authentication)?;
    let signing_key = fixed(&identity.signing_key)?;
    let dh_key = fixed(&identity.dh_key)?;
    let public_identity =
        pqxdh::PublicIdentity::new(signing_key, dh_key, fixed(&identity.binding_signature)?)?;
    let signed = profile
        .signed_prekey
        .as_ref()
        .ok_or(CoreError::Authentication)?;
    let signed_key = curve_from_wire(signed.prekey.as_ref().ok_or(CoreError::Authentication)?)?;
    let kem = bundle
        .kem_prekey
        .as_ref()
        .ok_or(CoreError::Authentication)?;
    let result = pqxdh::PreKeyBundle {
        identity: public_identity,
        signed_prekey: signed_key,
        signed_prekey_signature: fixed(&signed.signature)?,
        one_time_prekey: bundle
            .one_time_curve_prekey
            .as_ref()
            .map(curve_from_wire)
            .transpose()?,
        kem_prekey: pqxdh::PublicKemPreKey {
            id: kem.id,
            key: kem.public_key.clone(),
            one_time: kem.one_time,
        },
        kem_prekey_signature: fixed(&kem.signature)?,
    };
    result.verify_for(expected_signing_key)?;
    Ok(result)
}

fn validate_inventory(inventory: &v1::PreKeyInventory, device_id: &str) -> Result<(), CoreError> {
    if inventory.protocol_version != protocol::VERSION
        || inventory.device_id != device_id
        || inventory.profile_revision > protocol::MAX_CURSOR
        || inventory.one_time_curve_prekeys as usize > protocol::MAX_ONE_TIME_PREKEYS
        || inventory.one_time_kem_prekeys as usize > protocol::MAX_ONE_TIME_PREKEYS
        || (inventory.profile_revision == 0
            && (inventory.one_time_curve_prekeys != 0 || inventory.one_time_kem_prekeys != 0))
    {
        return Err(CoreError::Authentication);
    }
    Ok(())
}

fn reserved_ids(profile: &v1::PreKeyProfile) -> Result<HashSet<u64>, CoreError> {
    let mut ids = HashSet::new();
    ids.insert(
        profile
            .signed_prekey
            .as_ref()
            .and_then(|key| key.prekey.as_ref())
            .map(|key| key.id)
            .ok_or(CoreError::Authentication)?,
    );
    ids.insert(
        profile
            .last_resort_kem_prekey
            .as_ref()
            .map(|key| key.id)
            .ok_or(CoreError::Authentication)?,
    );
    Ok(ids)
}

fn unique_id(used: &HashSet<u64>) -> Result<u64, CoreError> {
    for _ in 0..16 {
        let mut bytes = [0; 8];
        getrandom::fill(&mut bytes).map_err(|_| CoreError::Provider)?;
        let id = u64::from_be_bytes(bytes) & protocol::MAX_CURSOR;
        if id != 0 && !used.contains(&id) {
            return Ok(id);
        }
    }
    Err(CoreError::Provider)
}

fn random_uuid() -> Result<String, CoreError> {
    let mut bytes = [0; 16];
    getrandom::fill(&mut bytes).map_err(|_| CoreError::Provider)?;
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Ok(uuid::Uuid::from_bytes(bytes).to_string())
}

fn cleanup<T, V: PreKeySecretStore>(
    vault: &mut V,
    stored: &[(SecretKind, u64)],
    error: CoreError,
) -> Result<T, CoreError> {
    for (kind, id) in stored.iter().rev() {
        let _ = vault.delete(*kind, *id);
    }
    Err(error)
}

fn curve_to_wire(key: &pqxdh::PublicCurvePreKey) -> v1::CurvePreKey {
    v1::CurvePreKey {
        id: key.id,
        public_key: key.key.to_vec(),
    }
}

fn kem_to_wire(key: &pqxdh::PublicKemPreKey, signature: [u8; 64]) -> v1::KemPreKey {
    v1::KemPreKey {
        id: key.id,
        public_key: key.key.clone(),
        one_time: key.one_time,
        signature: signature.to_vec(),
    }
}

fn curve_from_wire(key: &v1::CurvePreKey) -> Result<pqxdh::PublicCurvePreKey, CoreError> {
    Ok(pqxdh::PublicCurvePreKey {
        id: key.id,
        key: fixed(&key.public_key)?,
    })
}

fn fixed<const N: usize>(bytes: &[u8]) -> Result<[u8; N], CoreError> {
    bytes.try_into().map_err(|_| CoreError::Authentication)
}
