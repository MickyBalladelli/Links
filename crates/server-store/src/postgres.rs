use crate::{
    payload::{AppendRequest, AppendResult, EncryptedPayloadStore, ReadRequest, MAX_PURGE_BATCH},
    StoreError,
};
use async_trait::async_trait;
use links_protocol::{
    self as protocol, v1, validate_handle, validate_prekey_upload, MAX_CURSOR, MAX_FRAME_BYTES,
    MAX_ONE_TIME_PREKEYS, MAX_RETENTION_MS,
};
use prost::Message;
use sha2::{Digest, Sha256};
use sqlx::{postgres::PgPoolOptions, PgPool, Postgres, Row, Transaction};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Owner,
    Admin,
    Member,
}
impl Role {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Owner => "owner",
            Self::Admin => "admin",
            Self::Member => "member",
        }
    }
    pub fn parse(value: &str) -> Result<Self, StoreError> {
        match value {
            "owner" => Ok(Self::Owner),
            "admin" => Ok(Self::Admin),
            "member" => Ok(Self::Member),
            _ => Err(StoreError::Invalid),
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccountKind {
    Consumer,
    Pseudonymous,
    Organization,
}
impl AccountKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Consumer => "consumer",
            Self::Pseudonymous => "pseudonymous",
            Self::Organization => "organization",
        }
    }
}
#[derive(Debug, Clone, Copy)]
pub enum GroupKind {
    Direct,
    Group,
    Channel,
}

pub struct PasskeyCredentialRecord {
    pub credential_id: Vec<u8>,
    pub public_key: [u8; 64],
    pub sign_count: u32,
}

pub struct EncryptedKeyBackupRecord {
    pub backup_id: Uuid,
    pub device_id: Uuid,
    pub credential_id: Vec<u8>,
    pub encrypted_envelope: Vec<u8>,
}

/// Public device material returned by the global username directory.
/// This record intentionally contains no pre-key private material, session
/// data, phone-derived subject, or routing state.
pub struct DirectoryDeviceRecord {
    pub device_id: Uuid,
    pub mls_node_id: Uuid,
    pub did: String,
    pub identity_public_key: Vec<u8>,
    pub mls_credential: Vec<u8>,
    pub delegation_role: String,
    pub delegation_certificate: Option<Vec<u8>>,
}

pub struct HandleDirectoryRecord {
    pub user_id: Uuid,
    pub devices: Vec<DirectoryDeviceRecord>,
    pub verification_badge: Option<Vec<u8>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OrganizationControls {
    pub organization_id: Uuid,
    pub mini_apps_enabled: bool,
    pub bots_enabled: bool,
    pub revision: u64,
}
impl GroupKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Direct => "direct",
            Self::Group => "group",
            Self::Channel => "channel",
        }
    }
}

pub struct GroupMemberRecord {
    pub user_id: Uuid,
    pub role: Role,
}

#[derive(Clone)]
pub struct RelationalStore {
    pool: PgPool,
}
impl RelationalStore {
    pub async fn connect(url: &str) -> Result<Self, StoreError> {
        Ok(Self {
            pool: PgPoolOptions::new()
                .max_connections(10)
                .connect(url)
                .await?,
        })
    }
    pub fn from_pool(pool: PgPool) -> Self {
        Self { pool }
    }
    pub async fn migrate(&self) -> Result<(), StoreError> {
        sqlx::migrate!("./migrations")
            .run(&self.pool)
            .await
            .map_err(StoreError::Migration)
    }
    pub async fn close(&self) {
        self.pool.close().await;
    }

    /// Input is a keyed, server-secret lookup digest from the auth service.
    /// Never pass a plain phone number or an unsalted phone hash here.
    pub async fn create_account(
        &self,
        user_id: Uuid,
        auth_subject_hash: &[u8],
    ) -> Result<(), StoreError> {
        self.create_account_with_kind(user_id, auth_subject_hash, AccountKind::Consumer)
            .await
    }

    async fn create_account_with_kind(
        &self,
        user_id: Uuid,
        auth_subject_hash: &[u8],
        account_kind: AccountKind,
    ) -> Result<(), StoreError> {
        let mut tx = self.pool.begin().await?;
        sqlx::query(
            "INSERT INTO accounts (user_id, auth_subject_hash, account_kind) VALUES ($1, $2, $3)",
        )
            .bind(user_id)
            .bind(auth_subject_hash)
            .bind(account_kind.as_str())
            .execute(&mut *tx)
            .await?;
        if account_kind == AccountKind::Organization {
            sqlx::query("INSERT INTO organization_controls (organization_id) VALUES ($1)")
                .bind(user_id)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    pub async fn create_organization_account(
        &self,
        user_id: Uuid,
        auth_subject_hash: &[u8],
    ) -> Result<(), StoreError> {
        self.create_account_with_kind(user_id, auth_subject_hash, AccountKind::Organization)
            .await
    }
    pub async fn claim_handle(&self, user_id: Uuid, handle: &str) -> Result<(), StoreError> {
        validate_handle(handle)?;
        let mut tx = self.pool.begin().await?;
        require_active_account(&mut tx, user_id).await?;
        let current: Option<Uuid> =
            sqlx::query_scalar("SELECT user_id FROM handles WHERE handle = $1")
                .bind(handle)
                .fetch_optional(&mut *tx)
                .await?;
        if current == Some(user_id) {
            tx.commit().await?;
            return Ok(());
        }
        sqlx::query("INSERT INTO handles (handle, user_id) VALUES ($1, $2)")
            .bind(handle)
            .bind(user_id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }
    pub async fn resolve_handle(&self, handle: &str) -> Result<Option<Uuid>, StoreError> {
        validate_handle(handle)?;
        Ok(sqlx::query_scalar("SELECT h.user_id FROM handles h JOIN accounts a USING (user_id) WHERE handle = $1 AND a.disabled_at IS NULL")
            .bind(handle).fetch_optional(&self.pool).await?)
    }
    pub async fn lookup_handle_directory(
        &self,
        handle: &str,
    ) -> Result<Option<HandleDirectoryRecord>, StoreError> {
        validate_handle(handle)?;
        let rows = sqlx::query(
            "SELECT h.user_id,a.verification_badge,d.device_id,d.mls_node_id,d.identity_public_key,d.mls_credential,d.delegation_role,d.delegation_certificate FROM handles h JOIN accounts a USING (user_id) JOIN devices d USING (user_id) WHERE h.handle=$1 AND a.disabled_at IS NULL AND d.revoked_at IS NULL ORDER BY d.device_id",
        )
        .bind(handle)
        .fetch_all(&self.pool)
        .await?;
        let Some(first) = rows.first() else {
            return Ok(None);
        };
        let user_id: Uuid = first.get("user_id");
        let verification_badge: Option<Vec<u8>> = first.get("verification_badge");
        if let Some(bytes) = verification_badge.as_ref() {
            let badge = v1::VerificationBadge::decode(bytes.as_slice())
                .map_err(|_| StoreError::CorruptObject)?;
            protocol::validate_verification_badge(&badge)
                .map_err(|_| StoreError::CorruptObject)?;
            if badge.subject_user_id != user_id.to_string() {
                return Err(StoreError::CorruptObject);
            }
        }
        let mut devices = Vec::with_capacity(rows.len());
        for row in rows {
            if row.get::<Uuid, _>("user_id") != user_id {
                return Err(StoreError::CorruptObject);
            }
            let identity_public_key: Vec<u8> = row.get("identity_public_key");
            let mls_credential: Vec<u8> = row.get("mls_credential");
            if identity_public_key.len() != 32
                || mls_credential.is_empty()
                || mls_credential.len() > 65_536
            {
                return Err(StoreError::CorruptObject);
            }
            let did = protocol::did_key_for_ed25519(&identity_public_key)
                .map_err(|_| StoreError::CorruptObject)?;
            let delegation_role: String = row.get("delegation_role");
            let delegation_role_id = match delegation_role.as_str() {
                "owner" => 0,
                "device" => 1,
                "admin" => 2,
                _ => return Err(StoreError::CorruptObject),
            };
            let delegation_certificate: Option<Vec<u8>> = row.get("delegation_certificate");
            if let Some(bytes) = delegation_certificate.as_ref() {
                let certificate = v1::DeviceSubCertificate::decode(bytes.as_slice())
                    .map_err(|_| StoreError::CorruptObject)?;
                protocol::validate_device_subcertificate(&certificate)
                    .map_err(|_| StoreError::CorruptObject)?;
                if certificate.user_id != user_id.to_string()
                    || certificate.subject_device_id != row.get::<Uuid, _>("device_id").to_string()
                    || certificate.subject_mls_node_id
                        != row.get::<Uuid, _>("mls_node_id").to_string()
                    || certificate.subject_public_key != identity_public_key
                    || certificate.delegation_role != delegation_role_id
                {
                    return Err(StoreError::CorruptObject);
                }
            } else if delegation_role_id != 0 {
                return Err(StoreError::CorruptObject);
            }
            devices.push(DirectoryDeviceRecord {
                device_id: row.get("device_id"),
                mls_node_id: row.get("mls_node_id"),
                did,
                identity_public_key,
                mls_credential,
                delegation_role,
                delegation_certificate,
            });
        }
        Ok(Some(HandleDirectoryRecord {
            user_id,
            devices,
            verification_badge,
        }))
    }
    pub async fn register_device(
        &self,
        user_id: Uuid,
        device_id: Uuid,
        mls_node_id: Uuid,
        public_key: &[u8],
        credential: &[u8],
    ) -> Result<(), StoreError> {
        let mut tx = self.pool.begin().await?;
        require_active_account(&mut tx, user_id).await?;
        sqlx::query("INSERT INTO devices (device_id, user_id, mls_node_id, identity_public_key, mls_credential) VALUES ($1,$2,$3,$4,$5)")
            .bind(device_id).bind(user_id).bind(mls_node_id).bind(public_key).bind(credential).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(())
    }

    /// Atomically register a device authorized by a signed sub-certificate.
    /// The issuer must be an active owner, or an active admin delegating only
    /// a device role. The certificate is retained as public audit material.
    pub async fn register_delegated_device(
        &self,
        user_id: Uuid,
        certificate: &v1::DeviceSubCertificate,
        credential: &[u8],
    ) -> Result<(), StoreError> {
        protocol::validate_device_subcertificate(certificate)?;
        let certificate_user = Uuid::parse_str(&certificate.user_id)
            .map_err(|_| StoreError::Invalid)?;
        let issuer_device_id = Uuid::parse_str(&certificate.issuer_device_id)
            .map_err(|_| StoreError::Invalid)?;
        let issuer_mls_node_id = Uuid::parse_str(&certificate.issuer_mls_node_id)
            .map_err(|_| StoreError::Invalid)?;
        let subject_device_id = Uuid::parse_str(&certificate.subject_device_id)
            .map_err(|_| StoreError::Invalid)?;
        let subject_mls_node_id = Uuid::parse_str(&certificate.subject_mls_node_id)
            .map_err(|_| StoreError::Invalid)?;
        if certificate_user != user_id
            || subject_device_id.is_nil()
            || subject_mls_node_id.is_nil()
            || credential.is_empty()
            || credential.len() > 65_536
        {
            return Err(StoreError::Invalid);
        }
        let role = match certificate.delegation_role {
            1 => "device",
            2 => "admin",
            _ => return Err(StoreError::Invalid),
        };
        let mut tx = self.pool.begin().await?;
        require_active_account(&mut tx, user_id).await?;
        let issuer = sqlx::query(
            "SELECT user_id,mls_node_id,identity_public_key,delegation_role,revoked_at IS NULL AS active FROM devices WHERE device_id=$1 FOR UPDATE",
        )
        .bind(issuer_device_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(StoreError::Forbidden)?;
        if issuer.get::<Uuid, _>("user_id") != user_id
            || issuer.get::<Uuid, _>("mls_node_id") != issuer_mls_node_id
            || issuer.get::<Vec<u8>, _>("identity_public_key")
                != certificate.issuer_public_key
            || !issuer.get::<bool, _>("active")
        {
            return Err(StoreError::Forbidden);
        }
        match (issuer.get::<String, _>("delegation_role").as_str(), role) {
            ("owner", "device" | "admin") | ("admin", "device") => {}
            _ => return Err(StoreError::Forbidden),
        }
        let subject_exists: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM devices WHERE device_id=$1 OR mls_node_id=$2)")
                .bind(subject_device_id)
                .bind(subject_mls_node_id)
                .fetch_one(&mut *tx)
                .await?;
        if subject_exists {
            return Err(StoreError::Conflict);
        }
        sqlx::query("INSERT INTO devices (device_id,user_id,mls_node_id,identity_public_key,mls_credential,delegation_role,delegated_by_device_id,delegation_certificate) VALUES ($1,$2,$3,$4,$5,$6,$7,$8)")
            .bind(subject_device_id)
            .bind(user_id)
            .bind(subject_mls_node_id)
            .bind(&certificate.subject_public_key)
            .bind(credential)
            .bind(role)
            .bind(issuer_device_id)
            .bind(certificate.encode_to_vec())
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }

    /// Store the current authority-signed public badge for an active account.
    pub async fn put_verification_badge(
        &self,
        user_id: Uuid,
        badge: &v1::VerificationBadge,
    ) -> Result<(), StoreError> {
        protocol::validate_verification_badge(badge)?;
        if badge.subject_user_id != user_id.to_string() {
            return Err(StoreError::Invalid);
        }
        let mut tx = self.pool.begin().await?;
        require_active_account(&mut tx, user_id).await?;
        let updated = sqlx::query(
            "UPDATE accounts SET verification_badge=$2 WHERE user_id=$1 AND disabled_at IS NULL",
        )
        .bind(user_id)
        .bind(badge.encode_to_vec())
        .execute(&mut *tx)
        .await?;
        if updated.rows_affected() != 1 {
            return Err(StoreError::NotFound);
        }
        tx.commit().await?;
        Ok(())
    }

    /// Remove a badge during manual revocation. The badge body is never
    /// replaced with a false value; absence means unverified.
    pub async fn clear_verification_badge(&self, user_id: Uuid) -> Result<(), StoreError> {
        let updated = sqlx::query("UPDATE accounts SET verification_badge=NULL WHERE user_id=$1")
            .bind(user_id)
            .execute(&self.pool)
            .await?;
        if updated.rows_affected() != 1 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }
    pub async fn active_devices(&self, user_id: Uuid) -> Result<Vec<Uuid>, StoreError> {
        Ok(sqlx::query_scalar("SELECT d.device_id FROM devices d JOIN accounts a USING (user_id) WHERE d.user_id = $1 AND d.revoked_at IS NULL AND a.disabled_at IS NULL ORDER BY d.device_id")
            .bind(user_id).fetch_all(&self.pool).await?)
    }
    pub async fn revoke_device(
        &self,
        actor_user_id: Uuid,
        device_id: Uuid,
    ) -> Result<(), StoreError> {
        let result = sqlx::query("WITH RECURSIVE descendants AS (SELECT device_id FROM devices WHERE device_id=$1 AND user_id=$2 UNION ALL SELECT child.device_id FROM devices child JOIN descendants parent ON child.delegated_by_device_id=parent.device_id WHERE child.user_id=$2) UPDATE devices SET revoked_at=COALESCE(revoked_at,now()) WHERE device_id IN (SELECT device_id FROM descendants) AND user_id=$2")
            .bind(device_id).bind(actor_user_id).execute(&self.pool).await?;
        if result.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    pub async fn register_passkey_credential(
        &self,
        user_id: Uuid,
        credential_id: &[u8],
        public_key: &[u8],
        sign_count: u32,
    ) -> Result<(), StoreError> {
        if credential_id.is_empty() || credential_id.len() > 1024 || public_key.len() != 64 {
            return Err(StoreError::Invalid);
        }
        let mut tx = self.pool.begin().await?;
        require_active_account(&mut tx, user_id).await?;
        sqlx::query(
            "INSERT INTO passkey_credentials (user_id,credential_id,public_key,sign_count) VALUES ($1,$2,$3,$4)",
        )
        .bind(user_id)
        .bind(credential_id)
        .bind(public_key)
        .bind(i64::from(sign_count))
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn passkey_credential(
        &self,
        user_id: Uuid,
        credential_id: &[u8],
    ) -> Result<Option<PasskeyCredentialRecord>, StoreError> {
        let row = sqlx::query(
            "SELECT credential_id,public_key,sign_count FROM passkey_credentials pc JOIN accounts a USING (user_id) WHERE pc.user_id=$1 AND pc.credential_id=$2 AND a.disabled_at IS NULL",
        )
        .bind(user_id)
        .bind(credential_id)
        .fetch_optional(&self.pool)
        .await?;
        row.map(|row| {
            let public_key: [u8; 64] = row
                .get::<Vec<u8>, _>("public_key")
                .try_into()
                .map_err(|_| StoreError::Invalid)?;
            let sign_count = row.get::<i64, _>("sign_count");
            if !(0..=u32::MAX as i64).contains(&sign_count) {
                return Err(StoreError::Invalid);
            }
            Ok(PasskeyCredentialRecord {
                credential_id: row.get("credential_id"),
                public_key,
                sign_count: sign_count as u32,
            })
        })
        .transpose()
    }

    pub async fn put_encrypted_key_backup(
        &self,
        user_id: Uuid,
        device_id: Uuid,
        backup_id: Uuid,
        credential_id: &[u8],
        encrypted_envelope: &[u8],
    ) -> Result<(), StoreError> {
        if backup_id.is_nil()
            || device_id.is_nil()
            || credential_id.is_empty()
            || credential_id.len() > 1024
            || !(128..=8192).contains(&encrypted_envelope.len())
        {
            return Err(StoreError::Invalid);
        }
        let mut tx = self.pool.begin().await?;
        require_active_account(&mut tx, user_id).await?;
        let existing = sqlx::query(
            "SELECT backup_id,credential_id,encrypted_envelope FROM encrypted_key_backups WHERE user_id=$1 AND device_id=$2 FOR UPDATE",
        )
        .bind(user_id)
        .bind(device_id)
        .fetch_optional(&mut *tx)
        .await?;
        if let Some(existing) = existing {
            if existing.get::<Uuid, _>("backup_id") != backup_id
                || existing.get::<Vec<u8>, _>("credential_id") != credential_id
                || existing.get::<Vec<u8>, _>("encrypted_envelope") != encrypted_envelope
            {
                return Err(StoreError::Conflict);
            }
            tx.commit().await?;
            return Ok(());
        }
        sqlx::query(
            "INSERT INTO encrypted_key_backups (backup_id,user_id,device_id,credential_id,encrypted_envelope) VALUES ($1,$2,$3,$4,$5)",
        )
        .bind(backup_id)
        .bind(user_id)
        .bind(device_id)
        .bind(credential_id)
        .bind(encrypted_envelope)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn encrypted_key_backup(
        &self,
        user_id: Uuid,
        backup_id: Uuid,
    ) -> Result<EncryptedKeyBackupRecord, StoreError> {
        let row = sqlx::query(
            "SELECT backup_id,device_id,credential_id,encrypted_envelope FROM encrypted_key_backups WHERE user_id=$1 AND backup_id=$2",
        )
        .bind(user_id)
        .bind(backup_id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or(StoreError::NotFound)?;
        Ok(EncryptedKeyBackupRecord {
            backup_id: row.get("backup_id"),
            device_id: row.get("device_id"),
            credential_id: row.get("credential_id"),
            encrypted_envelope: row.get("encrypted_envelope"),
        })
    }
    pub async fn prekey_inventory(
        &self,
        user_id: Uuid,
        device_id: Uuid,
    ) -> Result<v1::PreKeyInventory, StoreError> {
        let row = sqlx::query(
            "SELECT p.profile_revision,
                (SELECT count(*) FROM device_curve_one_time_prekeys c WHERE c.device_id=p.device_id) AS curve_count,
                (SELECT count(*) FROM device_kem_one_time_prekeys k WHERE k.device_id=p.device_id) AS kem_count
             FROM device_prekey_profiles p
             JOIN devices d USING (device_id)
             JOIN accounts a USING (user_id)
             WHERE p.device_id=$1 AND d.user_id=$2 AND d.revoked_at IS NULL AND a.disabled_at IS NULL",
        )
        .bind(device_id)
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await?;
        let Some(row) = row else {
            let owns_device: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM devices d JOIN accounts a USING (user_id) WHERE d.device_id=$1 AND d.user_id=$2 AND d.revoked_at IS NULL AND a.disabled_at IS NULL)",
            )
            .bind(device_id)
            .bind(user_id)
            .fetch_one(&self.pool)
            .await?;
            if !owns_device {
                return Err(StoreError::Forbidden);
            }
            return Ok(v1::PreKeyInventory {
                protocol_version: links_protocol::VERSION,
                device_id: device_id.to_string(),
                profile_revision: 0,
                one_time_curve_prekeys: 0,
                one_time_kem_prekeys: 0,
            });
        };
        Ok(v1::PreKeyInventory {
            protocol_version: links_protocol::VERSION,
            device_id: device_id.to_string(),
            profile_revision: row.get::<i64, _>("profile_revision") as u64,
            one_time_curve_prekeys: row.get::<i64, _>("curve_count") as u32,
            one_time_kem_prekeys: row.get::<i64, _>("kem_count") as u32,
        })
    }
    pub async fn upload_prekeys(
        &self,
        user_id: Uuid,
        device_id: Uuid,
        upload: &v1::PreKeyUpload,
    ) -> Result<v1::PreKeyInventory, StoreError> {
        validate_prekey_upload(upload)?;
        if upload.device_id != device_id.to_string() {
            return Err(StoreError::Forbidden);
        }
        let profile = upload.profile.as_ref().ok_or(StoreError::Invalid)?;
        let identity = profile.identity.as_ref().ok_or(StoreError::Invalid)?;
        let signed = profile.signed_prekey.as_ref().ok_or(StoreError::Invalid)?;
        let signed_key = signed.prekey.as_ref().ok_or(StoreError::Invalid)?;
        let last = profile
            .last_resort_kem_prekey
            .as_ref()
            .ok_or(StoreError::Invalid)?;
        let upload_id = Uuid::parse_str(&upload.upload_id).map_err(|_| StoreError::Invalid)?;
        let upload_digest = Sha256::digest(upload.encode_to_vec());
        let revision = upload.profile_revision as i64;
        let mut tx = self.pool.begin().await?;
        let enrolled_signing_key: Option<Vec<u8>> = sqlx::query_scalar(
            "SELECT d.identity_public_key FROM devices d JOIN accounts a USING (user_id) WHERE d.device_id=$1 AND d.user_id=$2 AND d.revoked_at IS NULL AND a.disabled_at IS NULL FOR UPDATE OF d",
        )
        .bind(device_id)
        .bind(user_id)
        .fetch_optional(&mut *tx)
        .await?;
        if enrolled_signing_key.as_deref() != Some(identity.signing_key.as_slice()) {
            return Err(StoreError::Forbidden);
        }

        let current = sqlx::query("SELECT * FROM device_prekey_profiles WHERE device_id=$1")
            .bind(device_id)
            .fetch_optional(&mut *tx)
            .await?;
        if let Some(current) = current {
            let current_revision: i64 = current.get("profile_revision");
            if current_revision > revision {
                return Err(StoreError::Conflict);
            }
            if current_revision == revision
                && (current.get::<Vec<u8>, _>("identity_dh_key") != identity.dh_key
                    || current.get::<Vec<u8>, _>("identity_binding_signature")
                        != identity.binding_signature
                    || current.get::<i64, _>("signed_curve_prekey_id") != signed_key.id as i64
                    || current.get::<Vec<u8>, _>("signed_curve_prekey") != signed_key.public_key
                    || current.get::<Vec<u8>, _>("signed_curve_signature") != signed.signature
                    || current.get::<i64, _>("last_resort_kem_prekey_id") != last.id as i64
                    || current.get::<Vec<u8>, _>("last_resort_kem_prekey") != last.public_key
                    || current.get::<Vec<u8>, _>("last_resort_kem_signature") != last.signature)
            {
                return Err(StoreError::Conflict);
            }
            if current_revision < revision {
                sqlx::query("DELETE FROM device_prekey_uploads WHERE device_id=$1")
                    .bind(device_id)
                    .execute(&mut *tx)
                    .await?;
                sqlx::query("DELETE FROM device_curve_one_time_prekeys WHERE device_id=$1")
                    .bind(device_id)
                    .execute(&mut *tx)
                    .await?;
                sqlx::query("DELETE FROM device_kem_one_time_prekeys WHERE device_id=$1")
                    .bind(device_id)
                    .execute(&mut *tx)
                    .await?;
            }
        }
        let prior_digest: Option<Vec<u8>> = sqlx::query_scalar(
            "SELECT upload_digest FROM device_prekey_uploads WHERE device_id=$1 AND upload_id=$2 AND profile_revision=$3",
        )
        .bind(device_id)
        .bind(upload_id)
        .bind(revision)
        .fetch_optional(&mut *tx)
        .await?;
        if let Some(prior_digest) = prior_digest {
            if prior_digest != upload_digest.as_slice() {
                return Err(StoreError::Conflict);
            }
            let curve_count: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM device_curve_one_time_prekeys WHERE device_id=$1",
            )
            .bind(device_id)
            .fetch_one(&mut *tx)
            .await?;
            let kem_count: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM device_kem_one_time_prekeys WHERE device_id=$1",
            )
            .bind(device_id)
            .fetch_one(&mut *tx)
            .await?;
            tx.commit().await?;
            return Ok(v1::PreKeyInventory {
                protocol_version: links_protocol::VERSION,
                device_id: device_id.to_string(),
                profile_revision: upload.profile_revision,
                one_time_curve_prekeys: curve_count as u32,
                one_time_kem_prekeys: kem_count as u32,
            });
        }
        sqlx::query(
            "INSERT INTO device_prekey_profiles (device_id,profile_revision,identity_dh_key,identity_binding_signature,signed_curve_prekey_id,signed_curve_prekey,signed_curve_signature,last_resort_kem_prekey_id,last_resort_kem_prekey,last_resort_kem_signature)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)
             ON CONFLICT (device_id) DO UPDATE SET profile_revision=EXCLUDED.profile_revision,identity_dh_key=EXCLUDED.identity_dh_key,identity_binding_signature=EXCLUDED.identity_binding_signature,signed_curve_prekey_id=EXCLUDED.signed_curve_prekey_id,signed_curve_prekey=EXCLUDED.signed_curve_prekey,signed_curve_signature=EXCLUDED.signed_curve_signature,last_resort_kem_prekey_id=EXCLUDED.last_resort_kem_prekey_id,last_resort_kem_prekey=EXCLUDED.last_resort_kem_prekey,last_resort_kem_signature=EXCLUDED.last_resort_kem_signature,updated_at=now()",
        )
        .bind(device_id)
        .bind(revision)
        .bind(&identity.dh_key)
        .bind(&identity.binding_signature)
        .bind(signed_key.id as i64)
        .bind(&signed_key.public_key)
        .bind(&signed.signature)
        .bind(last.id as i64)
        .bind(&last.public_key)
        .bind(&last.signature)
        .execute(&mut *tx)
        .await?;

        for key in &upload.one_time_curve_prekeys {
            let result = sqlx::query("INSERT INTO device_curve_one_time_prekeys (device_id,prekey_id,public_key) VALUES ($1,$2,$3) ON CONFLICT DO NOTHING")
                .bind(device_id).bind(key.id as i64).bind(&key.public_key).execute(&mut *tx).await?;
            if result.rows_affected() == 0 {
                let same: bool = sqlx::query_scalar("SELECT public_key=$3 FROM device_curve_one_time_prekeys WHERE device_id=$1 AND prekey_id=$2")
                    .bind(device_id).bind(key.id as i64).bind(&key.public_key).fetch_one(&mut *tx).await?;
                if !same {
                    return Err(StoreError::Conflict);
                }
            }
        }
        for key in &upload.one_time_kem_prekeys {
            let result = sqlx::query("INSERT INTO device_kem_one_time_prekeys (device_id,prekey_id,public_key,signature) VALUES ($1,$2,$3,$4) ON CONFLICT DO NOTHING")
                .bind(device_id).bind(key.id as i64).bind(&key.public_key).bind(&key.signature).execute(&mut *tx).await?;
            if result.rows_affected() == 0 {
                let same: bool = sqlx::query_scalar("SELECT public_key=$3 AND signature=$4 FROM device_kem_one_time_prekeys WHERE device_id=$1 AND prekey_id=$2")
                    .bind(device_id).bind(key.id as i64).bind(&key.public_key).bind(&key.signature).fetch_one(&mut *tx).await?;
                if !same {
                    return Err(StoreError::Conflict);
                }
            }
        }
        let curve_count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM device_curve_one_time_prekeys WHERE device_id=$1",
        )
        .bind(device_id)
        .fetch_one(&mut *tx)
        .await?;
        let kem_count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM device_kem_one_time_prekeys WHERE device_id=$1",
        )
        .bind(device_id)
        .fetch_one(&mut *tx)
        .await?;
        if curve_count > MAX_ONE_TIME_PREKEYS as i64 || kem_count > MAX_ONE_TIME_PREKEYS as i64 {
            return Err(StoreError::Invalid);
        }
        sqlx::query("INSERT INTO device_prekey_uploads (device_id,upload_id,profile_revision,upload_digest) VALUES ($1,$2,$3,$4)")
            .bind(device_id).bind(upload_id).bind(revision).bind(upload_digest.as_slice()).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(v1::PreKeyInventory {
            protocol_version: links_protocol::VERSION,
            device_id: device_id.to_string(),
            profile_revision: upload.profile_revision,
            one_time_curve_prekeys: curve_count as u32,
            one_time_kem_prekeys: kem_count as u32,
        })
    }
    pub async fn claim_prekey_bundle(
        &self,
        target_device_id: Uuid,
    ) -> Result<v1::PreKeyBundle, StoreError> {
        let mut tx = self.pool.begin().await?;
        let row = sqlx::query(
            "SELECT p.*,d.identity_public_key FROM device_prekey_profiles p JOIN devices d USING (device_id) JOIN accounts a USING (user_id) WHERE p.device_id=$1 AND d.revoked_at IS NULL AND a.disabled_at IS NULL FOR UPDATE OF p",
        )
        .bind(target_device_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(StoreError::NotFound)?;
        let curve = sqlx::query("DELETE FROM device_curve_one_time_prekeys WHERE (device_id,prekey_id) IN (SELECT device_id,prekey_id FROM device_curve_one_time_prekeys WHERE device_id=$1 ORDER BY created_at,prekey_id LIMIT 1 FOR UPDATE) RETURNING prekey_id,public_key")
            .bind(target_device_id).fetch_optional(&mut *tx).await?;
        let kem = sqlx::query("DELETE FROM device_kem_one_time_prekeys WHERE (device_id,prekey_id) IN (SELECT device_id,prekey_id FROM device_kem_one_time_prekeys WHERE device_id=$1 ORDER BY created_at,prekey_id LIMIT 1 FOR UPDATE) RETURNING prekey_id,public_key,signature")
            .bind(target_device_id).fetch_optional(&mut *tx).await?;
        let identity = v1::PqxdhPublicIdentity {
            signing_key: row.get("identity_public_key"),
            dh_key: row.get("identity_dh_key"),
            binding_signature: row.get("identity_binding_signature"),
        };
        let signed_prekey = v1::SignedCurvePreKey {
            prekey: Some(v1::CurvePreKey {
                id: row.get::<i64, _>("signed_curve_prekey_id") as u64,
                public_key: row.get("signed_curve_prekey"),
            }),
            signature: row.get("signed_curve_signature"),
        };
        let last_resort = v1::KemPreKey {
            id: row.get::<i64, _>("last_resort_kem_prekey_id") as u64,
            public_key: row.get("last_resort_kem_prekey"),
            one_time: false,
            signature: row.get("last_resort_kem_signature"),
        };
        let one_time_curve_prekey = curve.map(|key| v1::CurvePreKey {
            id: key.get::<i64, _>("prekey_id") as u64,
            public_key: key.get("public_key"),
        });
        let kem_prekey = kem.map_or(last_resort.clone(), |key| v1::KemPreKey {
            id: key.get::<i64, _>("prekey_id") as u64,
            public_key: key.get("public_key"),
            one_time: true,
            signature: key.get("signature"),
        });
        let bundle = v1::PreKeyBundle {
            protocol_version: links_protocol::VERSION,
            device_id: target_device_id.to_string(),
            profile_revision: row.get::<i64, _>("profile_revision") as u64,
            profile: Some(v1::PreKeyProfile {
                identity: Some(identity),
                signed_prekey: Some(signed_prekey),
                last_resort_kem_prekey: Some(last_resort),
            }),
            one_time_curve_prekey,
            kem_prekey: Some(kem_prekey),
        };
        links_protocol::validate_prekey_bundle(&bundle)?;
        tx.commit().await?;
        Ok(bundle)
    }
    pub async fn create_group(
        &self,
        group_id: Uuid,
        owner: Uuid,
        kind: GroupKind,
    ) -> Result<(), StoreError> {
        let mut tx = self.pool.begin().await?;
        require_active_account(&mut tx, owner).await?;
        sqlx::query("INSERT INTO groups (group_id, group_kind) VALUES ($1,$2)")
            .bind(group_id)
            .bind(kind.as_str())
            .execute(&mut *tx)
            .await?;
        sqlx::query(
            "INSERT INTO group_memberships (group_id, user_id, role) VALUES ($1,$2,'owner')",
        )
        .bind(group_id)
        .bind(owner)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }
    pub async fn role(&self, group_id: Uuid, user_id: Uuid) -> Result<Option<Role>, StoreError> {
        let value: Option<String> = sqlx::query_scalar(
            "SELECT role FROM group_memberships WHERE group_id = $1 AND user_id = $2",
        )
        .bind(group_id)
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await?;
        value.as_deref().map(Role::parse).transpose()
    }

    /// Owner: all roles. Admin: add/update members only. Member: no grants.
    /// API authentication must supply actor; never trust an actor ID from a body.
    pub async fn set_role(
        &self,
        group_id: Uuid,
        actor: Uuid,
        target: Uuid,
        role: Role,
    ) -> Result<(), StoreError> {
        let mut tx = self.pool.begin().await?;
        lock_group(&mut tx, group_id).await?;
        require_active_account(&mut tx, actor).await?;
        require_active_account(&mut tx, target).await?;
        let actor_role = role_in(&mut tx, group_id, actor).await?;
        let current = role_in(&mut tx, group_id, target).await?;
        match actor_role {
            Some(Role::Owner) => {}
            Some(Role::Admin)
                if role == Role::Member && matches!(current, None | Some(Role::Member)) => {}
            _ => return Err(StoreError::Forbidden),
        }
        if current == Some(Role::Owner) && role != Role::Owner {
            require_other_owner(&mut tx, group_id).await?;
        }
        sqlx::query("INSERT INTO group_memberships (group_id,user_id,role) VALUES ($1,$2,$3) ON CONFLICT (group_id,user_id) DO UPDATE SET role = EXCLUDED.role")
            .bind(group_id).bind(target).bind(role.as_str()).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(())
    }
    pub async fn remove_member(
        &self,
        group_id: Uuid,
        actor: Uuid,
        target: Uuid,
    ) -> Result<(), StoreError> {
        let mut tx = self.pool.begin().await?;
        lock_group(&mut tx, group_id).await?;
        require_active_account(&mut tx, actor).await?;
        let actor_role = role_in(&mut tx, group_id, actor).await?;
        let current = role_in(&mut tx, group_id, target)
            .await?
            .ok_or(StoreError::NotFound)?;
        if actor != target
            && actor_role != Some(Role::Owner)
            && !(actor_role == Some(Role::Admin) && current == Role::Member)
        {
            return Err(StoreError::Forbidden);
        }
        if current == Role::Owner {
            require_other_owner(&mut tx, group_id).await?;
        }
        sqlx::query("DELETE FROM group_memberships WHERE group_id=$1 AND user_id=$2")
            .bind(group_id)
            .bind(target)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }

    /// Return the current account-level membership snapshot to a group member.
    /// MLS leaves remain private to clients; this is only the RBAC control plane.
    pub async fn group_members(
        &self,
        group_id: Uuid,
        actor: Uuid,
    ) -> Result<Vec<GroupMemberRecord>, StoreError> {
        let mut tx = self.pool.begin().await?;
        lock_group(&mut tx, group_id).await?;
        require_active_account(&mut tx, actor).await?;
        if role_in(&mut tx, group_id, actor).await?.is_none() {
            return Err(StoreError::Forbidden);
        }
        let rows = sqlx::query(
            "SELECT user_id,role FROM group_memberships WHERE group_id=$1 ORDER BY joined_at,user_id",
        )
        .bind(group_id)
        .fetch_all(&mut *tx)
        .await?;
        let members = rows
            .into_iter()
            .map(|row| {
                Ok(GroupMemberRecord {
                    user_id: row.get("user_id"),
                    role: Role::parse(row.get("role"))?,
                })
            })
            .collect::<Result<Vec<_>, StoreError>>()?;
        tx.commit().await?;
        Ok(members)
    }

    /// Read organization feature gates for an active organization device.
    /// Missing rows are repaired to the safe disabled default.
    pub async fn organization_controls(
        &self,
        organization_id: Uuid,
        device_id: Uuid,
    ) -> Result<OrganizationControls, StoreError> {
        let mut tx = self.pool.begin().await?;
        let account = sqlx::query(
            "SELECT a.account_kind FROM accounts a JOIN devices d ON d.user_id=a.user_id WHERE a.user_id=$1 AND d.device_id=$2 AND a.disabled_at IS NULL AND d.revoked_at IS NULL FOR SHARE",
        )
        .bind(organization_id)
        .bind(device_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(StoreError::Forbidden)?;
        if account.get::<String, _>("account_kind") != AccountKind::Organization.as_str() {
            return Err(StoreError::Forbidden);
        }
        sqlx::query(
            "INSERT INTO organization_controls (organization_id) VALUES ($1) ON CONFLICT (organization_id) DO NOTHING",
        )
        .bind(organization_id)
        .execute(&mut *tx)
        .await?;
        let row = sqlx::query(
            "SELECT mini_apps_enabled,bots_enabled,revision FROM organization_controls WHERE organization_id=$1",
        )
        .bind(organization_id)
        .fetch_one(&mut *tx)
        .await?;
        let revision = row.get::<i64, _>("revision");
        if revision <= 0 {
            return Err(StoreError::CorruptObject);
        }
        let controls = OrganizationControls {
            organization_id,
            mini_apps_enabled: row.get("mini_apps_enabled"),
            bots_enabled: row.get("bots_enabled"),
            revision: revision as u64,
        };
        tx.commit().await?;
        Ok(controls)
    }

    /// Owners and delegated organization admins may change feature exposure.
    /// A revision lets clients reject stale control responses.
    pub async fn set_organization_controls(
        &self,
        organization_id: Uuid,
        device_id: Uuid,
        mini_apps_enabled: bool,
        bots_enabled: bool,
    ) -> Result<OrganizationControls, StoreError> {
        let mut tx = self.pool.begin().await?;
        let account = sqlx::query(
            "SELECT a.account_kind,d.delegation_role FROM accounts a JOIN devices d ON d.user_id=a.user_id WHERE a.user_id=$1 AND d.device_id=$2 AND a.disabled_at IS NULL AND d.revoked_at IS NULL FOR UPDATE OF a,d",
        )
        .bind(organization_id)
        .bind(device_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(StoreError::Forbidden)?;
        if account.get::<String, _>("account_kind") != AccountKind::Organization.as_str()
            || !matches!(
                account.get::<String, _>("delegation_role").as_str(),
                "owner" | "admin"
            )
        {
            return Err(StoreError::Forbidden);
        }
        sqlx::query(
            "INSERT INTO organization_controls (organization_id) VALUES ($1) ON CONFLICT (organization_id) DO NOTHING",
        )
        .bind(organization_id)
        .execute(&mut *tx)
        .await?;
        let row = sqlx::query(
            "UPDATE organization_controls SET mini_apps_enabled=$2,bots_enabled=$3,revision=revision+1,updated_at=now() WHERE organization_id=$1 RETURNING mini_apps_enabled,bots_enabled,revision",
        )
        .bind(organization_id)
        .bind(mini_apps_enabled)
        .bind(bots_enabled)
        .fetch_one(&mut *tx)
        .await?;
        let revision = row.get::<i64, _>("revision");
        if revision <= 0 {
            return Err(StoreError::CorruptObject);
        }
        let controls = OrganizationControls {
            organization_id,
            mini_apps_enabled: row.get("mini_apps_enabled"),
            bots_enabled: row.get("bots_enabled"),
            revision: revision as u64,
        };
        tx.commit().await?;
        Ok(controls)
    }
}

#[async_trait]
impl EncryptedPayloadStore for RelationalStore {
    async fn append(&self, request: AppendRequest) -> Result<AppendResult, StoreError> {
        let envelope = request.envelope();
        let recipient_device_id =
            Uuid::parse_str(&envelope.recipient_device_id).map_err(|_| StoreError::Invalid)?;
        let envelope_id =
            Uuid::parse_str(&envelope.envelope_id).map_err(|_| StoreError::Invalid)?;
        let envelope_bytes = envelope.encode_to_vec();
        let fingerprint = Sha256::digest(&envelope_bytes);
        let accepted_at_ms = signed_cursor(request.accepted_at_ms())?;
        let expires_at_ms = signed_cursor(envelope.expires_at_ms)?;
        let mut tx = self.pool.begin().await?;

        sqlx::query(
            "INSERT INTO encrypted_payload_cursors (recipient_device_id) VALUES ($1) ON CONFLICT DO NOTHING",
        )
        .bind(recipient_device_id)
        .execute(&mut *tx)
        .await?;
        let high_watermark: i64 = sqlx::query_scalar(
            "SELECT high_watermark FROM encrypted_payload_cursors WHERE recipient_device_id=$1 FOR UPDATE",
        )
        .bind(recipient_device_id)
        .fetch_one(&mut *tx)
        .await?;

        let existing = sqlx::query(
            "SELECT cursor,envelope_fingerprint FROM encrypted_payloads WHERE recipient_device_id=$1 AND envelope_id=$2 FOR UPDATE",
        )
        .bind(recipient_device_id)
        .bind(envelope_id)
        .fetch_optional(&mut *tx)
        .await?;
        if let Some(existing) = existing {
            let existing_fingerprint: Vec<u8> = existing.get("envelope_fingerprint");
            if existing_fingerprint != fingerprint.as_slice() {
                return Err(StoreError::Conflict);
            }
            let cursor = existing.get::<i64, _>("cursor");
            tx.commit().await?;
            return Ok(AppendResult {
                cursor: unsigned_cursor(cursor)?,
                duplicate: true,
            });
        }

        if high_watermark < 0 || high_watermark >= MAX_CURSOR as i64 {
            return Err(StoreError::Conflict);
        }
        let cursor = high_watermark + 1;
        sqlx::query(
            "INSERT INTO encrypted_payloads (recipient_device_id,cursor,envelope_id,envelope_bytes,envelope_fingerprint,accepted_at_ms,expires_at_ms,state) VALUES ($1,$2,$3,$4,$5,$6,$7,'live')",
        )
        .bind(recipient_device_id)
        .bind(cursor)
        .bind(envelope_id)
        .bind(envelope_bytes)
        .bind(fingerprint.as_slice())
        .bind(accepted_at_ms)
        .bind(expires_at_ms)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "UPDATE encrypted_payload_cursors SET high_watermark=$2 WHERE recipient_device_id=$1",
        )
        .bind(recipient_device_id)
        .bind(cursor)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(AppendResult {
            cursor: unsigned_cursor(cursor)?,
            duplicate: false,
        })
    }

    async fn read(&self, request: ReadRequest) -> Result<v1::SyncBatch, StoreError> {
        let device_id = Uuid::parse_str(request.device_id()).map_err(|_| StoreError::Invalid)?;
        let after_cursor = signed_cursor(request.after_cursor())?;
        let now_ms = signed_cursor(request.now_ms())?;
        let mut tx = self.pool.begin().await?;
        let high_watermark: Option<i64> = sqlx::query_scalar(
            "SELECT high_watermark FROM encrypted_payload_cursors WHERE recipient_device_id=$1",
        )
        .bind(device_id)
        .fetch_optional(&mut *tx)
        .await?;
        let high_watermark = high_watermark.unwrap_or(0);
        if high_watermark < 0 || after_cursor > high_watermark {
            return Err(StoreError::Invalid);
        }
        if after_cursor < high_watermark {
            let next_exists: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM encrypted_payloads WHERE recipient_device_id=$1 AND cursor=$2)",
            )
            .bind(device_id)
            .bind(after_cursor + 1)
            .fetch_one(&mut *tx)
            .await?;
            if !next_exists {
                return Err(StoreError::CursorExpired);
            }
        }

        let rows = sqlx::query(
            "SELECT cursor,envelope_bytes,state,expires_at_ms FROM encrypted_payloads WHERE recipient_device_id=$1 AND cursor>$2 ORDER BY cursor LIMIT $3",
        )
        .bind(device_id)
        .bind(after_cursor)
        .bind(i64::from(request.limit()))
        .fetch_all(&mut *tx)
        .await?;
        let mut items = Vec::with_capacity(rows.len());
        let mut expected = after_cursor + 1;
        for row in rows {
            let cursor: i64 = row.get("cursor");
            if cursor != expected {
                return Err(StoreError::CursorExpired);
            }
            expected = expected.checked_add(1).ok_or(StoreError::Conflict)?;
            let state: String = row.get("state");
            let expires_at_ms: i64 = row.get("expires_at_ms");
            let entry = if state == "acknowledged" || state == "expired" {
                Some(v1::queue_item::Entry::Tombstone(v1::Tombstone {
                    reason: if state == "acknowledged" {
                        v1::tombstone::Reason::Acknowledged as i32
                    } else {
                        v1::tombstone::Reason::Expired as i32
                    },
                }))
            } else if state == "live" && expires_at_ms > now_ms {
                let bytes: Vec<u8> = row.get("envelope_bytes");
                let envelope = protocol::decode_envelope(&bytes)?;
                if envelope.recipient_device_id != request.device_id() {
                    return Err(StoreError::Unavailable);
                }
                Some(v1::queue_item::Entry::Envelope(envelope))
            } else if state == "live" {
                Some(v1::queue_item::Entry::Tombstone(v1::Tombstone {
                    reason: v1::tombstone::Reason::Expired as i32,
                }))
            } else {
                return Err(StoreError::Unavailable);
            };
            let item = v1::QueueItem {
                cursor: unsigned_cursor(cursor)?,
                entry,
            };
            let mut candidate = v1::SyncBatch {
                recipient_device_id: request.device_id().to_owned(),
                after_cursor: request.after_cursor(),
                next_cursor: item.cursor,
                high_watermark: unsigned_cursor(high_watermark)?,
                items: items.clone(),
            };
            candidate.items.push(item.clone());
            if candidate.encoded_len() > MAX_FRAME_BYTES - 128 {
                if items.is_empty() {
                    return Err(StoreError::Invalid);
                }
                break;
            }
            items.push(item);
        }
        let next_cursor = items
            .last()
            .map_or(request.after_cursor(), |item| item.cursor);
        let batch = v1::SyncBatch {
            recipient_device_id: request.device_id().to_owned(),
            after_cursor: request.after_cursor(),
            next_cursor,
            high_watermark: unsigned_cursor(high_watermark)?,
            items,
        };
        if batch.items.is_empty() && batch.next_cursor != batch.high_watermark {
            return Err(StoreError::CursorExpired);
        }
        tx.commit().await?;
        Ok(batch)
    }

    async fn acknowledge(
        &self,
        device_id: &str,
        through_cursor: u64,
        now_ms: u64,
    ) -> Result<(), StoreError> {
        let device_id = Uuid::parse_str(device_id).map_err(|_| StoreError::Invalid)?;
        let through_cursor = signed_cursor(through_cursor)?;
        let now_ms = signed_cursor(now_ms)?;
        let mut tx = self.pool.begin().await?;
        let high_watermark: Option<i64> = sqlx::query_scalar(
            "SELECT high_watermark FROM encrypted_payload_cursors WHERE recipient_device_id=$1 FOR UPDATE",
        )
        .bind(device_id)
        .fetch_optional(&mut *tx)
        .await?;
        if through_cursor > high_watermark.unwrap_or(0) {
            return Err(StoreError::Invalid);
        }
        sqlx::query(
            "UPDATE encrypted_payloads SET state=CASE WHEN expires_at_ms <= $3 THEN 'expired' ELSE 'acknowledged' END, envelope_bytes=NULL WHERE recipient_device_id=$1 AND cursor <= $2 AND state='live'",
        )
        .bind(device_id)
        .bind(through_cursor)
        .bind(now_ms)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }

    async fn purge_expired(&self, now_ms: u64, limit: u32) -> Result<u64, StoreError> {
        if limit == 0 || limit > MAX_PURGE_BATCH {
            return Err(StoreError::Invalid);
        }
        let now_ms = signed_cursor(now_ms)?;
        let cutoff = now_ms.saturating_sub(MAX_RETENTION_MS as i64);
        let mut tx = self.pool.begin().await?;
        let expired = sqlx::query(
            "WITH candidates AS (SELECT recipient_device_id,cursor FROM encrypted_payloads WHERE state='live' AND expires_at_ms <= $1 ORDER BY expires_at_ms,cursor LIMIT $2 FOR UPDATE SKIP LOCKED) UPDATE encrypted_payloads p SET state='expired',envelope_bytes=NULL FROM candidates c WHERE p.recipient_device_id=c.recipient_device_id AND p.cursor=c.cursor",
        )
        .bind(now_ms)
        .bind(i64::from(limit))
        .execute(&mut *tx)
        .await?
        .rows_affected();
        let remaining = u64::from(limit).saturating_sub(expired);
        let deleted = if remaining == 0 {
            0
        } else {
            sqlx::query(
                "WITH candidates AS (SELECT recipient_device_id,cursor FROM encrypted_payloads WHERE state <> 'live' AND accepted_at_ms <= $1 ORDER BY accepted_at_ms,cursor LIMIT $2 FOR UPDATE SKIP LOCKED) DELETE FROM encrypted_payloads p USING candidates c WHERE p.recipient_device_id=c.recipient_device_id AND p.cursor=c.cursor",
            )
            .bind(cutoff)
            .bind(i64::try_from(remaining).map_err(|_| StoreError::Invalid)?)
            .execute(&mut *tx)
            .await?
            .rows_affected()
        };
        tx.commit().await?;
        Ok(expired + deleted)
    }
}

fn signed_cursor(value: u64) -> Result<i64, StoreError> {
    if value > MAX_CURSOR {
        return Err(StoreError::Invalid);
    }
    Ok(value as i64)
}

fn unsigned_cursor(value: i64) -> Result<u64, StoreError> {
    u64::try_from(value).map_err(|_| StoreError::Unavailable)
}

async fn require_active_account(
    tx: &mut Transaction<'_, Postgres>,
    user: Uuid,
) -> Result<(), StoreError> {
    // SHARE (not UPDATE) allows independent memberships for the same account.
    let active: Option<Uuid> = sqlx::query_scalar(
        "SELECT user_id FROM accounts WHERE user_id=$1 AND disabled_at IS NULL FOR SHARE",
    )
    .bind(user)
    .fetch_optional(&mut **tx)
    .await?;
    active.ok_or(StoreError::Forbidden)?;
    Ok(())
}
async fn lock_group(tx: &mut Transaction<'_, Postgres>, group: Uuid) -> Result<(), StoreError> {
    let found: Option<Uuid> =
        sqlx::query_scalar("SELECT group_id FROM groups WHERE group_id=$1 FOR UPDATE")
            .bind(group)
            .fetch_optional(&mut **tx)
            .await?;
    found.ok_or(StoreError::NotFound)?;
    Ok(())
}
async fn role_in(
    tx: &mut Transaction<'_, Postgres>,
    group: Uuid,
    user: Uuid,
) -> Result<Option<Role>, StoreError> {
    let role: Option<String> =
        sqlx::query_scalar("SELECT role FROM group_memberships WHERE group_id=$1 AND user_id=$2")
            .bind(group)
            .bind(user)
            .fetch_optional(&mut **tx)
            .await?;
    role.as_deref().map(Role::parse).transpose()
}
async fn require_other_owner(
    tx: &mut Transaction<'_, Postgres>,
    group: Uuid,
) -> Result<(), StoreError> {
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM group_memberships WHERE group_id=$1 AND role='owner'",
    )
    .bind(group)
    .fetch_one(&mut **tx)
    .await?;
    if count < 2 {
        return Err(StoreError::Forbidden);
    }
    Ok(())
}
