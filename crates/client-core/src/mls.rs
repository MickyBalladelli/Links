use crate::{crypto::SecretBytes, protocol, CoreError};
use links_identity::DeviceBinding;
use openmls::prelude::tls_codec::{Deserialize as TlsDeserialize, Serialize as TlsSerialize};
use openmls::prelude::*;
use openmls_traits::{
    signatures::{Signer, SignerError},
    OpenMlsProvider as RawOpenMlsProvider,
};
use uuid::Uuid;

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

/// OpenMLS-backed MLS engine. OpenMLS owns the RFC 9420 TreeKEM tree and
/// epoch ratchets; the host owns durable storage and credential trust checks.
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

    /// Stage a TreeKEM member-add commit. Keep the returned bytes until the
    /// delivery service accepts the commit, then call `process_commit`.
    pub fn add_members(
        &mut self,
        conversation_id: &str,
        key_packages: &[&[u8]],
    ) -> Result<PendingCommit, CoreError> {
        if key_packages.is_empty() {
            return Err(CoreError::Authentication);
        }
        let group_id = group_id(conversation_id)?;
        let key_packages = key_packages
            .iter()
            .map(|bytes| self.validate_key_package(bytes))
            .collect::<Result<Vec<_>, _>>()?;
        let mut group = self.load_group(&group_id)?;
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
        for member in staged.members() {
            verify_credential(&self.verifier, &member.credential, &member.signature_key)?;
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
        match processed.into_content() {
            ProcessedMessageContent::StagedCommitMessage(staged) => {
                validate_staged_commit(&self.verifier, &staged)?;
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
        self.join_group_with_id(group_id(conversation_id)?, welcome)
    }

    fn process_commit(&mut self, conversation_id: &str, commit: &[u8]) -> Result<(), CoreError> {
        self.process_commit_for_group(group_id(conversation_id)?, commit)
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
    staged: &StagedCommit,
) -> Result<(), CoreError> {
    if let Some(leaf) = staged.update_path_leaf_node() {
        verify_leaf(verifier, leaf)?;
    }
    for proposal in staged.add_proposals() {
        verify_leaf(verifier, proposal.add_proposal().key_package().leaf_node())?;
    }
    for proposal in staged.update_proposals() {
        verify_leaf(verifier, proposal.update_proposal().leaf_node())?;
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
