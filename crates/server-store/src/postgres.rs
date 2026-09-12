use crate::StoreError;
use links_protocol::validate_handle;
use sqlx::{postgres::PgPoolOptions, PgPool, Postgres, Transaction};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role { Owner, Admin, Member }
impl Role {
    fn as_str(self) -> &'static str { match self { Self::Owner => "owner", Self::Admin => "admin", Self::Member => "member" } }
    fn parse(value: &str) -> Result<Self, StoreError> { match value { "owner" => Ok(Self::Owner), "admin" => Ok(Self::Admin), "member" => Ok(Self::Member), _ => Err(StoreError::Invalid) } }
}
#[derive(Debug, Clone, Copy)]
pub enum GroupKind { Direct, Group, Channel }
impl GroupKind { fn as_str(self) -> &'static str { match self { Self::Direct => "direct", Self::Group => "group", Self::Channel => "channel" } } }

#[derive(Clone)]
pub struct RelationalStore { pool: PgPool }
impl RelationalStore {
    pub async fn connect(url: &str) -> Result<Self, StoreError> {
        Ok(Self { pool: PgPoolOptions::new().max_connections(10).connect(url).await? })
    }
    pub fn from_pool(pool: PgPool) -> Self { Self { pool } }
    pub async fn migrate(&self) -> Result<(), StoreError> {
        sqlx::migrate!("./migrations").run(&self.pool).await.map_err(StoreError::Migration)
    }
    pub async fn close(&self) { self.pool.close().await; }

    /// Input is a keyed, server-secret lookup digest from the auth service.
    /// Never pass a plain phone number or an unsalted phone hash here.
    pub async fn create_account(&self, user_id: Uuid, auth_subject_hash: &[u8]) -> Result<(), StoreError> {
        sqlx::query("INSERT INTO accounts (user_id, auth_subject_hash) VALUES ($1, $2)")
            .bind(user_id).bind(auth_subject_hash).execute(&self.pool).await?;
        Ok(())
    }
    pub async fn claim_handle(&self, user_id: Uuid, handle: &str) -> Result<(), StoreError> {
        validate_handle(handle)?;
        let mut tx = self.pool.begin().await?;
        require_active_account(&mut tx, user_id).await?;
        let current: Option<Uuid> = sqlx::query_scalar("SELECT user_id FROM handles WHERE handle = $1")
            .bind(handle).fetch_optional(&mut *tx).await?;
        if current == Some(user_id) { tx.commit().await?; return Ok(()); }
        sqlx::query("INSERT INTO handles (handle, user_id) VALUES ($1, $2)").bind(handle).bind(user_id).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(())
    }
    pub async fn resolve_handle(&self, handle: &str) -> Result<Option<Uuid>, StoreError> {
        validate_handle(handle)?;
        Ok(sqlx::query_scalar("SELECT h.user_id FROM handles h JOIN accounts a USING (user_id) WHERE handle = $1 AND a.disabled_at IS NULL")
            .bind(handle).fetch_optional(&self.pool).await?)
    }
    pub async fn register_device(&self, user_id: Uuid, device_id: Uuid, mls_node_id: Uuid, public_key: &[u8], credential: &[u8]) -> Result<(), StoreError> {
        let mut tx = self.pool.begin().await?;
        require_active_account(&mut tx, user_id).await?;
        sqlx::query("INSERT INTO devices (device_id, user_id, mls_node_id, identity_public_key, mls_credential) VALUES ($1,$2,$3,$4,$5)")
            .bind(device_id).bind(user_id).bind(mls_node_id).bind(public_key).bind(credential).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(())
    }
    pub async fn active_devices(&self, user_id: Uuid) -> Result<Vec<Uuid>, StoreError> {
        Ok(sqlx::query_scalar("SELECT d.device_id FROM devices d JOIN accounts a USING (user_id) WHERE d.user_id = $1 AND d.revoked_at IS NULL AND a.disabled_at IS NULL ORDER BY d.device_id")
            .bind(user_id).fetch_all(&self.pool).await?)
    }
    pub async fn revoke_device(&self, actor_user_id: Uuid, device_id: Uuid) -> Result<(), StoreError> {
        let result = sqlx::query("UPDATE devices SET revoked_at = COALESCE(revoked_at, now()) WHERE device_id = $1 AND user_id = $2")
            .bind(device_id).bind(actor_user_id).execute(&self.pool).await?;
        if result.rows_affected() == 0 { return Err(StoreError::NotFound); }
        Ok(())
    }
    pub async fn create_group(&self, group_id: Uuid, owner: Uuid, kind: GroupKind) -> Result<(), StoreError> {
        let mut tx = self.pool.begin().await?;
        require_active_account(&mut tx, owner).await?;
        sqlx::query("INSERT INTO groups (group_id, group_kind) VALUES ($1,$2)").bind(group_id).bind(kind.as_str()).execute(&mut *tx).await?;
        sqlx::query("INSERT INTO group_memberships (group_id, user_id, role) VALUES ($1,$2,'owner')").bind(group_id).bind(owner).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(())
    }
    pub async fn role(&self, group_id: Uuid, user_id: Uuid) -> Result<Option<Role>, StoreError> {
        let value: Option<String> = sqlx::query_scalar("SELECT role FROM group_memberships WHERE group_id = $1 AND user_id = $2")
            .bind(group_id).bind(user_id).fetch_optional(&self.pool).await?;
        value.as_deref().map(Role::parse).transpose()
    }

    /// Owner: all roles. Admin: add/update members only. Member: no grants.
    /// API authentication must supply actor; never trust an actor ID from a body.
    pub async fn set_role(&self, group_id: Uuid, actor: Uuid, target: Uuid, role: Role) -> Result<(), StoreError> {
        let mut tx = self.pool.begin().await?;
        lock_group(&mut tx, group_id).await?;
        require_active_account(&mut tx, actor).await?;
        require_active_account(&mut tx, target).await?;
        let actor_role = role_in(&mut tx, group_id, actor).await?;
        let current = role_in(&mut tx, group_id, target).await?;
        match actor_role {
            Some(Role::Owner) => {},
            Some(Role::Admin) if role == Role::Member && matches!(current, None | Some(Role::Member)) => {},
            _ => return Err(StoreError::Forbidden),
        }
        if current == Some(Role::Owner) && role != Role::Owner { require_other_owner(&mut tx, group_id).await?; }
        sqlx::query("INSERT INTO group_memberships (group_id,user_id,role) VALUES ($1,$2,$3) ON CONFLICT (group_id,user_id) DO UPDATE SET role = EXCLUDED.role")
            .bind(group_id).bind(target).bind(role.as_str()).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(())
    }
    pub async fn remove_member(&self, group_id: Uuid, actor: Uuid, target: Uuid) -> Result<(), StoreError> {
        let mut tx = self.pool.begin().await?;
        lock_group(&mut tx, group_id).await?;
        require_active_account(&mut tx, actor).await?;
        let actor_role = role_in(&mut tx, group_id, actor).await?;
        let current = role_in(&mut tx, group_id, target).await?.ok_or(StoreError::NotFound)?;
        if actor != target && actor_role != Some(Role::Owner) && !(actor_role == Some(Role::Admin) && current == Role::Member) { return Err(StoreError::Forbidden); }
        if current == Role::Owner { require_other_owner(&mut tx, group_id).await?; }
        sqlx::query("DELETE FROM group_memberships WHERE group_id=$1 AND user_id=$2").bind(group_id).bind(target).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(())
    }
}

async fn require_active_account(tx: &mut Transaction<'_, Postgres>, user: Uuid) -> Result<(), StoreError> {
    // SHARE (not UPDATE) allows independent memberships for the same account.
    let active: Option<Uuid> = sqlx::query_scalar("SELECT user_id FROM accounts WHERE user_id=$1 AND disabled_at IS NULL FOR SHARE")
        .bind(user).fetch_optional(&mut **tx).await?;
    active.ok_or(StoreError::Forbidden)?;
    Ok(())
}
async fn lock_group(tx: &mut Transaction<'_, Postgres>, group: Uuid) -> Result<(), StoreError> {
    let found: Option<Uuid> = sqlx::query_scalar("SELECT group_id FROM groups WHERE group_id=$1 FOR UPDATE").bind(group).fetch_optional(&mut **tx).await?;
    found.ok_or(StoreError::NotFound)?;
    Ok(())
}
async fn role_in(tx: &mut Transaction<'_, Postgres>, group: Uuid, user: Uuid) -> Result<Option<Role>, StoreError> {
    let role: Option<String> = sqlx::query_scalar("SELECT role FROM group_memberships WHERE group_id=$1 AND user_id=$2")
        .bind(group).bind(user).fetch_optional(&mut **tx).await?;
    role.as_deref().map(Role::parse).transpose()
}
async fn require_other_owner(tx: &mut Transaction<'_, Postgres>, group: Uuid) -> Result<(), StoreError> {
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM group_memberships WHERE group_id=$1 AND role='owner'").bind(group).fetch_one(&mut **tx).await?;
    if count < 2 { return Err(StoreError::Forbidden); }
    Ok(())
}
