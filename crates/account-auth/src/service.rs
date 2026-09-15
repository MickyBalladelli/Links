use crate::{
    passkeys::{verify_assertion, verify_registration, PasskeyConfig},
    provider::{Channel, OtpProvider},
    AuthError,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use hmac::{Hmac, Mac};
use links_identity::{verify, DeviceBinding};
use links_protocol::{self, v1, validate_handle};
use links_server_store::postgres::{
    GroupKind, OrganizationControls, RelationalStore, Role,
};
use prost::Message;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, Row, Transaction};
use std::{
    net::IpAddr,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};
use uuid::Uuid;
use zeroize::{Zeroize, Zeroizing};

pub const CHALLENGE_TTL_MS: u64 = 10 * 60 * 1000;
pub const SESSION_TTL_MS: u64 = 15 * 60 * 1000;
pub const PASSKEY_CHALLENGE_TTL_MS: u64 = 10 * 60 * 1000;
pub const PRIVACY_PASS_CHALLENGE_TTL_MS: u64 = 10 * 60 * 1000;
pub const PROOF_OF_WORK_CHALLENGE_TTL_MS: u64 = 5 * 60 * 1000;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AccountAuthMode {
    Production,
    LoopbackUsernameDev,
}
pub trait Clock: Send + Sync {
    fn now_ms(&self) -> u64;
}

/// Trusted verification provider boundary. Production providers should keep
/// the authority private key in an HSM; the account service only asks it to
/// sign the canonical public badge transcript.
pub trait VerificationSigner: Send + Sync {
    fn public_key(&self) -> Result<[u8; 32], AuthError>;
    fn sign(&self, transcript: &[u8]) -> Result<[u8; 64], AuthError>;
}

impl VerificationSigner for links_identity::IdentitySeed {
    fn public_key(&self) -> Result<[u8; 32], AuthError> {
        Ok(links_identity::IdentitySeed::public_key(self))
    }

    fn sign(&self, transcript: &[u8]) -> Result<[u8; 64], AuthError> {
        Ok(links_identity::IdentitySeed::sign(self, transcript))
    }
}

pub struct SystemClock;
impl Clock for SystemClock {
    fn now_ms(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0)
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StartRequest {
    pub phone: String,
    pub channel: Channel,
    pub device_id: Uuid,
    pub mls_node_id: Uuid,
    pub public_key: String,
    pub signature: String,
}
impl Drop for StartRequest {
    fn drop(&mut self) {
        self.phone.zeroize();
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UsernameRegistrationRequest {
    /// Canonical handle without the display-only `@` prefix.
    pub handle: String,
    pub device_id: Uuid,
    pub mls_node_id: Uuid,
    pub public_key: String,
    pub nonce: String,
    pub signature: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UsernameLoginRequest {
    /// Canonical handle without the display-only `@` prefix.
    pub handle: String,
    pub device_id: Uuid,
    pub mls_node_id: Uuid,
    pub public_key: String,
    pub nonce: String,
    pub signature: String,
}

#[derive(Serialize)]
pub struct UsernameAuthResponse {
    pub session: Session,
    pub handle: String,
    pub mls_credential: String,
}

#[derive(Serialize)]
pub struct UsernameDirectoryResponse {
    pub handle: String,
    pub user_id: Uuid,
    pub devices: Vec<UsernameDirectoryDeviceResponse>,
    pub verification_badge: Option<String>,
}

#[derive(Serialize)]
pub struct UsernameDirectoryDeviceResponse {
    pub device_id: Uuid,
    pub mls_node_id: Uuid,
    pub did: String,
    pub identity_public_key: String,
    pub mls_credential: String,
    pub delegation_role: String,
    pub delegation_certificate: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContactPsiQueryRequest {
    pub protocol_version: u32,
    pub blinded_inputs: Vec<String>,
}

#[derive(Serialize)]
pub struct ContactPsiEvaluationResponse {
    pub evaluated_point: String,
    pub proof: String,
}

#[derive(Serialize)]
pub struct ContactPsiQueryResponse {
    pub protocol_version: u32,
    pub evaluations: Vec<ContactPsiEvaluationResponse>,
}

#[derive(Serialize)]
pub struct ContactPsiParametersResponse {
    pub protocol_version: u32,
    pub server_public_key: String,
    pub filter: String,
    pub filter_hash_count: u8,
    pub filter_item_count: u64,
}

#[derive(Serialize)]
pub struct PrivacyPassParametersResponse {
    pub protocol_version: u32,
    pub token_type: u16,
    pub public_key: String,
    pub token_key_id: String,
}

#[derive(Serialize)]
pub struct PrivacyPassChallengeResponse {
    pub protocol_version: u32,
    pub token_type: u16,
    pub challenge: String,
    pub expires_at_ms: u64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrivacyPassIssueRequest {
    pub protocol_version: u32,
    pub token_type: u16,
    pub truncated_token_key_id: u8,
    pub blinded_message: String,
}

#[derive(Serialize)]
pub struct PrivacyPassIssueResponse {
    pub protocol_version: u32,
    pub token_type: u16,
    pub evaluated_message: String,
    pub proof: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrivacyPassRedeemRequest {
    pub protocol_version: u32,
    pub token: String,
    pub challenge: String,
}

#[derive(Serialize)]
pub struct PrivacyPassRedeemResponse {
    pub protocol_version: u32,
    pub accepted: bool,
}

#[derive(Serialize)]
pub struct ChatProofOfWorkChallengeResponse {
    pub protocol_version: u32,
    pub challenge: String,
    pub difficulty_bits: u8,
    pub expires_at_ms: u64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChatProofOfWorkVerifyRequest {
    pub protocol_version: u32,
    pub challenge: String,
    pub nonce: u64,
}

#[derive(Serialize)]
pub struct ChatProofOfWorkVerifyResponse {
    pub protocol_version: u32,
    pub accepted: bool,
}
#[derive(Serialize, Deserialize)]
pub struct Challenge {
    pub challenge_id: Uuid,
    pub user_id: Uuid,
    pub device_id: Uuid,
    pub mls_node_id: Uuid,
    pub public_key: String,
    pub nonce: String,
    pub expires_at_ms: u64,
    pub mls_credential: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FinishRequest {
    pub challenge_id: Uuid,
    pub code: String,
    pub signature: String,
}
impl Drop for FinishRequest {
    fn drop(&mut self) {
        self.code.zeroize();
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeviceRegistrationRequest {
    pub device_id: Uuid,
    pub mls_node_id: Uuid,
    pub public_key: String,
    pub nonce: String,
    pub signature: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DelegatedDeviceRegistrationRequest {
    pub certificate: String,
    pub nonce: String,
    pub signature: String,
}

#[derive(Serialize)]
pub struct DeviceRegistrationResponse {
    pub user_id: Uuid,
    pub device_id: Uuid,
    pub mls_node_id: Uuid,
    pub public_key: String,
    pub mls_credential: String,
    pub delegation_certificate: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateGroupRequest {
    pub group_id: Uuid,
    #[serde(default)]
    pub kind: Option<String>,
}

#[derive(Serialize)]
pub struct GroupResponse {
    pub group_id: Uuid,
    pub owner_id: Uuid,
    pub kind: String,
}

#[derive(Serialize)]
pub struct GroupMemberResponse {
    pub user_id: Uuid,
    pub role: String,
}

#[derive(Serialize)]
pub struct GroupMembersResponse {
    pub group_id: Uuid,
    pub members: Vec<GroupMemberResponse>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OrganizationControlsRequest {
    pub mini_apps_enabled: bool,
    pub bots_enabled: bool,
}

#[derive(Serialize)]
pub struct OrganizationControlsResponse {
    pub organization_id: Uuid,
    pub mini_apps_enabled: bool,
    pub bots_enabled: bool,
    pub revision: u64,
}

impl From<OrganizationControls> for OrganizationControlsResponse {
    fn from(controls: OrganizationControls) -> Self {
        Self {
            organization_id: controls.organization_id,
            mini_apps_enabled: controls.mini_apps_enabled,
            bots_enabled: controls.bots_enabled,
            revision: controls.revision,
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetGroupRoleRequest {
    pub role: String,
}

#[derive(Serialize)]
pub struct DeviceRevocationResponse {
    pub device_id: Uuid,
    pub revoked: bool,
}
#[derive(Serialize, Deserialize)]
pub struct Session {
    pub access_token: String,
    pub expires_at_ms: u64,
    pub user_id: Uuid,
    pub device_id: Uuid,
}
impl Drop for Session {
    fn drop(&mut self) {
        self.access_token.zeroize();
    }
}
#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct AuthenticatedAccount {
    pub user_id: Uuid,
    pub device_id: Uuid,
}

#[derive(Serialize)]
pub struct PasskeyOptions {
    pub challenge_id: Uuid,
    pub challenge: String,
    pub rp_id: String,
    pub user_id: Uuid,
    pub expires_at_ms: u64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PasskeyRegistrationFinishRequest {
    pub challenge_id: Uuid,
    pub credential_id: String,
    pub client_data_json: String,
    pub attestation_object: String,
}

#[derive(Serialize)]
pub struct RegisteredPasskeyResponse {
    pub credential_id: String,
    pub sign_count: u32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PasskeyAssertionFinishRequest {
    pub challenge_id: Uuid,
    pub credential_id: String,
    pub client_data_json: String,
    pub authenticator_data: String,
    pub signature: String,
}

#[derive(Serialize)]
pub struct PasskeyAssertionResponse {
    pub credential_id: String,
    pub sign_count: u32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EncryptedKeyBackupRequest {
    pub backup_id: Uuid,
    pub device_id: Uuid,
    pub credential_id: String,
    pub encrypted_envelope: String,
}

#[derive(Serialize)]
pub struct EncryptedKeyBackupResponse {
    pub backup_id: Uuid,
    pub device_id: Uuid,
    pub credential_id: String,
    pub encrypted_envelope: String,
}

pub fn validate_phone(phone: &str) -> Result<(), AuthError> {
    let digits = phone.strip_prefix('+').ok_or(AuthError::Invalid)?;
    if !(8..=15).contains(&digits.len())
        || digits.starts_with('0')
        || !digits.bytes().all(|b| b.is_ascii_digit())
    {
        return Err(AuthError::Invalid);
    }
    Ok(())
}
pub fn encode(bytes: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(bytes)
}
fn decode<const N: usize>(value: &str) -> Result<[u8; N], AuthError> {
    if value.len() > N * 2 {
        return Err(AuthError::Invalid);
    }
    URL_SAFE_NO_PAD
        .decode(value)
        .map_err(|_| AuthError::Invalid)?
        .try_into()
        .map_err(|_| AuthError::Invalid)
}
fn decode_blob(value: &str, max: usize) -> Result<Vec<u8>, AuthError> {
    if value.len() > max.saturating_mul(2) {
        return Err(AuthError::Invalid);
    }
    let bytes = URL_SAFE_NO_PAD
        .decode(value)
        .map_err(|_| AuthError::Invalid)?;
    if bytes.is_empty() || bytes.len() > max {
        return Err(AuthError::Invalid);
    }
    Ok(bytes)
}
fn random() -> Result<[u8; 32], AuthError> {
    let mut bytes = [0; 32];
    getrandom::getrandom(&mut bytes).map_err(|_| AuthError::Unavailable)?;
    Ok(bytes)
}

fn random_wide() -> Result<[u8; 64], AuthError> {
    let mut bytes = [0; 64];
    getrandom::getrandom(&mut bytes).map_err(|_| AuthError::Unavailable)?;
    Ok(bytes)
}

fn random_scalar_bytes() -> Result<[u8; links_protocol::privacy_pass::SCALAR_BYTES], AuthError> {
    let mut bytes = [0u8; links_protocol::privacy_pass::SCALAR_BYTES];
    getrandom::getrandom(&mut bytes).map_err(|_| AuthError::Unavailable)?;
    Ok(bytes)
}

fn derive_privacy_pass_key(seed: &[u8; 32]) -> Zeroizing<[u8; 32]> {
    let mut hasher = Sha256::new();
    hasher.update(b"links/privacy-pass/issuer-seed/v1\0");
    hasher.update(seed);
    Zeroizing::new(hasher.finalize().into())
}

pub struct AccountAuth {
    pool: PgPool,
    provider: Arc<dyn OtpProvider>,
    phone_lookup_key: Zeroizing<[u8; 32]>,
    privacy_pass_key: Zeroizing<[u8; 32]>,
    clock: Arc<dyn Clock>,
    passkey: Option<PasskeyConfig>,
    mode: AccountAuthMode,
}
impl AccountAuth {
    pub fn new(
        pool: PgPool,
        provider: Arc<dyn OtpProvider>,
        phone_lookup_key: Zeroizing<[u8; 32]>,
        clock: Arc<dyn Clock>,
    ) -> Result<Self, AuthError> {
        Self::new_with_mode(
            pool,
            provider,
            phone_lookup_key,
            clock,
            None,
            AccountAuthMode::Production,
        )
    }

    pub fn new_with_passkey(
        pool: PgPool,
        provider: Arc<dyn OtpProvider>,
        phone_lookup_key: Zeroizing<[u8; 32]>,
        clock: Arc<dyn Clock>,
        passkey: PasskeyConfig,
    ) -> Result<Self, AuthError> {
        Self::new_with_mode(
            pool,
            provider,
            phone_lookup_key,
            clock,
            Some(passkey),
            AccountAuthMode::Production,
        )
    }

    #[cfg(debug_assertions)]
    pub fn new_loopback_username_dev(
        pool: PgPool,
        phone_lookup_key: Zeroizing<[u8; 32]>,
        clock: Arc<dyn Clock>,
    ) -> Result<Self, AuthError> {
        Self::new_with_mode(
            pool,
            Arc::new(crate::provider::DisabledOtpProvider),
            phone_lookup_key,
            clock,
            None,
            AccountAuthMode::LoopbackUsernameDev,
        )
    }

    #[cfg(debug_assertions)]
    pub fn new_loopback_username_dev_with_passkey(
        pool: PgPool,
        phone_lookup_key: Zeroizing<[u8; 32]>,
        clock: Arc<dyn Clock>,
        passkey: PasskeyConfig,
    ) -> Result<Self, AuthError> {
        Self::new_with_mode(
            pool,
            Arc::new(crate::provider::DisabledOtpProvider),
            phone_lookup_key,
            clock,
            Some(passkey),
            AccountAuthMode::LoopbackUsernameDev,
        )
    }

    fn new_with_mode(
        pool: PgPool,
        provider: Arc<dyn OtpProvider>,
        phone_lookup_key: Zeroizing<[u8; 32]>,
        clock: Arc<dyn Clock>,
        passkey: Option<PasskeyConfig>,
        mode: AccountAuthMode,
    ) -> Result<Self, AuthError> {
        if *phone_lookup_key == [0; 32] {
            return Err(AuthError::Invalid);
        }
        let privacy_pass_key = derive_privacy_pass_key(&phone_lookup_key);
        Ok(Self {
            pool,
            provider,
            phone_lookup_key,
            privacy_pass_key,
            clock,
            passkey,
            mode,
        })
    }

    pub fn mode(&self) -> AccountAuthMode {
        self.mode
    }

    pub fn is_loopback_username_dev(&self) -> bool {
        self.mode == AccountAuthMode::LoopbackUsernameDev
    }

    fn passkey_config(&self) -> Result<&PasskeyConfig, AuthError> {
        self.passkey.as_ref().ok_or(AuthError::Unavailable)
    }
    fn now(&self) -> Result<i64, AuthError> {
        let now = self.clock.now_ms();
        if now == 0 || now > i64::MAX as u64 - SESSION_TTL_MS {
            return Err(AuthError::Unavailable);
        }
        Ok(now as i64)
    }

    async fn enforce_username_rate_limits(
        &self,
        handle: &str,
        peer_ip: IpAddr,
        now: i64,
    ) -> Result<(), AuthError> {
        let handle_minute_key =
            self.digest(b"links/username-handle-minute/v1\0", handle.as_bytes());
        let handle_hour_key = self.digest(b"links/username-handle-hour/v1\0", handle.as_bytes());
        let ip_key = self.digest(b"links/username-ip/v1\0", peer_ip.to_string().as_bytes());
        let mut tx = self.pool.begin().await?;
        let mut limited = false;
        for (key, window, limit) in [
            (handle_minute_key, 60_000_i64, 3_i32),
            (handle_hour_key, 3_600_000_i64, 10_i32),
            (ip_key, 3_600_000_i64, 50_i32),
        ] {
            let count: Option<i32> = sqlx::query_scalar("INSERT INTO auth_rate_limits (key_hash,window_start_ms,attempts) VALUES ($1,$2,1) ON CONFLICT (key_hash) DO UPDATE SET attempts=CASE WHEN auth_rate_limits.window_start_ms <= $2-$3 THEN 1 ELSE auth_rate_limits.attempts+1 END, window_start_ms=CASE WHEN auth_rate_limits.window_start_ms <= $2-$3 THEN $2 ELSE auth_rate_limits.window_start_ms END WHERE auth_rate_limits.window_start_ms <= $2-$3 OR auth_rate_limits.attempts < $4 RETURNING attempts")
                .bind(key.as_slice())
                .bind(now)
                .bind(window)
                .bind(limit)
                .fetch_optional(&mut *tx)
                .await?;
            if count.is_none() {
                limited = true;
                break;
            }
        }
        tx.commit().await?;
        if limited {
            return Err(AuthError::RateLimited);
        }
        Ok(())
    }

    async fn enforce_directory_rate_limits(
        &self,
        handle: &str,
        peer_ip: IpAddr,
        now: i64,
    ) -> Result<(), AuthError> {
        let handle_minute_key =
            self.digest(b"links/directory-handle-minute/v1\0", handle.as_bytes());
        let handle_hour_key = self.digest(b"links/directory-handle-hour/v1\0", handle.as_bytes());
        let ip_key = self.digest(b"links/directory-ip/v1\0", peer_ip.to_string().as_bytes());
        let mut tx = self.pool.begin().await?;
        let mut limited = false;
        for (key, window, limit) in [
            (handle_minute_key, 60_000_i64, 60_i32),
            (handle_hour_key, 3_600_000_i64, 1_000_i32),
            (ip_key, 3_600_000_i64, 300_i32),
        ] {
            let count: Option<i32> = sqlx::query_scalar("INSERT INTO auth_rate_limits (key_hash,window_start_ms,attempts) VALUES ($1,$2,1) ON CONFLICT (key_hash) DO UPDATE SET attempts=CASE WHEN auth_rate_limits.window_start_ms <= $2-$3 THEN 1 ELSE auth_rate_limits.attempts+1 END, window_start_ms=CASE WHEN auth_rate_limits.window_start_ms <= $2-$3 THEN $2 ELSE auth_rate_limits.window_start_ms END WHERE auth_rate_limits.window_start_ms <= $2-$3 OR auth_rate_limits.attempts < $4 RETURNING attempts")
                .bind(key.as_slice())
                .bind(now)
                .bind(window)
                .bind(limit)
                .fetch_optional(&mut *tx)
                .await?;
            if count.is_none() {
                limited = true;
                break;
            }
        }
        tx.commit().await?;
        if limited {
            return Err(AuthError::RateLimited);
        }
        Ok(())
    }

    async fn enforce_contact_psi_rate_limits(
        &self,
        user_id: Uuid,
        peer_ip: IpAddr,
        now: i64,
        batch_size: usize,
    ) -> Result<(), AuthError> {
        let batch_cost = i32::try_from(batch_size).map_err(|_| AuthError::Invalid)?;
        let user_minute_key =
            self.digest(b"links/contact-psi-user-minute/v1\0", user_id.as_bytes());
        let user_hour_key = self.digest(b"links/contact-psi-user-hour/v1\0", user_id.as_bytes());
        let user_items_key = self.digest(
            b"links/contact-psi-user-items-hour/v1\0",
            user_id.as_bytes(),
        );
        let ip_key = self.digest(
            b"links/contact-psi-ip-hour/v1\0",
            peer_ip.to_string().as_bytes(),
        );
        let ip_items_key = self.digest(
            b"links/contact-psi-ip-items-hour/v1\0",
            peer_ip.to_string().as_bytes(),
        );
        let mut tx = self.pool.begin().await?;
        let mut limited = false;
        for (key, window, limit, cost) in [
            (user_minute_key, 60_000_i64, 10_i32, 1_i32),
            (user_hour_key, 3_600_000_i64, 60_i32, 1_i32),
            (user_items_key, 3_600_000_i64, 10_000_i32, batch_cost),
            (ip_key, 3_600_000_i64, 100_i32, 1_i32),
            (ip_items_key, 3_600_000_i64, 20_000_i32, batch_cost),
        ] {
            let count: Option<i32> = sqlx::query_scalar("INSERT INTO auth_rate_limits (key_hash,window_start_ms,attempts) VALUES ($1,$2,$5) ON CONFLICT (key_hash) DO UPDATE SET attempts=CASE WHEN auth_rate_limits.window_start_ms <= $2-$3 THEN $5 ELSE auth_rate_limits.attempts+$5 END, window_start_ms=CASE WHEN auth_rate_limits.window_start_ms <= $2-$3 THEN $2 ELSE auth_rate_limits.window_start_ms END WHERE auth_rate_limits.window_start_ms <= $2-$3 OR auth_rate_limits.attempts+$5 <= $4 RETURNING attempts")
                .bind(key.as_slice())
                .bind(now)
                .bind(window)
                .bind(limit)
                .bind(cost)
                .fetch_optional(&mut *tx)
                .await?;
            if count.is_none() {
                limited = true;
                break;
            }
        }
        tx.commit().await?;
        if limited {
            return Err(AuthError::RateLimited);
        }
        Ok(())
    }

    async fn enforce_rate_limit_set<const N: usize>(
        &self,
        limits: [([u8; 32], i64, i32); N],
    ) -> Result<(), AuthError> {
        let mut tx = self.pool.begin().await?;
        let mut limited = false;
        for (key, window, limit) in limits {
            let count: Option<i32> = sqlx::query_scalar("INSERT INTO auth_rate_limits (key_hash,window_start_ms,attempts) VALUES ($1,$2,1) ON CONFLICT (key_hash) DO UPDATE SET attempts=CASE WHEN auth_rate_limits.window_start_ms <= $2-$3 THEN 1 ELSE auth_rate_limits.attempts+1 END, window_start_ms=CASE WHEN auth_rate_limits.window_start_ms <= $2-$3 THEN $2 ELSE auth_rate_limits.window_start_ms END WHERE auth_rate_limits.window_start_ms <= $2-$3 OR auth_rate_limits.attempts < $4 RETURNING attempts")
                .bind(key.as_slice())
                .bind(self.now()?)
                .bind(window)
                .bind(limit)
                .fetch_optional(&mut *tx)
                .await?;
            if count.is_none() {
                limited = true;
                break;
            }
        }
        tx.commit().await?;
        if limited {
            return Err(AuthError::RateLimited);
        }
        Ok(())
    }

    async fn enforce_privacy_pass_issue_rate_limits(
        &self,
        user_id: Uuid,
        peer_ip: IpAddr,
    ) -> Result<(), AuthError> {
        self.enforce_rate_limit_set([
            (
                self.digest(
                    b"links/privacy-pass-issue-user-minute/v1\0",
                    user_id.as_bytes(),
                ),
                60_000,
                5,
            ),
            (
                self.digest(
                    b"links/privacy-pass-issue-user-hour/v1\0",
                    user_id.as_bytes(),
                ),
                3_600_000,
                30,
            ),
            (
                self.digest(
                    b"links/privacy-pass-issue-ip-hour/v1\0",
                    peer_ip.to_string().as_bytes(),
                ),
                3_600_000,
                100,
            ),
        ])
        .await
    }

    async fn enforce_privacy_pass_ip_rate_limit(
        &self,
        domain: &'static [u8],
        peer_ip: IpAddr,
        limit: i32,
    ) -> Result<(), AuthError> {
        self.enforce_rate_limit_set([(
            self.digest(domain, peer_ip.to_string().as_bytes()),
            3_600_000,
            limit,
        )])
        .await
    }

    async fn enforce_chat_pow_rate_limits(
        &self,
        domain: &'static [u8],
        user_id: Uuid,
        peer_ip: IpAddr,
        user_limit: i32,
        ip_limit: i32,
    ) -> Result<(), AuthError> {
        self.enforce_rate_limit_set([
            (
                self.digest(domain, user_id.as_bytes()),
                3_600_000,
                user_limit,
            ),
            (
                self.digest(
                    b"links/chat-request-pow-ip-hour/v1\0",
                    peer_ip.to_string().as_bytes(),
                ),
                3_600_000,
                ip_limit,
            ),
        ])
        .await
    }

    async fn require_unverified_account(
        &self,
        account: &AuthenticatedAccount,
    ) -> Result<(), AuthError> {
        let unverified: Option<bool> = sqlx::query_scalar(
            "SELECT account_kind='pseudonymous' FROM accounts WHERE user_id=$1 AND disabled_at IS NULL",
        )
        .bind(account.user_id)
        .fetch_optional(&self.pool)
        .await?;
        if unverified == Some(true) {
            Ok(())
        } else {
            Err(AuthError::Denied)
        }
    }

    async fn issue_session(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        user_id: Uuid,
        device_id: Uuid,
    ) -> Result<Session, AuthError> {
        let token_bytes = Zeroizing::new(random()?);
        let token_hash = Sha256::digest(token_bytes.as_slice());
        let expires_at_ms = self.now()? + SESSION_TTL_MS as i64;
        sqlx::query("INSERT INTO auth_sessions (token_hash,user_id,device_id,expires_at_ms) VALUES ($1,$2,$3,$4)")
            .bind(token_hash.as_slice())
            .bind(user_id)
            .bind(device_id)
            .bind(expires_at_ms)
            .execute(&mut **tx)
            .await?;
        Ok(Session {
            access_token: encode(token_bytes.as_slice()),
            expires_at_ms: expires_at_ms as u64,
            user_id,
            device_id,
        })
    }

    fn digest(&self, domain: &[u8], value: &[u8]) -> [u8; 32] {
        let mut mac = Hmac::<Sha256>::new_from_slice(self.phone_lookup_key.as_ref())
            .expect("fixed length HMAC key");
        mac.update(domain);
        mac.update(value);
        mac.finalize().into_bytes().into()
    }

    /// Create a pseudonymous account from a canonical handle and a device key.
    /// The handle, account, device, MLS credential, and first session commit
    /// atomically. No phone number or phone-derived subject is stored.
    pub async fn register_username(
        &self,
        request: UsernameRegistrationRequest,
        peer_ip: IpAddr,
    ) -> Result<UsernameAuthResponse, AuthError> {
        validate_handle(&request.handle).map_err(|_| AuthError::Invalid)?;
        if request.device_id.is_nil() || request.mls_node_id.is_nil() {
            return Err(AuthError::Invalid);
        }
        let public_key = decode::<32>(&request.public_key)?;
        let nonce = decode::<32>(&request.nonce)?;
        let signature = decode::<64>(&request.signature)?;
        let transcript = links_identity::username_registration_transcript(
            &request.handle,
            request.device_id,
            request.mls_node_id,
            &public_key,
            &nonce,
        )?;
        verify(&public_key, &transcript, &signature)?;
        let now = self.now()?;
        self.enforce_username_rate_limits(&request.handle, peer_ip, now)
            .await?;

        let user_id = Uuid::new_v4();
        let binding = DeviceBinding {
            user_id,
            device_id: request.device_id,
            mls_node_id: request.mls_node_id,
            public_key,
        };
        let credential = binding.mls_credential()?;
        let mut tx = self.pool.begin().await?;
        sqlx::query("INSERT INTO accounts (user_id,auth_subject_hash,account_kind) VALUES ($1,$2,'pseudonymous')")
            .bind(user_id)
            .bind(Option::<Vec<u8>>::None)
            .execute(&mut *tx)
            .await?;
        sqlx::query("INSERT INTO handles (handle,user_id) VALUES ($1,$2)")
            .bind(&request.handle)
            .bind(user_id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("INSERT INTO devices (device_id,user_id,mls_node_id,identity_public_key,mls_credential) VALUES ($1,$2,$3,$4,$5)")
            .bind(request.device_id)
            .bind(user_id)
            .bind(request.mls_node_id)
            .bind(binding.public_key.as_slice())
            .bind(&credential)
            .execute(&mut *tx)
            .await?;
        let session = self
            .issue_session(&mut tx, user_id, request.device_id)
            .await?;
        tx.commit().await?;
        Ok(UsernameAuthResponse {
            session,
            handle: request.handle,
            mls_credential: encode(&credential),
        })
    }

    /// Log in to a username account by proving possession of the registered
    /// device key. This keeps phone OTP out of the pseudonymous path.
    pub async fn login_username(
        &self,
        request: UsernameLoginRequest,
        peer_ip: IpAddr,
    ) -> Result<UsernameAuthResponse, AuthError> {
        validate_handle(&request.handle).map_err(|_| AuthError::Invalid)?;
        if request.device_id.is_nil() || request.mls_node_id.is_nil() {
            return Err(AuthError::Invalid);
        }
        let public_key = decode::<32>(&request.public_key)?;
        let nonce = decode::<32>(&request.nonce)?;
        let signature = decode::<64>(&request.signature)?;
        let now = self.now()?;
        self.enforce_username_rate_limits(&request.handle, peer_ip, now)
            .await?;
        let mut tx = self.pool.begin().await?;
        let row = sqlx::query("SELECT a.user_id,d.mls_node_id,d.identity_public_key,d.mls_credential FROM handles h JOIN accounts a USING (user_id) JOIN devices d USING (user_id) WHERE h.handle=$1 AND d.device_id=$2 AND a.account_kind='pseudonymous' AND a.disabled_at IS NULL AND d.revoked_at IS NULL FOR SHARE OF a,d")
            .bind(&request.handle)
            .bind(request.device_id)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(AuthError::Denied)?;
        let user_id: Uuid = row.get("user_id");
        let stored_node: Uuid = row.get("mls_node_id");
        let stored_public: [u8; 32] = row
            .get::<Vec<u8>, _>("identity_public_key")
            .try_into()
            .map_err(|_| AuthError::Unavailable)?;
        if stored_node != request.mls_node_id || stored_public != public_key {
            return Err(AuthError::Denied);
        }
        let binding = DeviceBinding {
            user_id,
            device_id: request.device_id,
            mls_node_id: request.mls_node_id,
            public_key,
        };
        let credential = binding.mls_credential()?;
        if row.get::<Vec<u8>, _>("mls_credential") != credential {
            return Err(AuthError::Unavailable);
        }
        let transcript = links_identity::username_login_transcript(
            &request.handle,
            request.device_id,
            request.mls_node_id,
            &public_key,
            &nonce,
        )?;
        verify(&public_key, &transcript, &signature)?;
        let session = self
            .issue_session(&mut tx, user_id, request.device_id)
            .await?;
        tx.commit().await?;
        Ok(UsernameAuthResponse {
            session,
            handle: request.handle,
            mls_credential: encode(&credential),
        })
    }

    /// Return the active public device directory for a canonical username.
    /// Pre-key bundles stay behind the authenticated claim endpoint so a
    /// directory read never consumes one-time pre-keys.
    pub async fn lookup_username_directory(
        &self,
        handle: &str,
        peer_ip: IpAddr,
    ) -> Result<Option<UsernameDirectoryResponse>, AuthError> {
        validate_handle(handle).map_err(|_| AuthError::Invalid)?;
        let now = self.now()?;
        self.enforce_directory_rate_limits(handle, peer_ip, now)
            .await?;
        let directory = RelationalStore::from_pool(self.pool.clone())
            .lookup_handle_directory(handle)
            .await?;
        let Some(directory) = directory else {
            return Ok(None);
        };
        let mut devices = Vec::with_capacity(directory.devices.len());
        for device in directory.devices {
            let public_key: [u8; 32] = device
                .identity_public_key
                .as_slice()
                .try_into()
                .map_err(|_| AuthError::Unavailable)?;
            let binding = DeviceBinding {
                user_id: directory.user_id,
                device_id: device.device_id,
                mls_node_id: device.mls_node_id,
                public_key,
            };
            if binding.mls_credential()? != device.mls_credential {
                return Err(AuthError::Unavailable);
            }
            devices.push(UsernameDirectoryDeviceResponse {
                device_id: device.device_id,
                mls_node_id: device.mls_node_id,
                did: device.did,
                identity_public_key: encode(&public_key),
                mls_credential: encode(&device.mls_credential),
                delegation_role: device.delegation_role,
                delegation_certificate: device.delegation_certificate.as_deref().map(encode),
            });
        }
        Ok(Some(UsernameDirectoryResponse {
            handle: handle.to_owned(),
            user_id: directory.user_id,
            devices,
            verification_badge: directory.verification_badge.as_deref().map(encode),
        }))
    }

    /// Issue a public verification badge after an external verification
    /// workflow has approved the account. No verification evidence enters the
    /// database; only the authority-signed claim is retained.
    pub async fn issue_verification_badge(
        &self,
        subject_user_id: Uuid,
        badge_id: Uuid,
        subject_handle: Option<String>,
        badge_kind: u32,
        issued_at_ms: u64,
        expires_at_ms: u64,
        authority: &dyn VerificationSigner,
    ) -> Result<v1::VerificationBadge, AuthError> {
        if subject_user_id.is_nil() || badge_id.is_nil() {
            return Err(AuthError::Invalid);
        }
        let now = self.now()? as u64;
        if issued_at_ms > now || expires_at_ms <= now {
            return Err(AuthError::Invalid);
        }
        let active: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM accounts WHERE user_id=$1 AND disabled_at IS NULL)",
        )
        .bind(subject_user_id)
        .fetch_one(&self.pool)
        .await?;
        if !active {
            return Err(AuthError::Denied);
        }
        if let Some(handle) = subject_handle.as_deref() {
            validate_handle(handle).map_err(|_| AuthError::Invalid)?;
            let owned: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM handles WHERE handle=$1 AND user_id=$2)",
            )
            .bind(handle)
            .bind(subject_user_id)
            .fetch_one(&self.pool)
            .await?;
            if !owned {
                return Err(AuthError::Invalid);
            }
        }
        let badge = v1::VerificationBadge {
            protocol_version: links_protocol::VERSION,
            badge_id: badge_id.to_string(),
            subject_user_id: subject_user_id.to_string(),
            subject_handle,
            issuer_public_key: authority.public_key()?.to_vec(),
            badge_kind,
            issued_at_ms,
            expires_at_ms,
            signature: vec![0; 64],
        };
        links_protocol::validate_verification_badge(&badge)
            .map_err(|_| AuthError::Invalid)?;
        let signature = authority.sign(
            &links_identity::verification_badge_transcript(&badge)
                .map_err(|_| AuthError::Invalid)?,
        )?;
        let mut signed = badge;
        signed.signature = signature.to_vec();
        links_protocol::validate_verification_badge(&signed)
            .map_err(|_| AuthError::Invalid)?;
        RelationalStore::from_pool(self.pool.clone())
            .put_verification_badge(subject_user_id, &signed)
            .await?;
        Ok(signed)
    }

    /// Revoke the current badge. Directory clients treat missing badges as
    /// unverified and must not cache a prior signed claim past its expiry.
    pub async fn revoke_verification_badge(&self, subject_user_id: Uuid) -> Result<(), AuthError> {
        if subject_user_id.is_nil() {
            return Err(AuthError::Invalid);
        }
        RelationalStore::from_pool(self.pool.clone())
            .clear_verification_badge(subject_user_id)
            .await?;
        Ok(())
    }

    pub async fn start(
        &self,
        request: StartRequest,
        peer_ip: IpAddr,
    ) -> Result<Challenge, AuthError> {
        if self.is_loopback_username_dev() {
            return Err(AuthError::Unavailable);
        }
        validate_phone(&request.phone)?;
        if request.device_id.is_nil() || request.mls_node_id.is_nil() {
            return Err(AuthError::Invalid);
        }
        let public_key = decode::<32>(&request.public_key)?;
        let start_proof = links_identity::phone_auth_transcript(
            &request.phone,
            request.channel.as_str(),
            request.device_id,
            request.mls_node_id,
            &public_key,
        )?;
        verify(
            &public_key,
            &start_proof,
            &decode::<64>(&request.signature)?,
        )?;
        let now = self.now()?;
        let subject = self.digest(b"links/phone-lookup/v1\0", request.phone.as_bytes());
        let contact_directory_token =
            links_protocol::contact_psi::directory_token(&request.phone, &*self.phone_lookup_key)
                .map_err(|_| AuthError::Unavailable)?;
        let cooldown = self.digest(b"links/otp-cooldown/v1\0", &subject);
        let per_phone = self.digest(b"links/otp-phone-hour/v1\0", &subject);
        let per_ip = self.digest(b"links/otp-ip-hour/v1\0", peer_ip.to_string().as_bytes());
        let mut tx = self.pool.begin().await?;
        for (key, window, limit) in [
            (cooldown, 60_000_i64, 1_i32),
            (per_phone, 3_600_000, 5),
            (per_ip, 3_600_000, 20),
        ] {
            let count: Option<i32> = sqlx::query_scalar("INSERT INTO auth_rate_limits (key_hash,window_start_ms,attempts) VALUES ($1,$2,1) ON CONFLICT (key_hash) DO UPDATE SET attempts=CASE WHEN auth_rate_limits.window_start_ms <= $2-$3 THEN 1 ELSE auth_rate_limits.attempts+1 END, window_start_ms=CASE WHEN auth_rate_limits.window_start_ms <= $2-$3 THEN $2 ELSE auth_rate_limits.window_start_ms END WHERE auth_rate_limits.window_start_ms <= $2-$3 OR auth_rate_limits.attempts < $4 RETURNING attempts")
                .bind(key.as_slice()).bind(now).bind(window).bind(limit).fetch_optional(&mut *tx).await?;
            if count.is_none() {
                return Err(AuthError::RateLimited);
            }
        }
        let account = sqlx::query("SELECT user_id, disabled_at IS NULL AS active FROM accounts WHERE auth_subject_hash=$1")
            .bind(subject.as_slice()).fetch_optional(&mut *tx).await?;
        let enrolling = account.is_none();
        let mut permitted = enrolling;
        let mut user_id = Uuid::new_v4();
        if let Some(account) = account {
            let existing_user: Uuid = account.get("user_id");
            let device: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM devices WHERE user_id=$1 AND device_id=$2 AND mls_node_id=$3 AND identity_public_key=$4 AND revoked_at IS NULL)")
                .bind(existing_user).bind(request.device_id).bind(request.mls_node_id).bind(public_key.as_slice()).fetch_one(&mut *tx).await?;
            permitted = account.get::<bool, _>("active") && device;
            if permitted {
                user_id = existing_user;
            }
            // Ineligible devices get an indistinguishable provisional challenge;
            // OTP alone may not replace an existing account's device keys.
        }
        let binding = DeviceBinding {
            user_id,
            device_id: request.device_id,
            mls_node_id: request.mls_node_id,
            public_key,
        };
        let credential = binding.mls_credential()?;
        let id = Uuid::new_v4();
        let nonce = random()?;
        let expires = now + CHALLENGE_TTL_MS as i64;
        sqlx::query("UPDATE auth_challenges SET state='failed' WHERE subject_hash=$1 AND state IN ('reserved','pending','checking')")
            .bind(subject.as_slice()).execute(&mut *tx).await?;
        sqlx::query("INSERT INTO auth_challenges (challenge_id,subject_hash,contact_directory_token,user_id,device_id,mls_node_id,public_key,nonce,enrolling,permitted,state,expires_at_ms) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,'reserved',$11)")
            .bind(id).bind(subject.as_slice()).bind(contact_directory_token.as_slice()).bind(user_id).bind(request.device_id).bind(request.mls_node_id).bind(public_key.as_slice()).bind(nonce.as_slice()).bind(enrolling).bind(permitted).bind(expires).execute(&mut *tx).await?;
        tx.commit().await?; // Consume quota before sending; outages never bypass quotas.
        let provider_sid = match self.provider.start(&request.phone, request.channel).await {
            Ok(sid) if !sid.is_empty() && sid.len() <= 128 => sid,
            _ => {
                self.fail(id).await?;
                return Err(AuthError::Unavailable);
            }
        };
        let updated = sqlx::query("UPDATE auth_challenges SET provider_sid=$2,state='pending' WHERE challenge_id=$1 AND state='reserved' AND expires_at_ms>$3")
            .bind(id).bind(provider_sid).bind(self.now()?).execute(&self.pool).await?;
        if updated.rows_affected() != 1 {
            return Err(AuthError::Denied);
        }
        Ok(Challenge {
            challenge_id: id,
            user_id,
            device_id: request.device_id,
            mls_node_id: request.mls_node_id,
            public_key: encode(&public_key),
            nonce: encode(&nonce),
            expires_at_ms: expires as u64,
            mls_credential: encode(&credential),
        })
    }
    async fn fail(&self, id: Uuid) -> Result<(), AuthError> {
        sqlx::query("UPDATE auth_challenges SET state='failed' WHERE challenge_id=$1 AND state <> 'consumed'").bind(id).execute(&self.pool).await?;
        Ok(())
    }
    pub async fn finish(&self, request: FinishRequest) -> Result<Session, AuthError> {
        if self.is_loopback_username_dev() {
            return Err(AuthError::Unavailable);
        }
        if !(6..=10).contains(&request.code.len())
            || !request.code.bytes().all(|b| b.is_ascii_digit())
        {
            return Err(AuthError::Invalid);
        }
        let signature = decode::<64>(&request.signature)?;
        let mut tx = self.pool.begin().await?;
        let row = sqlx::query("SELECT * FROM auth_challenges WHERE challenge_id=$1 FOR UPDATE")
            .bind(request.challenge_id)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(AuthError::Denied)?;
        let expires: i64 = row.get("expires_at_ms");
        if row.get::<String, _>("state") != "pending"
            || expires <= self.now()?
            || row.get::<i16, _>("attempts") >= 5
        {
            return Err(AuthError::Denied);
        }
        let binding = DeviceBinding {
            user_id: row.get("user_id"),
            device_id: row.get("device_id"),
            mls_node_id: row.get("mls_node_id"),
            public_key: row
                .get::<Vec<u8>, _>("public_key")
                .try_into()
                .map_err(|_| AuthError::Unavailable)?,
        };
        let nonce: [u8; 32] = row
            .get::<Vec<u8>, _>("nonce")
            .try_into()
            .map_err(|_| AuthError::Unavailable)?;
        verify(
            &binding.public_key,
            &binding.enrollment_transcript(request.challenge_id, &nonce, expires as u64)?,
            &signature,
        )?;
        let sid: String = row
            .get::<Option<String>, _>("provider_sid")
            .ok_or(AuthError::Unavailable)?;
        sqlx::query(
            "UPDATE auth_challenges SET state='checking',attempts=attempts+1 WHERE challenge_id=$1",
        )
        .bind(request.challenge_id)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?; // No DB transaction is held during provider HTTP.
        match self.provider.check(&sid, &request.code).await {
            Ok(true) => {}
            Ok(false) => {
                sqlx::query("UPDATE auth_challenges SET state=CASE WHEN attempts>=5 THEN 'failed' ELSE 'pending' END WHERE challenge_id=$1 AND state='checking'")
                    .bind(request.challenge_id).execute(&self.pool).await?;
                return Err(AuthError::Denied);
            }
            Err(_) => {
                self.fail(request.challenge_id).await?;
                return Err(AuthError::Unavailable);
            }
        }
        let mut tx = self.pool.begin().await?;
        let state: String = sqlx::query_scalar(
            "SELECT state FROM auth_challenges WHERE challenge_id=$1 FOR UPDATE",
        )
        .bind(request.challenge_id)
        .fetch_one(&mut *tx)
        .await?;
        if state != "checking" || expires <= self.now()? || !row.get::<bool, _>("permitted") {
            return Err(AuthError::Denied);
        }
        let subject: Vec<u8> = row.get("subject_hash");
        let contact_directory_token: Vec<u8> = row
            .get::<Option<Vec<u8>>, _>("contact_directory_token")
            .ok_or(AuthError::Unavailable)?;
        if contact_directory_token.len() != 32 {
            return Err(AuthError::Unavailable);
        }
        if row.get::<bool, _>("enrolling") {
            let inserted = sqlx::query("INSERT INTO accounts (user_id,auth_subject_hash,contact_directory_token) VALUES ($1,$2,$3) ON CONFLICT DO NOTHING")
                .bind(binding.user_id).bind(&subject).bind(&contact_directory_token).execute(&mut *tx).await?;
            if inserted.rows_affected() != 1 {
                return Err(AuthError::Denied);
            }
            sqlx::query("INSERT INTO devices (device_id,user_id,mls_node_id,identity_public_key,mls_credential) VALUES ($1,$2,$3,$4,$5)")
                .bind(binding.device_id).bind(binding.user_id).bind(binding.mls_node_id).bind(binding.public_key.as_slice()).bind(binding.mls_credential()?).execute(&mut *tx).await?;
        }
        let allowed = sqlx::query("SELECT a.user_id FROM accounts a JOIN devices d USING (user_id) WHERE a.user_id=$1 AND a.auth_subject_hash=$2 AND d.device_id=$3 AND d.mls_node_id=$4 AND d.identity_public_key=$5 AND d.mls_credential=$6 AND a.disabled_at IS NULL AND d.revoked_at IS NULL FOR SHARE OF a,d")
            .bind(binding.user_id).bind(subject).bind(binding.device_id).bind(binding.mls_node_id).bind(binding.public_key.as_slice()).bind(binding.mls_credential()?).fetch_optional(&mut *tx).await?;
        if allowed.is_none() {
            return Err(AuthError::Denied);
        }
        if !row.get::<bool, _>("enrolling") {
            let updated = sqlx::query("UPDATE accounts SET contact_directory_token=COALESCE(contact_directory_token,$2) WHERE user_id=$1 AND (contact_directory_token IS NULL OR contact_directory_token=$2)")
                .bind(binding.user_id)
                .bind(&contact_directory_token)
                .execute(&mut *tx)
                .await?;
            if updated.rows_affected() != 1 {
                return Err(AuthError::Unavailable);
            }
        }
        let token_bytes = Zeroizing::new(random()?);
        let token_hash = Sha256::digest(token_bytes.as_slice());
        let session_expires = self.now()? + SESSION_TTL_MS as i64;
        sqlx::query("INSERT INTO auth_sessions (token_hash,user_id,device_id,expires_at_ms) VALUES ($1,$2,$3,$4)")
            .bind(token_hash.as_slice()).bind(binding.user_id).bind(binding.device_id).bind(session_expires).execute(&mut *tx).await?;
        sqlx::query("UPDATE auth_challenges SET state='consumed' WHERE challenge_id=$1")
            .bind(request.challenge_id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(Session {
            access_token: encode(token_bytes.as_slice()),
            expires_at_ms: session_expires as u64,
            user_id: binding.user_id,
            device_id: binding.device_id,
        })
    }
    pub async fn authenticate(&self, token: &str) -> Result<AuthenticatedAccount, AuthError> {
        let bytes = Zeroizing::new(decode::<32>(token).map_err(|_| AuthError::Denied)?);
        let hash = Sha256::digest(bytes.as_slice());
        let row = sqlx::query("SELECT s.user_id,s.device_id FROM auth_sessions s JOIN accounts a ON a.user_id=s.user_id JOIN devices d ON d.device_id=s.device_id AND d.user_id=s.user_id WHERE s.token_hash=$1 AND s.expires_at_ms>$2 AND a.disabled_at IS NULL AND d.revoked_at IS NULL")
            .bind(hash.as_slice()).bind(self.now()?).fetch_optional(&self.pool).await?.ok_or(AuthError::Denied)?;
        Ok(AuthenticatedAccount {
            user_id: row.get("user_id"),
            device_id: row.get("device_id"),
        })
    }

    /// Create the account-level control-plane record for a group. MLS state is
    /// created separately by the elected member and never lives on the server.
    pub async fn create_group(
        &self,
        token: &str,
        request: CreateGroupRequest,
    ) -> Result<GroupResponse, AuthError> {
        if request.group_id.is_nil() {
            return Err(AuthError::Invalid);
        }
        let kind = match request.kind.as_deref().unwrap_or("group") {
            "direct" => GroupKind::Direct,
            "group" => GroupKind::Group,
            "channel" => GroupKind::Channel,
            _ => return Err(AuthError::Invalid),
        };
        let account = self.authenticate(token).await?;
        RelationalStore::from_pool(self.pool.clone())
            .create_group(request.group_id, account.user_id, kind)
            .await?;
        Ok(GroupResponse {
            group_id: request.group_id,
            owner_id: account.user_id,
            kind: kind.as_str().to_owned(),
        })
    }

    /// Return feature exposure for the authenticated organization device.
    pub async fn organization_controls(
        &self,
        token: &str,
    ) -> Result<OrganizationControlsResponse, AuthError> {
        let account = self.authenticate(token).await?;
        Ok(RelationalStore::from_pool(self.pool.clone())
            .organization_controls(account.user_id, account.device_id)
            .await?
            .into())
    }

    /// Owners and organization admins control whether Mini-Apps and bots are
    /// exposed to the organization. The store checks the account kind and role.
    pub async fn set_organization_controls(
        &self,
        token: &str,
        request: OrganizationControlsRequest,
    ) -> Result<OrganizationControlsResponse, AuthError> {
        let account = self.authenticate(token).await?;
        Ok(RelationalStore::from_pool(self.pool.clone())
            .set_organization_controls(
                account.user_id,
                account.device_id,
                request.mini_apps_enabled,
                request.bots_enabled,
            )
            .await?
            .into())
    }

    /// Return an authenticated membership snapshot. Role changes are applied
    /// by separate endpoints so the client can stage the matching MLS commit.
    pub async fn group_members(
        &self,
        token: &str,
        group_id: Uuid,
    ) -> Result<GroupMembersResponse, AuthError> {
        if group_id.is_nil() {
            return Err(AuthError::Invalid);
        }
        let account = self.authenticate(token).await?;
        let members = RelationalStore::from_pool(self.pool.clone())
            .group_members(group_id, account.user_id)
            .await?
            .into_iter()
            .map(|member| GroupMemberResponse {
                user_id: member.user_id,
                role: member.role.as_str().to_owned(),
            })
            .collect();
        Ok(GroupMembersResponse { group_id, members })
    }

    /// Apply RBAC using the bearer-session account as actor. The target user
    /// comes from the path, never from an untrusted actor field in JSON.
    pub async fn set_group_role(
        &self,
        token: &str,
        group_id: Uuid,
        target_user_id: Uuid,
        request: SetGroupRoleRequest,
    ) -> Result<(), AuthError> {
        if group_id.is_nil() || target_user_id.is_nil() {
            return Err(AuthError::Invalid);
        }
        let role = Role::parse(&request.role).map_err(|_| AuthError::Invalid)?;
        let account = self.authenticate(token).await?;
        RelationalStore::from_pool(self.pool.clone())
            .set_role(group_id, account.user_id, target_user_id, role)
            .await?;
        Ok(())
    }

    /// Remove a member or let the authenticated member leave. The caller must
    /// deliver the corresponding MLS remove commit before the next epoch.
    pub async fn remove_group_member(
        &self,
        token: &str,
        group_id: Uuid,
        target_user_id: Uuid,
    ) -> Result<(), AuthError> {
        if group_id.is_nil() || target_user_id.is_nil() {
            return Err(AuthError::Invalid);
        }
        let account = self.authenticate(token).await?;
        RelationalStore::from_pool(self.pool.clone())
            .remove_member(group_id, account.user_id, target_user_id)
            .await?;
        Ok(())
    }

    /// Revoke a physical client from the authenticated account. Every local
    /// MLS group containing that device must then stage a remove commit with
    /// `OpenMlsEngine::remove_devices`.
    pub async fn revoke_device(
        &self,
        token: &str,
        device_id: Uuid,
    ) -> Result<DeviceRevocationResponse, AuthError> {
        if device_id.is_nil() {
            return Err(AuthError::Invalid);
        }
        let account = self.authenticate(token).await?;
        RelationalStore::from_pool(self.pool.clone())
            .revoke_device(account.user_id, device_id)
            .await?;
        Ok(DeviceRevocationResponse {
            device_id,
            revoked: true,
        })
    }

    /// Register an additional physical client under the authenticated account.
    /// The current device authorizes the operation through its bearer session;
    /// the new device proves possession of its own Ed25519 identity key.
    pub async fn register_device(
        &self,
        token: &str,
        request: DeviceRegistrationRequest,
    ) -> Result<DeviceRegistrationResponse, AuthError> {
        if request.device_id.is_nil() || request.mls_node_id.is_nil() {
            return Err(AuthError::Invalid);
        }
        let account = self.authenticate(token).await?;
        let public_key = decode::<32>(&request.public_key)?;
        let nonce = decode::<32>(&request.nonce)?;
        let signature = decode::<64>(&request.signature)?;
        let binding = DeviceBinding {
            user_id: account.user_id,
            device_id: request.device_id,
            mls_node_id: request.mls_node_id,
            public_key,
        };
        verify(
            &public_key,
            &links_identity::device_pairing_transcript(
                account.user_id,
                request.device_id,
                request.mls_node_id,
                &public_key,
                &nonce,
            )?,
            &signature,
        )?;
        let credential = binding.mls_credential()?;
        let mut tx = self.pool.begin().await?;
        sqlx::query(
            "SELECT user_id FROM accounts WHERE user_id=$1 AND disabled_at IS NULL FOR UPDATE",
        )
        .bind(account.user_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(AuthError::Denied)?;
        let existing = sqlx::query(
            "SELECT user_id,mls_node_id,identity_public_key,mls_credential,revoked_at IS NULL AS active FROM devices WHERE device_id=$1 FOR UPDATE",
        )
        .bind(request.device_id)
        .fetch_optional(&mut *tx)
        .await?;
        if let Some(existing) = existing {
            let same = existing.get::<Uuid, _>("user_id") == account.user_id
                && existing.get::<Uuid, _>("mls_node_id") == request.mls_node_id
                && existing.get::<Vec<u8>, _>("identity_public_key") == public_key
                && existing.get::<Vec<u8>, _>("mls_credential") == credential
                && existing.get::<bool, _>("active");
            if same {
                tx.commit().await?;
                return Ok(DeviceRegistrationResponse {
                    user_id: account.user_id,
                    device_id: request.device_id,
                    mls_node_id: request.mls_node_id,
                    public_key: encode(&public_key),
                    mls_credential: encode(&credential),
                    delegation_certificate: None,
                });
            }
            return Err(AuthError::Conflict);
        }
        let node_taken: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM devices WHERE mls_node_id=$1)")
                .bind(request.mls_node_id)
                .fetch_one(&mut *tx)
                .await?;
        if node_taken {
            return Err(AuthError::Conflict);
        }
        sqlx::query("INSERT INTO devices (device_id,user_id,mls_node_id,identity_public_key,mls_credential) VALUES ($1,$2,$3,$4,$5)")
            .bind(request.device_id)
            .bind(account.user_id)
            .bind(request.mls_node_id)
            .bind(public_key.as_slice())
            .bind(&credential)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(DeviceRegistrationResponse {
            user_id: account.user_id,
            device_id: request.device_id,
            mls_node_id: request.mls_node_id,
            public_key: encode(&public_key),
            mls_credential: encode(&credential),
            delegation_certificate: None,
        })
    }

    /// Register a device authorized by an active owner/admin key. The issuer
    /// certificate grants either a device leaf or another admin leaf; an
    /// admin may not create another admin. The new device also proves local
    /// possession of its subject key before it is added to the account.
    pub async fn register_delegated_device(
        &self,
        token: &str,
        request: DelegatedDeviceRegistrationRequest,
    ) -> Result<DeviceRegistrationResponse, AuthError> {
        let account = self.authenticate(token).await?;
        let certificate_bytes = decode_blob(&request.certificate, links_protocol::MAX_MESSAGE_BYTES)?;
        let certificate = v1::DeviceSubCertificate::decode(certificate_bytes.as_slice())
            .map_err(|_| AuthError::Invalid)?;
        links_protocol::validate_device_subcertificate(&certificate)
            .map_err(|_| AuthError::Invalid)?;
        if certificate.user_id != account.user_id.to_string()
            || certificate.issuer_device_id != account.device_id.to_string()
        {
            return Err(AuthError::Denied);
        }
        let now = self.now()? as u64;
        links_identity::verify_device_subcertificate(&certificate, now)?;
        let subject_device_id = Uuid::parse_str(&certificate.subject_device_id)
            .map_err(|_| AuthError::Invalid)?;
        let subject_mls_node_id = Uuid::parse_str(&certificate.subject_mls_node_id)
            .map_err(|_| AuthError::Invalid)?;
        let subject_public_key: [u8; 32] = certificate
            .subject_public_key
            .as_slice()
            .try_into()
            .map_err(|_| AuthError::Invalid)?;
        let nonce = decode::<32>(&request.nonce)?;
        let subject_signature = decode::<64>(&request.signature)?;
        verify(
            &subject_public_key,
            &links_identity::device_pairing_transcript(
                account.user_id,
                subject_device_id,
                subject_mls_node_id,
                &subject_public_key,
                &nonce,
            )?,
            &subject_signature,
        )?;

        let issuer = sqlx::query(
            "SELECT delegation_role,delegation_certificate FROM devices WHERE user_id=$1 AND device_id=$2 AND revoked_at IS NULL",
        )
        .bind(account.user_id)
        .bind(account.device_id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or(AuthError::Denied)?;
        let issuer_role: String = issuer.get("delegation_role");
        match (issuer_role.as_str(), certificate.delegation_role) {
            ("owner", 1 | 2) => {}
            ("admin", 1) => {}
            _ => return Err(AuthError::Denied),
        }
        if issuer_role == "admin" {
            let issuer_bytes: Vec<u8> = issuer
                .get::<Option<Vec<u8>>, _>("delegation_certificate")
                .ok_or(AuthError::Unavailable)?;
            let issuer_certificate = v1::DeviceSubCertificate::decode(issuer_bytes.as_slice())
                .map_err(|_| AuthError::Unavailable)?;
            links_identity::verify_device_subcertificate(&issuer_certificate, now)?;
            if issuer_certificate.user_id != account.user_id.to_string()
                || issuer_certificate.subject_device_id != account.device_id.to_string()
                || issuer_certificate.delegation_role != 2
            {
                return Err(AuthError::Unavailable);
            }
        }
        let binding = DeviceBinding {
            user_id: account.user_id,
            device_id: subject_device_id,
            mls_node_id: subject_mls_node_id,
            public_key: subject_public_key,
        };
        let credential = binding.mls_credential()?;
        RelationalStore::from_pool(self.pool.clone())
            .register_delegated_device(account.user_id, &certificate, &credential)
            .await?;
        Ok(DeviceRegistrationResponse {
            user_id: account.user_id,
            device_id: subject_device_id,
            mls_node_id: subject_mls_node_id,
            public_key: encode(&subject_public_key),
            mls_credential: encode(&credential),
            delegation_certificate: Some(encode(&certificate_bytes)),
        })
    }

    pub async fn passkey_registration_start(
        &self,
        token: &str,
    ) -> Result<PasskeyOptions, AuthError> {
        let account = self.authenticate(token).await?;
        let config = self.passkey_config()?.clone();
        let now = self.now()?;
        let challenge = random()?;
        let challenge_id = Uuid::new_v4();
        let expires = now + PASSKEY_CHALLENGE_TTL_MS as i64;
        let mut tx = self.pool.begin().await?;
        sqlx::query(
            "UPDATE passkey_challenges SET state='failed' WHERE user_id=$1 AND purpose='registration' AND state='pending'",
        )
        .bind(account.user_id)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT INTO passkey_challenges (challenge_id,user_id,purpose,challenge,state,expires_at_ms) VALUES ($1,$2,'registration',$3,'pending',$4)",
        )
        .bind(challenge_id)
        .bind(account.user_id)
        .bind(challenge.as_slice())
        .bind(expires)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(PasskeyOptions {
            challenge_id,
            challenge: encode(&challenge),
            rp_id: config.rp_id,
            user_id: account.user_id,
            expires_at_ms: expires as u64,
        })
    }

    pub async fn passkey_registration_finish(
        &self,
        token: &str,
        request: PasskeyRegistrationFinishRequest,
    ) -> Result<RegisteredPasskeyResponse, AuthError> {
        let account = self.authenticate(token).await?;
        let config = self.passkey_config()?.clone();
        let expected_credential_id = decode_blob(&request.credential_id, 1024)?;
        let client_data_json = decode_blob(&request.client_data_json, 4096)?;
        let attestation_object = decode_blob(&request.attestation_object, 8192)?;
        let mut tx = self.pool.begin().await?;
        let row = sqlx::query(
            "SELECT challenge,state,expires_at_ms FROM passkey_challenges WHERE challenge_id=$1 AND user_id=$2 AND purpose='registration' FOR UPDATE",
        )
        .bind(request.challenge_id)
        .bind(account.user_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(AuthError::Denied)?;
        let challenge: [u8; 32] = row
            .get::<Vec<u8>, _>("challenge")
            .try_into()
            .map_err(|_| AuthError::Unavailable)?;
        if row.get::<String, _>("state") != "pending"
            || row.get::<i64, _>("expires_at_ms") <= self.now()?
        {
            return Err(AuthError::Denied);
        }
        let credential = verify_registration(
            &config,
            &challenge,
            &expected_credential_id,
            &client_data_json,
            &attestation_object,
        )?;
        sqlx::query(
            "INSERT INTO passkey_credentials (user_id,credential_id,public_key,sign_count) VALUES ($1,$2,$3,$4)",
        )
        .bind(account.user_id)
        .bind(&credential.credential_id)
        .bind(credential.public_key.as_slice())
        .bind(i64::from(credential.sign_count))
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "UPDATE passkey_challenges SET state='consumed' WHERE challenge_id=$1 AND state='pending'",
        )
        .bind(request.challenge_id)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(RegisteredPasskeyResponse {
            credential_id: encode(&credential.credential_id),
            sign_count: credential.sign_count,
        })
    }

    pub async fn passkey_assertion_start(&self, token: &str) -> Result<PasskeyOptions, AuthError> {
        let account = self.authenticate(token).await?;
        let config = self.passkey_config()?.clone();
        let now = self.now()?;
        let challenge = random()?;
        let challenge_id = Uuid::new_v4();
        let expires = now + PASSKEY_CHALLENGE_TTL_MS as i64;
        let mut tx = self.pool.begin().await?;
        sqlx::query(
            "UPDATE passkey_challenges SET state='failed' WHERE user_id=$1 AND purpose='assertion' AND state='pending'",
        )
        .bind(account.user_id)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT INTO passkey_challenges (challenge_id,user_id,purpose,challenge,state,expires_at_ms) VALUES ($1,$2,'assertion',$3,'pending',$4)",
        )
        .bind(challenge_id)
        .bind(account.user_id)
        .bind(challenge.as_slice())
        .bind(expires)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(PasskeyOptions {
            challenge_id,
            challenge: encode(&challenge),
            rp_id: config.rp_id,
            user_id: account.user_id,
            expires_at_ms: expires as u64,
        })
    }

    pub async fn passkey_assertion_finish(
        &self,
        token: &str,
        request: PasskeyAssertionFinishRequest,
    ) -> Result<PasskeyAssertionResponse, AuthError> {
        let account = self.authenticate(token).await?;
        let config = self.passkey_config()?.clone();
        let credential_id = decode_blob(&request.credential_id, 1024)?;
        let client_data_json = decode_blob(&request.client_data_json, 4096)?;
        let authenticator_data = decode_blob(&request.authenticator_data, 4096)?;
        let signature = decode_blob(&request.signature, 1024)?;
        let mut tx = self.pool.begin().await?;
        let challenge_row = sqlx::query(
            "SELECT challenge,state,expires_at_ms FROM passkey_challenges WHERE challenge_id=$1 AND user_id=$2 AND purpose='assertion' FOR UPDATE",
        )
        .bind(request.challenge_id)
        .bind(account.user_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(AuthError::Denied)?;
        let challenge: [u8; 32] = challenge_row
            .get::<Vec<u8>, _>("challenge")
            .try_into()
            .map_err(|_| AuthError::Unavailable)?;
        if challenge_row.get::<String, _>("state") != "pending"
            || challenge_row.get::<i64, _>("expires_at_ms") <= self.now()?
        {
            return Err(AuthError::Denied);
        }
        let credential_row = sqlx::query(
            "SELECT public_key,sign_count FROM passkey_credentials WHERE user_id=$1 AND credential_id=$2 FOR UPDATE",
        )
        .bind(account.user_id)
        .bind(&credential_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(AuthError::Denied)?;
        let public_key: [u8; 64] = credential_row
            .get::<Vec<u8>, _>("public_key")
            .try_into()
            .map_err(|_| AuthError::Unavailable)?;
        let previous_sign_count = credential_row.get::<i64, _>("sign_count");
        if !(0..=u32::MAX as i64).contains(&previous_sign_count) {
            return Err(AuthError::Unavailable);
        }
        let sign_count = verify_assertion(
            &config,
            &challenge,
            &client_data_json,
            &authenticator_data,
            &signature,
            &public_key,
            previous_sign_count as u32,
        )?;
        sqlx::query(
            "UPDATE passkey_credentials SET sign_count=$3,last_used_at=now() WHERE user_id=$1 AND credential_id=$2",
        )
        .bind(account.user_id)
        .bind(&credential_id)
        .bind(i64::from(sign_count))
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "UPDATE passkey_challenges SET state='consumed' WHERE challenge_id=$1 AND state='pending'",
        )
        .bind(request.challenge_id)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(PasskeyAssertionResponse {
            credential_id: encode(&credential_id),
            sign_count,
        })
    }

    pub async fn put_encrypted_key_backup(
        &self,
        token: &str,
        request: EncryptedKeyBackupRequest,
    ) -> Result<(), AuthError> {
        let account = self.authenticate(token).await?;
        if request.device_id != account.device_id {
            return Err(AuthError::Denied);
        }
        let credential_id = decode_blob(&request.credential_id, 1024)?;
        let encrypted_envelope = decode_blob(&request.encrypted_envelope, 8192)?;
        validate_encrypted_backup_envelope(
            request.backup_id,
            request.device_id,
            &credential_id,
            &encrypted_envelope,
        )?;
        RelationalStore::from_pool(self.pool.clone())
            .put_encrypted_key_backup(
                account.user_id,
                account.device_id,
                request.backup_id,
                &credential_id,
                &encrypted_envelope,
            )
            .await?;
        Ok(())
    }

    pub async fn get_encrypted_key_backup(
        &self,
        token: &str,
        backup_id: Uuid,
    ) -> Result<EncryptedKeyBackupResponse, AuthError> {
        let account = self.authenticate(token).await?;
        let record = RelationalStore::from_pool(self.pool.clone())
            .encrypted_key_backup(account.user_id, backup_id)
            .await?;
        Ok(EncryptedKeyBackupResponse {
            backup_id: record.backup_id,
            device_id: record.device_id,
            credential_id: encode(&record.credential_id),
            encrypted_envelope: encode(&record.encrypted_envelope),
        })
    }
    pub async fn prekey_inventory(&self, token: &str) -> Result<v1::PreKeyInventory, AuthError> {
        let account = self.authenticate(token).await?;
        Ok(RelationalStore::from_pool(self.pool.clone())
            .prekey_inventory(account.user_id, account.device_id)
            .await?)
    }
    pub async fn upload_prekeys(
        &self,
        token: &str,
        upload: v1::PreKeyUpload,
    ) -> Result<v1::PreKeyInventory, AuthError> {
        let account = self.authenticate(token).await?;
        if upload.device_id != account.device_id.to_string() {
            return Err(AuthError::Denied);
        }
        verify_prekey_upload(&upload)?;
        Ok(RelationalStore::from_pool(self.pool.clone())
            .upload_prekeys(account.user_id, account.device_id, &upload)
            .await?)
    }
    pub async fn claim_prekey_bundle(
        &self,
        token: &str,
        target_device_id: Uuid,
    ) -> Result<v1::PreKeyBundle, AuthError> {
        self.authenticate(token).await?;
        Ok(RelationalStore::from_pool(self.pool.clone())
            .claim_prekey_bundle(target_device_id)
            .await?)
    }

    pub async fn put_mls_key_package(
        &self,
        token: &str,
        key_package: Vec<u8>,
    ) -> Result<(), AuthError> {
        let account = self.authenticate(token).await?;
        if key_package.is_empty() || key_package.len() > links_protocol::MAX_FRAME_BYTES {
            return Err(AuthError::Invalid);
        }
        RelationalStore::from_pool(self.pool.clone())
            .put_mls_key_package(account.device_id, &key_package)
            .await?;
        Ok(())
    }

    pub async fn get_mls_key_package(
        &self,
        token: &str,
        target_device_id: Uuid,
    ) -> Result<Vec<u8>, AuthError> {
        self.authenticate(token).await?;
        RelationalStore::from_pool(self.pool.clone())
            .mls_key_package(target_device_id)
            .await?
            .ok_or(AuthError::Denied)
    }

    pub async fn contact_psi_parameters(
        &self,
        token: &str,
        peer_ip: IpAddr,
    ) -> Result<ContactPsiParametersResponse, AuthError> {
        let account = self.authenticate(token).await?;
        self.enforce_contact_psi_rate_limits(account.user_id, peer_ip, self.now()?, 1)
            .await?;
        let rows = sqlx::query(
            "SELECT a.contact_directory_token FROM accounts a WHERE a.disabled_at IS NULL AND a.contact_directory_token IS NOT NULL AND EXISTS (SELECT 1 FROM devices d WHERE d.user_id=a.user_id AND d.revoked_at IS NULL) ORDER BY a.contact_directory_token",
        )
        .fetch_all(&self.pool)
        .await?;
        if rows.len() > links_protocol::contact_psi::MAX_DIRECTORY_TOKENS {
            return Err(AuthError::Unavailable);
        }
        let mut directory_tokens = Vec::with_capacity(rows.len());
        for row in rows {
            let token: Vec<u8> = row.get("contact_directory_token");
            if token.len() != links_protocol::contact_psi::TOKEN_BYTES {
                return Err(AuthError::Unavailable);
            }
            directory_tokens.push(token.try_into().map_err(|_| AuthError::Unavailable)?);
        }
        let filter = links_protocol::contact_psi::ContactPsiFilter::from_tokens(&directory_tokens)
            .map_err(|_| AuthError::Unavailable)?;
        Ok(ContactPsiParametersResponse {
            protocol_version: links_protocol::contact_psi::VERSION,
            server_public_key: encode(&links_protocol::contact_psi::server_public_key(
                &*self.phone_lookup_key,
            )),
            filter: encode(filter.bits()),
            filter_hash_count: filter.hash_count(),
            filter_item_count: filter.item_count(),
        })
    }

    pub async fn contact_psi_query(
        &self,
        token: &str,
        peer_ip: IpAddr,
        request: ContactPsiQueryRequest,
    ) -> Result<ContactPsiQueryResponse, AuthError> {
        if request.protocol_version != links_protocol::contact_psi::VERSION
            || request.blinded_inputs.is_empty()
            || request.blinded_inputs.len() > links_protocol::contact_psi::MAX_QUERY_ITEMS
        {
            return Err(AuthError::Invalid);
        }
        let account = self.authenticate(token).await?;
        self.enforce_contact_psi_rate_limits(
            account.user_id,
            peer_ip,
            self.now()?,
            request.blinded_inputs.len(),
        )
        .await?;
        let mut evaluations = Vec::with_capacity(request.blinded_inputs.len());
        for encoded in request.blinded_inputs {
            let blinded = decode::<{ links_protocol::contact_psi::POINT_BYTES }>(&encoded)?;
            let evaluation = links_protocol::contact_psi::evaluate_blinded(
                &*self.phone_lookup_key,
                &blinded,
                &random_wide()?,
            )
            .map_err(|_| AuthError::Invalid)?;
            evaluations.push(ContactPsiEvaluationResponse {
                evaluated_point: encode(&evaluation.evaluated_point),
                proof: encode(&evaluation.proof),
            });
        }
        Ok(ContactPsiQueryResponse {
            protocol_version: links_protocol::contact_psi::VERSION,
            evaluations,
        })
    }

    /// Return the public Privacy Pass issuer key. This response is safe to
    /// cache only in memory; HTTP callers still receive no-store headers.
    pub fn privacy_pass_parameters(&self) -> Result<PrivacyPassParametersResponse, AuthError> {
        let parameters =
            links_protocol::privacy_pass::IssuerParameters::from_seed(&*self.privacy_pass_key)
                .map_err(|_| AuthError::Unavailable)?;
        Ok(PrivacyPassParametersResponse {
            protocol_version: links_protocol::privacy_pass::VERSION,
            token_type: links_protocol::privacy_pass::TOKEN_TYPE,
            public_key: encode(&parameters.public_key),
            token_key_id: encode(&parameters.token_key_id),
        })
    }

    /// Issue a blind signature after authenticating the client. The issuer
    /// sees the account and quota use, but never the token challenge or nonce.
    pub async fn privacy_pass_issue(
        &self,
        token: &str,
        peer_ip: IpAddr,
        request: PrivacyPassIssueRequest,
    ) -> Result<PrivacyPassIssueResponse, AuthError> {
        if request.protocol_version != links_protocol::privacy_pass::VERSION
            || request.token_type != links_protocol::privacy_pass::TOKEN_TYPE
        {
            return Err(AuthError::Invalid);
        }
        let account = self.authenticate(token).await?;
        self.enforce_privacy_pass_issue_rate_limits(account.user_id, peer_ip)
            .await?;
        let parameters =
            links_protocol::privacy_pass::IssuerParameters::from_seed(&*self.privacy_pass_key)
                .map_err(|_| AuthError::Unavailable)?;
        if request.truncated_token_key_id
            != parameters.token_key_id[links_protocol::privacy_pass::TOKEN_KEY_ID_BYTES - 1]
        {
            return Err(AuthError::Invalid);
        }
        let blinded_message =
            decode::<{ links_protocol::privacy_pass::POINT_BYTES }>(&request.blinded_message)?;
        let response = links_protocol::privacy_pass::evaluate(
            &*self.privacy_pass_key,
            &links_protocol::privacy_pass::TokenRequest {
                token_type: request.token_type,
                truncated_token_key_id: request.truncated_token_key_id,
                blinded_message,
            },
            &random_scalar_bytes()?,
        )
        .map_err(|_| AuthError::Invalid)?;
        Ok(PrivacyPassIssueResponse {
            protocol_version: links_protocol::privacy_pass::VERSION,
            token_type: links_protocol::privacy_pass::TOKEN_TYPE,
            evaluated_message: encode(&response.evaluated_message),
            proof: encode(&response.proof),
        })
    }

    /// Create an origin challenge without creating an identity-bound record.
    pub async fn privacy_pass_challenge(
        &self,
        peer_ip: IpAddr,
    ) -> Result<PrivacyPassChallengeResponse, AuthError> {
        let now = self.now()?;
        self.enforce_privacy_pass_ip_rate_limit(
            b"links/privacy-pass-challenge-ip-hour/v1\0",
            peer_ip,
            120,
        )
        .await?;
        let expires_at_ms = now
            .checked_add(PRIVACY_PASS_CHALLENGE_TTL_MS as i64)
            .ok_or(AuthError::Unavailable)? as u64;
        let mut challenge = [0u8; links_protocol::privacy_pass::CHALLENGE_BYTES];
        let challenge_nonce = Zeroizing::new(random()?);
        challenge[..32].copy_from_slice(&*challenge_nonce);
        challenge[32..].copy_from_slice(&expires_at_ms.to_be_bytes());
        Ok(PrivacyPassChallengeResponse {
            protocol_version: links_protocol::privacy_pass::VERSION,
            token_type: links_protocol::privacy_pass::TOKEN_TYPE,
            challenge: encode(&challenge),
            expires_at_ms,
        })
    }

    /// Redeem a token without bearer authentication. Only an opaque token
    /// digest and expiry are stored, so redemption is not linked to a user.
    pub async fn privacy_pass_redeem(
        &self,
        peer_ip: IpAddr,
        request: PrivacyPassRedeemRequest,
    ) -> Result<PrivacyPassRedeemResponse, AuthError> {
        if request.protocol_version != links_protocol::privacy_pass::VERSION {
            return Err(AuthError::Invalid);
        }
        self.enforce_privacy_pass_ip_rate_limit(
            b"links/privacy-pass-redeem-ip-hour/v1\0",
            peer_ip,
            200,
        )
        .await?;
        let token_bytes = decode::<{ links_protocol::privacy_pass::TOKEN_BYTES }>(&request.token)?;
        let challenge =
            decode::<{ links_protocol::privacy_pass::CHALLENGE_BYTES }>(&request.challenge)?;
        let token = links_protocol::privacy_pass::PrivacyPassToken::from_bytes(&token_bytes)
            .map_err(|_| AuthError::Denied)?;
        let now = self.now()? as u64;
        links_protocol::privacy_pass::verify_token(
            &token,
            &challenge,
            now,
            &*self.privacy_pass_key,
        )
        .map_err(|_| AuthError::Denied)?;
        let expires_at_ms = links_protocol::privacy_pass::challenge_expiry_ms(&challenge);
        let expires_at_ms = i64::try_from(expires_at_ms).map_err(|_| AuthError::Denied)?;
        let token_hash = Sha256::digest(token_bytes);
        let inserted = sqlx::query(
            "INSERT INTO privacy_pass_redeemed (token_hash,expires_at_ms) VALUES ($1,$2) ON CONFLICT (token_hash) DO NOTHING",
        )
        .bind(token_hash.as_slice())
        .bind(expires_at_ms)
        .execute(&self.pool)
        .await?;
        if inserted.rows_affected() != 1 {
            return Err(AuthError::Denied);
        }
        Ok(PrivacyPassRedeemResponse {
            protocol_version: links_protocol::privacy_pass::VERSION,
            accepted: true,
        })
    }

    /// Create a short-lived hashcash challenge for a pseudonymous account.
    /// Phone-verified accounts do not need this admission proof.
    pub async fn chat_pow_challenge(
        &self,
        token: &str,
        peer_ip: IpAddr,
    ) -> Result<ChatProofOfWorkChallengeResponse, AuthError> {
        let account = self.authenticate(token).await?;
        self.require_unverified_account(&account).await?;
        self.enforce_chat_pow_rate_limits(
            b"links/chat-request-pow-challenge-user-hour/v1\0",
            account.user_id,
            peer_ip,
            20,
            200,
        )
        .await?;
        let now = self.now()?;
        let expires_at_ms = now
            .checked_add(PROOF_OF_WORK_CHALLENGE_TTL_MS as i64)
            .ok_or(AuthError::Unavailable)? as u64;
        let challenge = Zeroizing::new(random()?);
        let challenge_hash = self.digest(b"links/chat-request-pow-challenge/v1\0", &*challenge);
        sqlx::query(
            "INSERT INTO proof_of_work_challenges (challenge_hash,user_id,device_id,difficulty_bits,expires_at_ms,state) VALUES ($1,$2,$3,$4,$5,'issued')",
        )
        .bind(challenge_hash.as_slice())
        .bind(account.user_id)
        .bind(account.device_id)
        .bind(i16::from(links_protocol::proof_of_work::DEFAULT_DIFFICULTY_BITS))
        .bind(expires_at_ms as i64)
        .execute(&self.pool)
        .await?;
        Ok(ChatProofOfWorkChallengeResponse {
            protocol_version: links_protocol::proof_of_work::VERSION,
            challenge: encode(&*challenge),
            difficulty_bits: links_protocol::proof_of_work::DEFAULT_DIFFICULTY_BITS,
            expires_at_ms,
        })
    }

    /// Verify and consume one client proof-of-work challenge. The challenge is
    /// bound to the authenticated account and device, but no IP is persisted.
    pub async fn chat_pow_verify(
        &self,
        token: &str,
        peer_ip: IpAddr,
        request: ChatProofOfWorkVerifyRequest,
    ) -> Result<ChatProofOfWorkVerifyResponse, AuthError> {
        if request.protocol_version != links_protocol::proof_of_work::VERSION {
            return Err(AuthError::Invalid);
        }
        let account = self.authenticate(token).await?;
        self.require_unverified_account(&account).await?;
        self.enforce_chat_pow_rate_limits(
            b"links/chat-request-pow-verify-user-hour/v1\0",
            account.user_id,
            peer_ip,
            60,
            300,
        )
        .await?;
        let challenge =
            decode::<{ links_protocol::proof_of_work::CHALLENGE_BYTES }>(&request.challenge)?;
        let challenge_hash = self.digest(b"links/chat-request-pow-challenge/v1\0", &challenge);
        let now = self.now()?;
        let mut tx = self.pool.begin().await?;
        let row = sqlx::query(
            "SELECT user_id,device_id,difficulty_bits,expires_at_ms,state FROM proof_of_work_challenges WHERE challenge_hash=$1 FOR UPDATE",
        )
        .bind(challenge_hash.as_slice())
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(AuthError::Denied)?;
        let expires_at_ms: i64 = row.get("expires_at_ms");
        let state: String = row.get("state");
        let difficulty_bits: u8 = row
            .get::<i16, _>("difficulty_bits")
            .try_into()
            .map_err(|_| AuthError::Unavailable)?;
        if state != "issued"
            || expires_at_ms <= now
            || row.get::<Uuid, _>("user_id") != account.user_id
            || row.get::<Uuid, _>("device_id") != account.device_id
        {
            return Err(AuthError::Denied);
        }
        links_protocol::proof_of_work::verify(&challenge, difficulty_bits, request.nonce)
            .map_err(|_| AuthError::Denied)?;
        let updated = sqlx::query(
            "UPDATE proof_of_work_challenges SET state='consumed' WHERE challenge_hash=$1 AND state='issued'",
        )
        .bind(challenge_hash.as_slice())
        .execute(&mut *tx)
        .await?;
        if updated.rows_affected() != 1 {
            return Err(AuthError::Denied);
        }
        tx.commit().await?;
        Ok(ChatProofOfWorkVerifyResponse {
            protocol_version: links_protocol::proof_of_work::VERSION,
            accepted: true,
        })
    }

    pub async fn purge_expired(&self) -> Result<(), AuthError> {
        let now = self.now()?;
        sqlx::query("DELETE FROM auth_sessions WHERE expires_at_ms <= $1")
            .bind(now)
            .execute(&self.pool)
            .await?;
        // Keep consumed provider SIDs for another 10 minutes after local expiry.
        sqlx::query("DELETE FROM auth_challenges WHERE expires_at_ms <= $1")
            .bind(now - CHALLENGE_TTL_MS as i64)
            .execute(&self.pool)
            .await?;
        sqlx::query("DELETE FROM passkey_challenges WHERE expires_at_ms <= $1")
            .bind(now - PASSKEY_CHALLENGE_TTL_MS as i64)
            .execute(&self.pool)
            .await?;
        sqlx::query("DELETE FROM auth_rate_limits WHERE window_start_ms <= $1")
            .bind(now - 3_600_000)
            .execute(&self.pool)
            .await?;
        sqlx::query("DELETE FROM privacy_pass_redeemed WHERE expires_at_ms <= $1")
            .bind(now)
            .execute(&self.pool)
            .await?;
        sqlx::query("DELETE FROM proof_of_work_challenges WHERE expires_at_ms <= $1")
            .bind(now)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}

fn verify_prekey_upload(upload: &v1::PreKeyUpload) -> Result<(), AuthError> {
    links_protocol::validate_prekey_upload(upload).map_err(|_| AuthError::Invalid)?;
    let profile = upload.profile.as_ref().ok_or(AuthError::Invalid)?;
    let identity = profile.identity.as_ref().ok_or(AuthError::Invalid)?;
    let signing_key: [u8; 32] = identity
        .signing_key
        .as_slice()
        .try_into()
        .map_err(|_| AuthError::Invalid)?;
    let dh_key: [u8; 32] = identity
        .dh_key
        .as_slice()
        .try_into()
        .map_err(|_| AuthError::Invalid)?;
    verify(
        &signing_key,
        &links_identity::pqxdh_identity_binding_transcript(&dh_key),
        &identity.binding_signature,
    )?;
    let signed = profile.signed_prekey.as_ref().ok_or(AuthError::Invalid)?;
    let signed_key = signed.prekey.as_ref().ok_or(AuthError::Invalid)?;
    let signed_public: [u8; 32] = signed_key
        .public_key
        .as_slice()
        .try_into()
        .map_err(|_| AuthError::Invalid)?;
    verify(
        &signing_key,
        &links_identity::pqxdh_signed_prekey_transcript(&dh_key, signed_key.id, &signed_public),
        &signed.signature,
    )?;
    let last = profile
        .last_resort_kem_prekey
        .as_ref()
        .ok_or(AuthError::Invalid)?;
    verify_kem_prekey(&signing_key, &dh_key, last)?;
    for key in &upload.one_time_kem_prekeys {
        verify_kem_prekey(&signing_key, &dh_key, key)?;
    }
    Ok(())
}

fn validate_encrypted_backup_envelope(
    backup_id: Uuid,
    device_id: Uuid,
    credential_id: &[u8],
    envelope: &[u8],
) -> Result<(), AuthError> {
    // Keep this structural check on the server so a plaintext seed or an
    // unrelated blob cannot be registered as a backup. AEAD verification stays
    // on the device because the passkey PRF output never reaches this service.
    if backup_id.is_nil()
        || device_id.is_nil()
        || !(1..=1024).contains(&credential_id.len())
        || envelope.len() < 128
        || envelope.len() > 8192
        || envelope[0] != 1
        || envelope[1] != 1
        || envelope[2..18] != backup_id.as_bytes()[..]
        || envelope[18..34] != device_id.as_bytes()[..]
    {
        return Err(AuthError::Invalid);
    }
    let credential_len = u16::from_be_bytes([envelope[34], envelope[35]]) as usize;
    if credential_len != credential_id.len()
        || envelope.len() != 128 + credential_len
        || envelope[36..36 + credential_len] != credential_id[..]
    {
        return Err(AuthError::Invalid);
    }
    Ok(())
}

fn verify_kem_prekey(
    signing_key: &[u8; 32],
    dh_key: &[u8; 32],
    key: &v1::KemPreKey,
) -> Result<(), AuthError> {
    verify(
        signing_key,
        &links_identity::pqxdh_kem_prekey_transcript(dh_key, key.id, key.one_time, &key.public_key),
        &key.signature,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn e164_only_and_secret_bounds() {
        assert!(validate_phone("+12025550123").is_ok());
        for phone in [
            "12025550123",
            "+02025550123",
            "+1 2025550123",
            "+123",
            "+1234567890123456",
            "+１２３４５６７８",
        ] {
            assert!(validate_phone(phone).is_err());
        }
        assert!(decode::<32>(&"A".repeat(10000)).is_err());
    }
}
