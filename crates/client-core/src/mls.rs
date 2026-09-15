use crate::{crypto::SecretBytes, protocol, CoreError};
use links_identity::DeviceBinding;
use openmls::prelude::tls_codec::{Deserialize as TlsDeserialize, Serialize as TlsSerialize};
use openmls::prelude::*;
use openmls_traits::{
    signatures::{Signer, SignerError},
    OpenMlsProvider as RawOpenMlsProvider,
};
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

const DIRECT_MAX_USERS: usize = 2;
pub const MAX_GROUP_USERS: usize = 100;
pub const MAX_GROUP_DEVICES: usize = protocol::MAX_FANOUT_DEVICES;

/// OpenMLS's hybrid draft suite: ML-KEM-768 + X25519 for TreeKEM, with
/// AES-128-GCM, SHA-256, and Ed25519 authentication.
///
/// This suite is a draft interoperability point. Keep it pinned until the MLS
/// post-quantum ciphersuite is standardized and all peers support the same
/// version.
pub const MLS_CIPHERSUITE: Ciphersuite =
    Ciphersuite::MLS_128_MLKEM768X25519_AES128GCM_SHA256_Ed25519;

/// Metadata must come from verified MLS authentication, never plaintext claims.
pub struct AuthenticatedApplication {
    pub conversation_id: String,
    pub sender_device_id: String,
    pub plaintext: SecretBytes,
}

/// An MLS commit that is staged locally and must be retained until the
/// delivery service accepts it. The group epoch advances only after the commit
/// is confirmed by `process_commit` or `merge_pending_commit`.
pub struct PendingCommit {
    pub commit: Vec<u8>,
    pub welcome: Option<Vec<u8>>,
    pub epoch: u64,
}

/// Core MLS operations used by the envelope layer.
pub trait MlsEngine {
    fn create_group(&mut self, conversation_id: &str, credential: &[u8]) -> Result<(), CoreError>;
    fn join_group(&mut self, conversation_id: &str, welcome: &[u8]) -> Result<(), CoreError>;
    fn process_commit(&mut self, conversation_id: &str, commit: &[u8]) -> Result<(), CoreError>;
    fn encrypt(
        &mut self,
        conversation_id: &str,
        sender_device_id: &str,
        plaintext: &[u8],
    ) -> Result<Vec<u8>, CoreError>;
    fn decrypt(&mut self, ciphertext: &[u8]) -> Result<AuthenticatedApplication, CoreError>;

    /// Read the committed epoch used for application messages. Hosts use this
    /// to order control updates and to persist an epoch checkpoint with MLS
    /// state, never to derive keys outside OpenMLS.
    fn current_epoch(&self, _: &str) -> Result<u64, CoreError> {
        Err(CoreError::CryptoUnavailable)
    }

    /// Process a control message only if the caller's durable epoch
    /// checkpoint still matches the committed local epoch.
    fn process_commit_at_epoch(
        &mut self,
        conversation_id: &str,
        expected_epoch: u64,
        commit: &[u8],
    ) -> Result<(), CoreError> {
        if self.current_epoch(conversation_id)? != expected_epoch {
            return Err(CoreError::Authentication);
        }
        self.process_commit(conversation_id, commit)
    }

    fn process_direct_commit_at_epoch(
        &mut self,
        conversation_id: &str,
        expected_epoch: u64,
        commit: &[u8],
    ) -> Result<(), CoreError> {
        if self.current_epoch(conversation_id)? != expected_epoch {
            return Err(CoreError::Authentication);
        }
        self.process_direct_commit(conversation_id, commit)
    }

    /// Direct-chat control updates retain the exact two-user invariant.
    fn join_direct_group(
        &mut self,
        conversation_id: &str,
        welcome: &[u8],
    ) -> Result<(), CoreError> {
        self.join_group(conversation_id, welcome)
    }

    fn process_direct_commit(
        &mut self,
        conversation_id: &str,
        commit: &[u8],
    ) -> Result<(), CoreError> {
        self.process_commit(conversation_id, commit)
    }

    /// Ensure a direct group contains the supplied recipient device leaves.
    /// The returned commit must be delivered before it is merged locally.
    fn ensure_direct_group(
        &mut self,
        _: &str,
        _: &[&[u8]],
    ) -> Result<Option<PendingCommit>, CoreError> {
        Err(CoreError::CryptoUnavailable)
    }

    /// Merge the local commit after the bootstrap delivery is accepted.
    fn merge_pending_direct_commit(&mut self, _: &str) -> Result<(), CoreError> {
        Err(CoreError::CryptoUnavailable)
    }

    /// Ensure a many-to-many group contains the supplied recipient device
    /// leaves. The returned commit must be delivered before it is merged.
    fn ensure_group(&mut self, _: &str, _: &[&[u8]]) -> Result<Option<PendingCommit>, CoreError> {
        Err(CoreError::CryptoUnavailable)
    }

    fn merge_pending_group_commit(&mut self, _: &str) -> Result<(), CoreError> {
        Err(CoreError::CryptoUnavailable)
    }
}

impl MlsEngine for crate::crypto::UnavailableCrypto {
    fn create_group(&mut self, _: &str, _: &[u8]) -> Result<(), CoreError> {
        Err(CoreError::CryptoUnavailable)
    }
    fn join_group(&mut self, _: &str, _: &[u8]) -> Result<(), CoreError> {
        Err(CoreError::CryptoUnavailable)
    }
    fn process_commit(&mut self, _: &str, _: &[u8]) -> Result<(), CoreError> {
        Err(CoreError::CryptoUnavailable)
    }
    fn encrypt(&mut self, _: &str, _: &str, _: &[u8]) -> Result<Vec<u8>, CoreError> {
        Err(CoreError::CryptoUnavailable)
    }
    fn decrypt(&mut self, _: &[u8]) -> Result<AuthenticatedApplication, CoreError> {
        Err(CoreError::CryptoUnavailable)
    }

    fn current_epoch(&self, _: &str) -> Result<u64, CoreError> {
        Err(CoreError::CryptoUnavailable)
    }
}

/// The application-facing identity signer. Native implementations should
/// delegate these calls to the iOS Secure Enclave or Android Keystore TEE.
pub trait MlsIdentitySigner {
    fn public_key(&self) -> Result<[u8; 32], CoreError>;
    fn sign(&self, transcript: &[u8]) -> Result<[u8; 64], CoreError>;
}

impl<T: crate::prekeys::PreKeySigner> MlsIdentitySigner for T {
    fn public_key(&self) -> Result<[u8; 32], CoreError> {
        crate::prekeys::PreKeySigner::public_key(self)
    }

    fn sign(&self, transcript: &[u8]) -> Result<[u8; 64], CoreError> {
        crate::prekeys::PreKeySigner::sign(self, transcript)
    }
}

/// Adapter from the device identity signer to the OpenMLS signer interface.
pub struct OpenMlsSigner<S> {
    signer: S,
    public_key: [u8; 32],
}

impl<S: MlsIdentitySigner> OpenMlsSigner<S> {
    pub fn new(signer: S) -> Result<Self, CoreError> {
        let public_key = signer.public_key()?;
        links_identity::validate_public_key(&public_key).map_err(|_| CoreError::Authentication)?;
        Ok(Self { signer, public_key })
    }

    pub fn public_key(&self) -> &[u8; 32] {
        &self.public_key
    }
}

impl<S: MlsIdentitySigner> Signer for OpenMlsSigner<S> {
    fn sign(&self, payload: &[u8]) -> Result<Vec<u8>, SignerError> {
        self.signer
            .sign(payload)
            .map(|signature| signature.to_vec())
            .map_err(|_| SignerError::SigningError)
    }

    fn signature_scheme(&self) -> SignatureScheme {
        SignatureScheme::ED25519
    }
}

/// The trust-directory hook for MLS credentials. A BasicCredential is only an
/// identity container; this verifier must check enrollment, user membership,
/// and revocation before a leaf is trusted.
pub trait MlsCredentialVerifier {
    fn verify(&self, binding: &DeviceBinding) -> Result<(), CoreError>;
}

impl<F> MlsCredentialVerifier for F
where
    F: Fn(&DeviceBinding) -> Result<(), CoreError>,
{
    fn verify(&self, binding: &DeviceBinding) -> Result<(), CoreError> {
        self(binding)
    }
}

/// Durable OpenMLS provider using RustCrypto for MLS primitives and an
/// application-owned storage implementation for ratchet state and private
/// key material.
pub struct RustCryptoProvider<Storage> {
    crypto: openmls_rust_crypto::RustCrypto,
    storage: Storage,
}

impl<Storage> RustCryptoProvider<Storage> {
    pub fn new(storage: Storage) -> Self {
        Self {
            crypto: openmls_rust_crypto::RustCrypto::default(),
            storage,
        }
    }

    pub fn storage(&self) -> &Storage {
        &self.storage
    }
}

impl<Storage> RawOpenMlsProvider for RustCryptoProvider<Storage>
where
    Storage: openmls::storage::StorageProvider,
{
    type CryptoProvider = openmls_rust_crypto::RustCrypto;
    type RandProvider = openmls_rust_crypto::RustCrypto;
    type StorageProvider = Storage;

    fn storage(&self) -> &Self::StorageProvider {
        &self.storage
    }

    fn crypto(&self) -> &Self::CryptoProvider {
        &self.crypto
    }

    fn rand(&self) -> &Self::RandProvider {
        &self.crypto
    }
}

/// OpenMLS-backed MLS engine. OpenMLS owns the RFC 9420 TreeKEM tree and epoch
/// ratchets; the host owns durable storage and credential trust checks. Direct
/// conversations use the two-user path, while group conversations allow a
/// bounded many-to-many membership set.
pub struct OpenMlsEngine<P, S, V> {
    provider: P,
    signer: OpenMlsSigner<S>,
    verifier: V,
    credential: CredentialWithKey,
    local_binding: DeviceBinding,
}

impl<P, S, V> OpenMlsEngine<P, S, V>
where
    P: openmls::storage::OpenMlsProvider,
    S: MlsIdentitySigner,
    V: MlsCredentialVerifier,
{
    /// Construct an engine from the full TLS-encoded MLS BasicCredential.
    /// The credential's embedded Ed25519 key must match the hardware signer.
    pub fn new(
        provider: P,
        signer: S,
        credential_bytes: &[u8],
        verifier: V,
    ) -> Result<Self, CoreError> {
        let public_key = signer.public_key()?;
        let signer = OpenMlsSigner::new(signer)?;
        let credential = parse_credential(credential_bytes)?;
        let local_binding = verify_credential(&verifier, &credential, &public_key)?;
        Ok(Self {
            provider,
            signer,
            verifier,
            credential: CredentialWithKey {
                credential,
                signature_key: public_key.to_vec().into(),
            },
            local_binding,
        })
    }

    pub fn provider(&self) -> &P {
        &self.provider
    }

    pub fn provider_mut(&mut self) -> &mut P {
        &mut self.provider
    }

    pub fn local_binding(&self) -> &DeviceBinding {
        &self.local_binding
    }

    /// Generate one offline-initiation KeyPackage. The private init and leaf
    /// encryption keys are written by OpenMLS into the provider's storage.
    pub fn generate_key_package(&self) -> Result<Vec<u8>, CoreError> {
        let bundle = KeyPackage::builder()
            .leaf_node_capabilities(Capabilities::for_provider(self.provider.crypto()))
            .build(
                MLS_CIPHERSUITE,
                &self.provider,
                &self.signer,
                self.credential.clone(),
            )
            .map_err(|_| CoreError::Provider)?;
        bundle
            .key_package()
            .tls_serialize_detached()
            .map_err(|_| CoreError::Provider)
    }

    /// Return whether a conversation is ready for one-to-one application
    /// messages. Each physical device is a leaf, so this does not require two
    /// leaves when either user has multiple devices.
    pub fn direct_group_ready(&self, conversation_id: &str) -> Result<bool, CoreError> {
        let group_id = group_id(conversation_id)?;
        let group = self.load_group(&group_id)?;
        let users = verified_group_user_counts(&self.verifier, &group)?;
        Ok(users.len() == DIRECT_MAX_USERS && users.contains_key(&self.local_binding.user_id))
    }

    /// Return whether a many-to-many group can carry application messages.
    /// Groups need at least two verified users and may contain several device
    /// leaves for each user.
    pub fn group_ready(&self, conversation_id: &str) -> Result<bool, CoreError> {
        let group_id = group_id(conversation_id)?;
        let group = self.load_group(&group_id)?;
        let users = verified_group_user_counts(&self.verifier, &group)?;
        Ok(group_shape_is_ready(
            &users,
            &self.local_binding.user_id,
            MAX_GROUP_USERS,
        ))
    }

    /// Return the committed MLS epoch for a conversation. OpenMLS remains the
    /// source of truth; this accessor is for durable host checkpoints and
    /// ordering authenticated Welcome/Commit updates.
    pub fn current_epoch(&self, conversation_id: &str) -> Result<u64, CoreError> {
        let group_id = group_id(conversation_id)?;
        Ok(self.load_group(&group_id)?.epoch().as_u64())
    }

    /// Recover the device bindings already authenticated into a locally
    /// persisted group. The encrypted MLS state is the source for this cache
    /// migration; the caller still uses the normal directory verifier for new
    /// credentials and group updates.
    pub fn group_member_bindings(
        &self,
        conversation_id: &str,
    ) -> Result<Vec<DeviceBinding>, CoreError> {
        let group_id = group_id(conversation_id)?;
        let group = self.load_group(&group_id)?;
        group
            .members()
            .map(|member| {
                let basic = BasicCredential::try_from(member.credential.clone())
                    .map_err(|_| CoreError::Authentication)?;
                let binding = links_identity::parse_mls_basic_identity(basic.identity())
                    .map_err(|_| CoreError::Authentication)?;
                if binding.public_key.as_slice() != member.signature_key.as_slice() {
                    return Err(CoreError::Authentication);
                }
                Ok(binding)
            })
            .collect()
    }

    /// Process one group commit against a caller-owned epoch checkpoint.
    /// OpenMLS performs the cryptographic validation and advances the epoch;
    /// the checkpoint prevents a replayed or concurrently applied update.
    pub fn process_group_commit_at_epoch(
        &mut self,
        conversation_id: &str,
        expected_epoch: u64,
        commit: &[u8],
    ) -> Result<(), CoreError> {
        if self.current_epoch(conversation_id)? != expected_epoch {
            return Err(CoreError::Authentication);
        }
        self.process_commit_for_group(group_id(conversation_id)?, commit, MAX_GROUP_USERS)
    }

    pub fn process_direct_commit_at_epoch(
        &mut self,
        conversation_id: &str,
        expected_epoch: u64,
        commit: &[u8],
    ) -> Result<(), CoreError> {
        if self.current_epoch(conversation_id)? != expected_epoch {
            return Err(CoreError::Authentication);
        }
        self.process_commit_for_group(group_id(conversation_id)?, commit, DIRECT_MAX_USERS)
    }

    /// Stage a TreeKEM member-add commit. Keep the returned bytes until the
    /// delivery service accepts the commit, then call `process_commit`.
    pub fn add_members(
        &mut self,
        conversation_id: &str,
        key_packages: &[&[u8]],
    ) -> Result<PendingCommit, CoreError> {
        self.add_members_with_user_limit(conversation_id, key_packages, DIRECT_MAX_USERS)
    }

    /// Stage a TreeKEM member-add commit for a many-to-many group. Each
    /// physical device is one leaf; all added leaves must have verified
    /// credentials and the resulting group stays within the bounded limits.
    pub fn add_group_members(
        &mut self,
        conversation_id: &str,
        key_packages: &[&[u8]],
    ) -> Result<PendingCommit, CoreError> {
        self.add_members_with_user_limit(conversation_id, key_packages, MAX_GROUP_USERS)
    }

    fn add_members_with_user_limit(
        &mut self,
        conversation_id: &str,
        key_packages: &[&[u8]],
        max_users: usize,
    ) -> Result<PendingCommit, CoreError> {
        if key_packages.is_empty() {
            return Err(CoreError::Authentication);
        }
        if key_packages.len() > MAX_GROUP_DEVICES {
            return Err(CoreError::Authentication);
        }
        let group_id = group_id(conversation_id)?;
        let key_packages = key_packages
            .iter()
            .map(|bytes| self.validate_key_package(bytes))
            .collect::<Result<Vec<_>, _>>()?;
        let mut group = self.load_group(&group_id)?;
        let mut users = verified_group_user_counts(&self.verifier, &group)?;
        ensure_group_user_limit(&users, max_users)?;
        if group.members().count().saturating_add(key_packages.len()) > MAX_GROUP_DEVICES {
            return Err(CoreError::Authentication);
        }
        let mut allowed_users = users.keys().copied().collect::<Vec<_>>();
        for key_package in &key_packages {
            let binding = verify_leaf(&self.verifier, key_package.leaf_node())?;
            if !allowed_users.contains(&binding.user_id) {
                if allowed_users.len() == max_users {
                    return Err(CoreError::Authentication);
                }
                allowed_users.push(binding.user_id);
            }
            increment_user(&mut users, binding.user_id);
        }
        ensure_group_user_limit(&users, max_users)?;
        let (commit, welcome, _) = group
            .add_members(&self.provider, &self.signer, &key_packages)
            .map_err(|_| CoreError::Provider)?;
        Ok(PendingCommit {
            commit: serialize_message(commit)?,
            welcome: Some(serialize_message(welcome)?),
            epoch: group.epoch().as_u64().saturating_add(1),
        })
    }

    /// Stage a TreeKEM member-remove commit by leaf index.
    pub fn remove_members(
        &mut self,
        conversation_id: &str,
        leaf_indices: &[u32],
    ) -> Result<PendingCommit, CoreError> {
        if leaf_indices.is_empty() {
            return Err(CoreError::Authentication);
        }
        let group_id = group_id(conversation_id)?;
        let members = leaf_indices
            .iter()
            .copied()
            .map(LeafNodeIndex::new)
            .collect::<Vec<_>>();
        let mut group = self.load_group(&group_id)?;
        let (commit, welcome, _) = group
            .remove_members(&self.provider, &self.signer, &members)
            .map_err(|_| CoreError::Provider)?;
        Ok(PendingCommit {
            commit: serialize_message(commit)?,
            welcome: welcome.map(serialize_message).transpose()?,
            epoch: group.epoch().as_u64().saturating_add(1),
        })
    }

    /// Stage removal of revoked or departed physical devices. Device IDs are
    /// resolved against verified MLS credentials, so a caller cannot remove a
    /// leaf by presenting an unrelated plaintext user/device claim.
    pub fn remove_devices(
        &mut self,
        conversation_id: &str,
        device_ids: &[Uuid],
    ) -> Result<PendingCommit, CoreError> {
        if device_ids.is_empty() || device_ids.len() > MAX_GROUP_DEVICES {
            return Err(CoreError::Authentication);
        }
        let requested = device_ids.iter().copied().collect::<HashSet<_>>();
        if requested.len() != device_ids.len() || requested.iter().any(Uuid::is_nil) {
            return Err(CoreError::Authentication);
        }
        let group_id = group_id(conversation_id)?;
        let group = self.load_group(&group_id)?;
        let mut leaves = Vec::with_capacity(requested.len());
        for member in group.members() {
            let binding =
                verify_credential(&self.verifier, &member.credential, &member.signature_key)?;
            if requested.contains(&binding.device_id) {
                leaves.push(member.index.u32());
            }
        }
        if leaves.len() != requested.len() {
            return Err(CoreError::Authentication);
        }
        self.remove_members(conversation_id, &leaves)
    }

    /// Stage a fresh self-update. OpenMLS creates a new TreeKEM path, so every
    /// member receives new path secrets after this commit is merged.
    pub fn self_update(&mut self, conversation_id: &str) -> Result<PendingCommit, CoreError> {
        let group_id = group_id(conversation_id)?;
        let mut group = self.load_group(&group_id)?;
        let bundle = group
            .self_update(&self.provider, &self.signer, LeafNodeParameters::default())
            .map_err(|_| CoreError::Provider)?;
        let commit = serialize_message(bundle.into_commit())?;
        Ok(PendingCommit {
            commit,
            welcome: None,
            epoch: group.epoch().as_u64().saturating_add(1),
        })
    }

    /// Merge the local pending TreeKEM commit after the delivery service
    /// confirms acceptance.
    pub fn merge_pending_commit(&mut self, conversation_id: &str) -> Result<(), CoreError> {
        let group_id = group_id(conversation_id)?;
        let mut group = self.load_group(&group_id)?;
        group
            .merge_pending_commit(&self.provider)
            .map_err(|_| CoreError::Provider)
    }

    fn load_group(&self, group_id: &GroupId) -> Result<MlsGroup, CoreError> {
        MlsGroup::load(self.provider.storage(), group_id)
            .map_err(|_| CoreError::Provider)?
            .ok_or(CoreError::Authentication)
    }

    fn validate_key_package(&self, bytes: &[u8]) -> Result<KeyPackage, CoreError> {
        let input =
            KeyPackageIn::tls_deserialize_exact(bytes).map_err(|_| CoreError::Authentication)?;
        let key_package = input
            .validate(self.provider.crypto(), ProtocolVersion::Mls10)
            .map_err(|_| CoreError::Authentication)?;
        if key_package.ciphersuite() != MLS_CIPHERSUITE {
            return Err(CoreError::Authentication);
        }
        verify_leaf(&self.verifier, key_package.leaf_node())?;
        Ok(key_package)
    }

    fn create_group_with_id(&self, group_id: GroupId) -> Result<(), CoreError> {
        MlsGroup::new_with_group_id(
            &self.provider,
            &self.signer,
            &create_config(self.provider.crypto()),
            group_id,
            self.credential.clone(),
        )
        .map(|_| ())
        .map_err(|_| CoreError::Provider)
    }

    fn join_group_with_id(
        &self,
        expected_group_id: GroupId,
        bytes: &[u8],
        max_users: usize,
    ) -> Result<(), CoreError> {
        let input =
            MlsMessageIn::tls_deserialize_exact(bytes).map_err(|_| CoreError::Authentication)?;
        let welcome = match input.extract() {
            MlsMessageBodyIn::Welcome(welcome) => welcome,
            _ => return Err(CoreError::Authentication),
        };
        let staged = StagedWelcome::new_from_welcome(&self.provider, &join_config(), welcome, None)
            .map_err(|_| CoreError::Authentication)?;
        if staged.group_context().group_id() != &expected_group_id {
            return Err(CoreError::Authentication);
        }
        let own_leaf = staged.own_leaf_node().ok_or(CoreError::Authentication)?;
        if own_leaf.credential() != &self.credential.credential
            || own_leaf.signature_key().as_slice() != self.signer.public_key()
        {
            return Err(CoreError::Authentication);
        }
        let users = staged
            .members()
            .map(|member| {
                verify_credential(&self.verifier, &member.credential, &member.signature_key)
                    .map(|binding| binding.user_id)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let user_counts = users
            .into_iter()
            .fold(HashMap::new(), |mut counts, user_id| {
                increment_user(&mut counts, user_id);
                counts
            });
        if !group_shape_is_ready(&user_counts, &self.local_binding.user_id, max_users)
            || staged.members().count() > MAX_GROUP_DEVICES
        {
            return Err(CoreError::Authentication);
        }
        staged
            .into_group(&self.provider)
            .map(|_| ())
            .map_err(|_| CoreError::Provider)
    }

    fn process_commit_for_group(
        &self,
        expected_group_id: GroupId,
        bytes: &[u8],
        max_users: usize,
    ) -> Result<(), CoreError> {
        let message = parse_protocol_message(bytes)?;
        if message.group_id() != &expected_group_id || message.content_type() != ContentType::Commit
        {
            return Err(CoreError::Authentication);
        }
        let mut group = self.load_group(&expected_group_id)?;
        let processed = group
            .process_message(&self.provider, message)
            .map_err(|_| CoreError::Authentication)?;
        verify_processed_sender(&self.verifier, &group, &processed)?;
        let sender_index = match processed.sender() {
            Sender::Member(index) => *index,
            _ => return Err(CoreError::Authentication),
        };
        match processed.into_content() {
            ProcessedMessageContent::StagedCommitMessage(staged) => {
                validate_staged_commit(&self.verifier, &group, sender_index, &staged, max_users)?;
                group
                    .merge_staged_commit(&self.provider, *staged)
                    .map_err(|_| CoreError::Provider)
            }
            ProcessedMessageContent::OwnPendingCommit => group
                .merge_pending_commit(&self.provider)
                .map_err(|_| CoreError::Provider),
            _ => Err(CoreError::Authentication),
        }
    }

    fn encrypt_message(
        &self,
        conversation_id: &str,
        sender_device_id: &str,
        plaintext: &[u8],
    ) -> Result<Vec<u8>, CoreError> {
        protocol::validate_id(sender_device_id)?;
        if sender_device_id != &self.local_binding.device_id.to_string() {
            return Err(CoreError::Authentication);
        }
        let group_id = group_id(conversation_id)?;
        let mut group = self.load_group(&group_id)?;
        ensure_group_ready(&self.verifier, &group, &self.local_binding.user_id)?;
        group
            .create_message(&self.provider, &self.signer, plaintext)
            .map_err(|_| CoreError::Provider)
            .and_then(serialize_message)
    }

    fn decrypt_message(&self, bytes: &[u8]) -> Result<AuthenticatedApplication, CoreError> {
        let message = parse_protocol_message(bytes)?;
        if message.content_type() != ContentType::Application {
            return Err(CoreError::Authentication);
        }
        let group_id = message.group_id().clone();
        let mut group = self.load_group(&group_id)?;
        ensure_group_ready(&self.verifier, &group, &self.local_binding.user_id)?;
        let processed = group
            .process_message(&self.provider, message)
            .map_err(|_| CoreError::Authentication)?;
        let binding = verify_processed_sender(&self.verifier, &group, &processed)?;
        let plaintext = match processed.into_content() {
            ProcessedMessageContent::ApplicationMessage(message) => message.into_bytes(),
            _ => return Err(CoreError::Authentication),
        };
        Ok(AuthenticatedApplication {
            conversation_id: conversation_id(&group_id)?,
            sender_device_id: binding.device_id.to_string(),
            plaintext: SecretBytes::new(plaintext),
        })
    }

    fn ensure_group_members(
        &mut self,
        conversation_id: &str,
        key_package_bytes: &[&[u8]],
    ) -> Result<Option<PendingCommit>, CoreError> {
        if key_package_bytes.is_empty() || key_package_bytes.len() > MAX_GROUP_DEVICES {
            return Err(CoreError::Authentication);
        }

        let group_id = group_id(conversation_id)?;
        let mut packages = Vec::with_capacity(key_package_bytes.len());
        let mut target_devices = HashSet::with_capacity(key_package_bytes.len());
        for bytes in key_package_bytes {
            let package = self.validate_key_package(bytes)?;
            let binding = verify_leaf(&self.verifier, package.leaf_node())?;
            if binding.user_id == self.local_binding.user_id
                || !target_devices.insert(binding.device_id)
            {
                return Err(CoreError::Authentication);
            }
            packages.push((*bytes, binding.device_id));
        }

        let existing =
            MlsGroup::load(self.provider.storage(), &group_id).map_err(|_| CoreError::Provider)?;
        let Some(group) = existing else {
            self.create_group_with_id(group_id)?;
            return self
                .add_group_members(conversation_id, key_package_bytes)
                .map(Some);
        };

        let users = verified_group_user_counts(&self.verifier, &group)?;
        ensure_group_user_limit(&users, MAX_GROUP_USERS)?;
        if !users.contains_key(&self.local_binding.user_id)
            || group.members().count() > MAX_GROUP_DEVICES
        {
            return Err(CoreError::Authentication);
        }

        let existing_devices = group
            .members()
            .map(|member| {
                verify_credential(&self.verifier, &member.credential, &member.signature_key)
                    .map(|binding| binding.device_id)
            })
            .collect::<Result<HashSet<_>, _>>()?;
        let missing = packages
            .iter()
            .filter(|(_, device_id)| !existing_devices.contains(device_id))
            .map(|(bytes, _)| *bytes)
            .collect::<Vec<_>>();
        if missing.is_empty() {
            ensure_group_ready(&self.verifier, &group, &self.local_binding.user_id)?;
            return Ok(None);
        }
        self.add_group_members(conversation_id, &missing).map(Some)
    }
}

impl<P, S, V> MlsEngine for OpenMlsEngine<P, S, V>
where
    P: openmls::storage::OpenMlsProvider,
    S: MlsIdentitySigner,
    V: MlsCredentialVerifier,
{
    fn create_group(&mut self, conversation_id: &str, credential: &[u8]) -> Result<(), CoreError> {
        let credential = parse_credential(credential)?;
        if credential != self.credential.credential {
            return Err(CoreError::Authentication);
        }
        self.create_group_with_id(group_id(conversation_id)?)
    }

    fn join_group(&mut self, conversation_id: &str, welcome: &[u8]) -> Result<(), CoreError> {
        self.join_group_with_id(group_id(conversation_id)?, welcome, MAX_GROUP_USERS)
    }

    fn join_direct_group(
        &mut self,
        conversation_id: &str,
        welcome: &[u8],
    ) -> Result<(), CoreError> {
        let group_id = group_id(conversation_id)?;
        if let Some(group) = MlsGroup::load(self.provider.storage(), &group_id)
            .map_err(|_| CoreError::Provider)?
        {
            ensure_direct_group_ready(&self.verifier, &group, &self.local_binding.user_id)?;
            return Ok(());
        }
        self.join_group_with_id(group_id, welcome, DIRECT_MAX_USERS)
    }

    fn process_commit(&mut self, conversation_id: &str, commit: &[u8]) -> Result<(), CoreError> {
        self.process_commit_for_group(group_id(conversation_id)?, commit, MAX_GROUP_USERS)
    }

    fn process_direct_commit(
        &mut self,
        conversation_id: &str,
        commit: &[u8],
    ) -> Result<(), CoreError> {
        self.process_commit_for_group(group_id(conversation_id)?, commit, DIRECT_MAX_USERS)
    }

    fn encrypt(
        &mut self,
        conversation_id: &str,
        sender_device_id: &str,
        plaintext: &[u8],
    ) -> Result<Vec<u8>, CoreError> {
        self.encrypt_message(conversation_id, sender_device_id, plaintext)
    }

    fn decrypt(&mut self, ciphertext: &[u8]) -> Result<AuthenticatedApplication, CoreError> {
        self.decrypt_message(ciphertext)
    }

    fn current_epoch(&self, conversation_id: &str) -> Result<u64, CoreError> {
        OpenMlsEngine::current_epoch(self, conversation_id)
    }

    fn ensure_direct_group(
        &mut self,
        conversation_id: &str,
        key_packages: &[&[u8]],
    ) -> Result<Option<PendingCommit>, CoreError> {
        self.ensure_direct_group_members(conversation_id, key_packages)
    }

    fn ensure_group(
        &mut self,
        conversation_id: &str,
        key_packages: &[&[u8]],
    ) -> Result<Option<PendingCommit>, CoreError> {
        self.ensure_group_members(conversation_id, key_packages)
    }

    fn merge_pending_direct_commit(&mut self, conversation_id: &str) -> Result<(), CoreError> {
        self.merge_pending_commit(conversation_id)
    }

    fn merge_pending_group_commit(&mut self, conversation_id: &str) -> Result<(), CoreError> {
        self.merge_pending_commit(conversation_id)
    }
}

impl<P, S, V> OpenMlsEngine<P, S, V>
where
    P: openmls::storage::OpenMlsProvider,
    S: MlsIdentitySigner,
    V: MlsCredentialVerifier,
{
    fn ensure_direct_group_members(
        &mut self,
        conversation_id: &str,
        key_package_bytes: &[&[u8]],
    ) -> Result<Option<PendingCommit>, CoreError> {
        if key_package_bytes.is_empty() {
            return Err(CoreError::Authentication);
        }

        let group_id = group_id(conversation_id)?;
        let mut packages = Vec::with_capacity(key_package_bytes.len());
        let mut target_devices = HashSet::with_capacity(key_package_bytes.len());
        let mut target_user = None;
        for bytes in key_package_bytes {
            let package = self.validate_key_package(bytes)?;
            let binding = verify_leaf(&self.verifier, package.leaf_node())?;
            if binding.user_id == self.local_binding.user_id
                || !target_devices.insert(binding.device_id)
                || target_user.is_some_and(|user_id| user_id != binding.user_id)
            {
                return Err(CoreError::Authentication);
            }
            target_user = Some(binding.user_id);
            packages.push((*bytes, binding.device_id));
        }

        let existing =
            MlsGroup::load(self.provider.storage(), &group_id).map_err(|_| CoreError::Provider)?;
        let Some(group) = existing else {
            self.create_group_with_id(group_id)?;
            return self
                .add_members(conversation_id, key_package_bytes)
                .map(Some);
        };

        let users = verified_group_user_counts(&self.verifier, &group)?;
        ensure_group_user_limit(&users, DIRECT_MAX_USERS)?;
        if !users.contains_key(&self.local_binding.user_id)
            || matches!(target_user, Some(user_id) if
                users.len() == DIRECT_MAX_USERS && !users.contains_key(&user_id))
        {
            return Err(CoreError::Authentication);
        }

        let existing_devices = group
            .members()
            .map(|member| {
                verify_credential(&self.verifier, &member.credential, &member.signature_key)
                    .map(|binding| binding.device_id)
            })
            .collect::<Result<HashSet<_>, _>>()?;
        let missing = packages
            .iter()
            .filter(|(_, device_id)| !existing_devices.contains(device_id))
            .map(|(bytes, _)| *bytes)
            .collect::<Vec<_>>();
        if missing.is_empty() {
            ensure_direct_group_ready(&self.verifier, &group, &self.local_binding.user_id)?;
            return Ok(None);
        }
        self.add_members(conversation_id, &missing).map(Some)
    }
}

fn create_config(crypto: &impl OpenMlsCrypto) -> MlsGroupCreateConfig {
    MlsGroupCreateConfig::builder()
        .ciphersuite(MLS_CIPHERSUITE)
        .capabilities(Capabilities::for_provider(crypto))
        .wire_format_policy(PURE_CIPHERTEXT_WIRE_FORMAT_POLICY)
        .use_ratchet_tree_extension(true)
        .build()
}

fn join_config() -> MlsGroupJoinConfig {
    MlsGroupJoinConfig::builder()
        .wire_format_policy(PURE_CIPHERTEXT_WIRE_FORMAT_POLICY)
        .use_ratchet_tree_extension(true)
        .build()
}

fn parse_credential(bytes: &[u8]) -> Result<Credential, CoreError> {
    Credential::tls_deserialize_exact(bytes).map_err(|_| CoreError::Authentication)
}

fn verify_credential<V: MlsCredentialVerifier>(
    verifier: &V,
    credential: &Credential,
    signature_key: &[u8],
) -> Result<DeviceBinding, CoreError> {
    let basic =
        BasicCredential::try_from(credential.clone()).map_err(|_| CoreError::Authentication)?;
    let binding = links_identity::parse_mls_basic_identity(basic.identity())
        .map_err(|_| CoreError::Authentication)?;
    if binding.public_key.as_slice() != signature_key {
        return Err(CoreError::Authentication);
    }
    verifier.verify(&binding)?;
    Ok(binding)
}

fn verify_leaf<V: MlsCredentialVerifier>(
    verifier: &V,
    leaf: &LeafNode,
) -> Result<DeviceBinding, CoreError> {
    verify_credential(verifier, leaf.credential(), leaf.signature_key().as_slice())
}

fn verify_processed_sender<V: MlsCredentialVerifier>(
    verifier: &V,
    group: &MlsGroup,
    processed: &ProcessedMessage,
) -> Result<DeviceBinding, CoreError> {
    let index = match processed.sender() {
        Sender::Member(index) => *index,
        _ => return Err(CoreError::Authentication),
    };
    let member = group.member_at(index).ok_or(CoreError::Authentication)?;
    if &member.credential != processed.credential() {
        return Err(CoreError::Authentication);
    }
    verify_credential(verifier, &member.credential, &member.signature_key)
}

fn validate_staged_commit<V: MlsCredentialVerifier>(
    verifier: &V,
    group: &MlsGroup,
    sender_index: LeafNodeIndex,
    staged: &StagedCommit,
    max_users: usize,
) -> Result<(), CoreError> {
    let mut users = verified_group_user_counts(verifier, group)?;
    ensure_group_user_limit(&users, max_users)?;
    let mut leaf_count = group.members().count();
    if leaf_count > MAX_GROUP_DEVICES {
        return Err(CoreError::Authentication);
    }
    let mut allowed_users = users.keys().copied().collect::<Vec<_>>();

    for proposal in staged.queued_proposals() {
        match proposal.proposal() {
            Proposal::Add(_) | Proposal::Update(_) | Proposal::Remove(_) => {}
            Proposal::SelfRemove => {
                let index = match proposal.sender() {
                    Sender::Member(index) => *index,
                    _ => return Err(CoreError::Authentication),
                };
                let member = group.member_at(index).ok_or(CoreError::Authentication)?;
                let binding =
                    verify_credential(verifier, &member.credential, &member.signature_key)?;
                decrement_user(&mut users, binding.user_id)?;
                leaf_count = leaf_count.checked_sub(1).ok_or(CoreError::Authentication)?;
            }
            _ => return Err(CoreError::Authentication),
        }
    }

    for proposal in staged.remove_proposals() {
        let index = proposal.remove_proposal().removed();
        let member = group.member_at(index).ok_or(CoreError::Authentication)?;
        let binding = verify_credential(verifier, &member.credential, &member.signature_key)?;
        decrement_user(&mut users, binding.user_id)?;
        leaf_count = leaf_count.checked_sub(1).ok_or(CoreError::Authentication)?;
    }

    for proposal in staged.add_proposals() {
        let binding = verify_leaf(verifier, proposal.add_proposal().key_package().leaf_node())?;
        if !allowed_users.contains(&binding.user_id) {
            if allowed_users.len() == max_users {
                return Err(CoreError::Authentication);
            }
            allowed_users.push(binding.user_id);
        }
        increment_user(&mut users, binding.user_id);
        leaf_count = leaf_count.checked_add(1).ok_or(CoreError::Authentication)?;
    }

    for proposal in staged.update_proposals() {
        let index = match proposal.sender() {
            Sender::Member(index) => *index,
            _ => return Err(CoreError::Authentication),
        };
        let member = group.member_at(index).ok_or(CoreError::Authentication)?;
        let current = verify_credential(verifier, &member.credential, &member.signature_key)?;
        let updated = verify_leaf(verifier, proposal.update_proposal().leaf_node())?;
        if updated.user_id != current.user_id {
            return Err(CoreError::Authentication);
        }
    }

    if let Some(leaf) = staged.update_path_leaf_node() {
        let member = group
            .member_at(sender_index)
            .ok_or(CoreError::Authentication)?;
        let current = verify_credential(verifier, &member.credential, &member.signature_key)?;
        let updated = verify_leaf(verifier, leaf)?;
        if updated.user_id != current.user_id {
            return Err(CoreError::Authentication);
        }
    }
    ensure_group_user_limit(&users, max_users)?;
    if leaf_count > MAX_GROUP_DEVICES {
        return Err(CoreError::Authentication);
    }
    Ok(())
}

fn verified_group_user_counts<V: MlsCredentialVerifier>(
    verifier: &V,
    group: &MlsGroup,
) -> Result<HashMap<Uuid, usize>, CoreError> {
    group
        .members()
        .map(|member| {
            verify_credential(verifier, &member.credential, &member.signature_key)
                .map(|binding| binding.user_id)
        })
        .try_fold(HashMap::new(), |mut users, user_id| {
            increment_user(&mut users, user_id?);
            Ok(users)
        })
}

fn increment_user(users: &mut HashMap<Uuid, usize>, user_id: Uuid) {
    *users.entry(user_id).or_insert(0) += 1;
}

fn decrement_user(users: &mut HashMap<Uuid, usize>, user_id: Uuid) -> Result<(), CoreError> {
    let count = users.get_mut(&user_id).ok_or(CoreError::Authentication)?;
    if *count == 1 {
        users.remove(&user_id);
    } else {
        *count -= 1;
    }
    Ok(())
}

fn ensure_group_user_limit(
    users: &HashMap<Uuid, usize>,
    max_users: usize,
) -> Result<(), CoreError> {
    if users.len() > max_users {
        Err(CoreError::Authentication)
    } else {
        Ok(())
    }
}

fn group_shape_is_ready(
    users: &HashMap<Uuid, usize>,
    local_user_id: &Uuid,
    max_users: usize,
) -> bool {
    users.len() >= 2 && users.len() <= max_users && users.contains_key(local_user_id)
}

fn ensure_group_ready<V: MlsCredentialVerifier>(
    verifier: &V,
    group: &MlsGroup,
    local_user_id: &Uuid,
) -> Result<(), CoreError> {
    let users = verified_group_user_counts(verifier, group)?;
    if !group_shape_is_ready(&users, local_user_id, MAX_GROUP_USERS) {
        return Err(CoreError::Authentication);
    }
    Ok(())
}

fn ensure_direct_group_ready<V: MlsCredentialVerifier>(
    verifier: &V,
    group: &MlsGroup,
    local_user_id: &Uuid,
) -> Result<(), CoreError> {
    let users = verified_group_user_counts(verifier, group)?;
    if !group_shape_is_ready(&users, local_user_id, DIRECT_MAX_USERS)
        || users.len() != DIRECT_MAX_USERS
    {
        return Err(CoreError::Authentication);
    }
    Ok(())
}

fn parse_protocol_message(bytes: &[u8]) -> Result<ProtocolMessage, CoreError> {
    MlsMessageIn::tls_deserialize_exact(bytes)
        .map_err(|_| CoreError::Authentication)?
        .try_into_protocol_message()
        .map_err(|_| CoreError::Authentication)
}

fn serialize_message(message: MlsMessageOut) -> Result<Vec<u8>, CoreError> {
    message.to_bytes().map_err(|_| CoreError::Provider)
}

fn group_id(value: &str) -> Result<GroupId, CoreError> {
    protocol::validate_id(value)?;
    let uuid = Uuid::parse_str(value).map_err(|_| CoreError::Authentication)?;
    Ok(GroupId::from_slice(uuid.as_bytes()))
}

fn conversation_id(group_id: &GroupId) -> Result<String, CoreError> {
    let uuid = Uuid::from_slice(group_id.as_slice()).map_err(|_| CoreError::Authentication)?;
    let value = uuid.hyphenated().to_string();
    protocol::validate_id(&value)?;
    Ok(value)
}
