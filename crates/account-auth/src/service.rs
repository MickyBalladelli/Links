use crate::{
    provider::{Channel, OtpProvider},
    AuthError,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use hmac::{Hmac, Mac};
use links_identity::{verify, DeviceBinding};
use links_protocol::v1;
use links_server_store::postgres::RelationalStore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Row};
use std::{
    net::IpAddr,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};
use uuid::Uuid;
use zeroize::{Zeroize, Zeroizing};

pub const CHALLENGE_TTL_MS: u64 = 10 * 60 * 1000;
pub const SESSION_TTL_MS: u64 = 15 * 60 * 1000;
pub trait Clock: Send + Sync {
    fn now_ms(&self) -> u64;
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
fn random() -> Result<[u8; 32], AuthError> {
    let mut bytes = [0; 32];
    getrandom::getrandom(&mut bytes).map_err(|_| AuthError::Unavailable)?;
    Ok(bytes)
}

pub struct AccountAuth {
    pool: PgPool,
    provider: Arc<dyn OtpProvider>,
    phone_lookup_key: Zeroizing<[u8; 32]>,
    clock: Arc<dyn Clock>,
}
impl AccountAuth {
    pub fn new(
        pool: PgPool,
        provider: Arc<dyn OtpProvider>,
        phone_lookup_key: Zeroizing<[u8; 32]>,
        clock: Arc<dyn Clock>,
    ) -> Result<Self, AuthError> {
        if *phone_lookup_key == [0; 32] {
            return Err(AuthError::Invalid);
        }
        Ok(Self {
            pool,
            provider,
            phone_lookup_key,
            clock,
        })
    }
    fn now(&self) -> Result<i64, AuthError> {
        let now = self.clock.now_ms();
        if now == 0 || now > i64::MAX as u64 - SESSION_TTL_MS {
            return Err(AuthError::Unavailable);
        }
        Ok(now as i64)
    }
    fn digest(&self, domain: &[u8], value: &[u8]) -> [u8; 32] {
        let mut mac = Hmac::<Sha256>::new_from_slice(self.phone_lookup_key.as_ref())
            .expect("fixed length HMAC key");
        mac.update(domain);
        mac.update(value);
        mac.finalize().into_bytes().into()
    }
    pub async fn start(
        &self,
        request: StartRequest,
        peer_ip: IpAddr,
    ) -> Result<Challenge, AuthError> {
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
        sqlx::query("INSERT INTO auth_challenges (challenge_id,subject_hash,user_id,device_id,mls_node_id,public_key,nonce,enrolling,permitted,state,expires_at_ms) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,'reserved',$10)")
            .bind(id).bind(subject.as_slice()).bind(user_id).bind(request.device_id).bind(request.mls_node_id).bind(public_key.as_slice()).bind(nonce.as_slice()).bind(enrolling).bind(permitted).bind(expires).execute(&mut *tx).await?;
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
        if row.get::<bool, _>("enrolling") {
            let inserted = sqlx::query("INSERT INTO accounts (user_id,auth_subject_hash) VALUES ($1,$2) ON CONFLICT DO NOTHING")
                .bind(binding.user_id).bind(&subject).execute(&mut *tx).await?;
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
        sqlx::query("DELETE FROM auth_rate_limits WHERE window_start_ms <= $1")
            .bind(now - 3_600_000)
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
