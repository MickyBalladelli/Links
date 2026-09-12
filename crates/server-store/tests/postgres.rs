//! Run only against a disposable development database. Every test owns a random
//! schema; no public tables are changed. CI runs these explicitly with --ignored.
use links_server_store::{
    postgres::{GroupKind, RelationalStore, Role},
    StoreError,
};
use sqlx::{postgres::PgPoolOptions, PgPool};
use uuid::Uuid;

struct Fixture {
    admin: PgPool,
    pool: PgPool,
    store: RelationalStore,
    schema: String,
}
impl Fixture {
    async fn new() -> Self {
        let url = std::env::var("LINKS_TEST_DATABASE_URL")
            .expect("set LINKS_TEST_DATABASE_URL to a disposable PostgreSQL database");
        let admin = PgPoolOptions::new()
            .max_connections(2)
            .connect(&url)
            .await
            .unwrap();
        let schema = format!("links_test_{}", Uuid::new_v4().simple());
        sqlx::query(&format!("CREATE SCHEMA {schema}"))
            .execute(&admin)
            .await
            .unwrap();
        let search_path = schema.clone();
        let pool = PgPoolOptions::new()
            .max_connections(5)
            .after_connect(move |conn, _| {
                let statement = format!("SET search_path TO {search_path}");
                Box::pin(async move {
                    sqlx::query(&statement).execute(conn).await?;
                    Ok(())
                })
            })
            .connect(&url)
            .await
            .unwrap();
        let store = RelationalStore::from_pool(pool.clone());
        store.migrate().await.unwrap();
        Self {
            admin,
            pool,
            store,
            schema,
        }
    }
    async fn account(&self, seed: u8) -> Uuid {
        let id = Uuid::new_v4();
        self.store.create_account(id, &[seed; 32]).await.unwrap();
        id
    }
    async fn finish(self) {
        self.store.close().await;
        sqlx::query(&format!("DROP SCHEMA {} CASCADE", self.schema))
            .execute(&self.admin)
            .await
            .unwrap();
        self.admin.close().await;
    }
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL via LINKS_TEST_DATABASE_URL"]
async fn migrations_are_repeatable_and_constraints_reject_bad_data() {
    let f = Fixture::new().await;
    f.store.migrate().await.unwrap();
    let tables: i64 = sqlx::query_scalar("SELECT count(*) FROM information_schema.tables WHERE table_schema=current_schema() AND table_name IN ('accounts','handles','devices','groups','group_memberships')")
        .fetch_one(&f.pool).await.unwrap();
    assert_eq!(tables, 5);
    assert!(matches!(
        f.store.create_account(Uuid::nil(), &[1; 32]).await,
        Err(StoreError::Invalid)
    ));
    assert!(matches!(
        f.store.create_account(Uuid::new_v4(), &[1; 31]).await,
        Err(StoreError::Invalid)
    ));
    let owner = f.account(1).await;
    assert!(matches!(
        f.store.create_account(Uuid::new_v4(), &[1; 32]).await,
        Err(StoreError::Conflict)
    ));
    let group = Uuid::new_v4();
    // Deferred owner constraint rolls back a transaction that never adds an owner.
    assert!(
        sqlx::query("INSERT INTO groups (group_id, group_kind) VALUES ($1,'group')")
            .bind(group)
            .execute(&f.pool)
            .await
            .is_err()
    );
    f.store
        .create_group(group, owner, GroupKind::Group)
        .await
        .unwrap();
    assert!(
        sqlx::query("DELETE FROM group_memberships WHERE group_id=$1")
            .bind(group)
            .execute(&f.pool)
            .await
            .is_err()
    );
    assert_eq!(f.store.role(group, owner).await.unwrap(), Some(Role::Owner));
    f.finish().await;
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL via LINKS_TEST_DATABASE_URL"]
async fn handle_claims_are_unique_under_race() {
    let f = Fixture::new().await;
    let a = f.account(1).await;
    let b = f.account(2).await;
    let (one, two) = tokio::join!(
        f.store.claim_handle(a, "shared_handle"),
        f.store.claim_handle(b, "shared_handle")
    );
    assert_ne!(one.is_ok(), two.is_ok());
    assert!(matches!(
        if one.is_err() { one } else { two },
        Err(StoreError::Conflict)
    ));
    let winner = f
        .store
        .resolve_handle("shared_handle")
        .await
        .unwrap()
        .unwrap();
    f.store.claim_handle(winner, "shared_handle").await.unwrap();
    assert!(f.store.claim_handle(a, "MixedCase").await.is_err());
    assert!(f.store.claim_handle(winner, "second_handle").await.is_err());
    assert!(f
        .store
        .resolve_handle("not_claimed")
        .await
        .unwrap()
        .is_none());
    f.finish().await;
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL via LINKS_TEST_DATABASE_URL"]
async fn device_nodes_enforce_ownership_uniqueness_and_revocation() {
    let f = Fixture::new().await;
    let owner = f.account(1).await;
    let other = f.account(2).await;
    let device = Uuid::new_v4();
    let node = Uuid::new_v4();
    f.store
        .register_device(owner, device, node, &[1; 32], &[2; 64])
        .await
        .unwrap();
    assert_eq!(f.store.active_devices(owner).await.unwrap(), vec![device]);
    assert!(matches!(
        f.store
            .register_device(other, Uuid::new_v4(), node, &[1; 32], &[2])
            .await,
        Err(StoreError::Conflict)
    ));
    assert!(f
        .store
        .register_device(owner, Uuid::new_v4(), Uuid::new_v4(), &[1; 31], &[2])
        .await
        .is_err());
    assert!(matches!(
        f.store.revoke_device(other, device).await,
        Err(StoreError::NotFound)
    ));
    f.store.revoke_device(owner, device).await.unwrap();
    f.store.revoke_device(owner, device).await.unwrap();
    assert!(f.store.active_devices(owner).await.unwrap().is_empty());
    assert!(matches!(
        f.store
            .register_device(owner, device, Uuid::new_v4(), &[1; 32], &[2])
            .await,
        Err(StoreError::Conflict)
    ));
    f.finish().await;
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL via LINKS_TEST_DATABASE_URL"]
async fn rbac_blocks_escalation_and_last_owner_removal() {
    let f = Fixture::new().await;
    let owner = f.account(1).await;
    let admin = f.account(2).await;
    let member = f.account(3).await;
    let outsider = f.account(4).await;
    let group = Uuid::new_v4();
    f.store
        .create_group(group, owner, GroupKind::Group)
        .await
        .unwrap();
    f.store
        .set_role(group, owner, admin, Role::Admin)
        .await
        .unwrap();
    f.store
        .set_role(group, admin, member, Role::Member)
        .await
        .unwrap();
    for (actor, target, role) in [
        (admin, admin, Role::Owner),
        (member, member, Role::Admin),
        (outsider, outsider, Role::Owner),
        (admin, owner, Role::Member),
    ] {
        assert!(matches!(
            f.store.set_role(group, actor, target, role).await,
            Err(StoreError::Forbidden)
        ));
    }
    assert!(matches!(
        f.store.remove_member(group, owner, owner).await,
        Err(StoreError::Forbidden)
    ));
    assert!(matches!(
        f.store.set_role(group, owner, owner, Role::Member).await,
        Err(StoreError::Forbidden)
    ));
    assert!(matches!(
        f.store.remove_member(group, admin, owner).await,
        Err(StoreError::Forbidden)
    ));
    f.store.remove_member(group, member, member).await.unwrap();
    assert_eq!(f.store.role(group, member).await.unwrap(), None);
    f.store
        .set_role(group, owner, admin, Role::Owner)
        .await
        .unwrap();
    f.store.remove_member(group, owner, owner).await.unwrap();
    assert_eq!(f.store.role(group, admin).await.unwrap(), Some(Role::Owner));
    f.finish().await;
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL via LINKS_TEST_DATABASE_URL"]
async fn concurrent_owner_departures_preserve_one_owner() {
    let f = Fixture::new().await;
    let a = f.account(1).await;
    let b = f.account(2).await;
    let group = Uuid::new_v4();
    f.store
        .create_group(group, a, GroupKind::Group)
        .await
        .unwrap();
    f.store.set_role(group, a, b, Role::Owner).await.unwrap();
    let (one, two) = tokio::join!(
        f.store.remove_member(group, a, a),
        f.store.remove_member(group, b, b)
    );
    assert_ne!(one.is_ok(), two.is_ok());
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM group_memberships WHERE group_id=$1 AND role='owner'",
    )
    .bind(group)
    .fetch_one(&f.pool)
    .await
    .unwrap();
    assert_eq!(count, 1);
    f.finish().await;
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL via LINKS_TEST_DATABASE_URL"]
async fn disabled_accounts_cannot_claim_register_or_change_roles() {
    let f = Fixture::new().await;
    let owner = f.account(1).await;
    let target = f.account(2).await;
    let group = Uuid::new_v4();
    f.store
        .create_group(group, owner, GroupKind::Group)
        .await
        .unwrap();
    f.store.claim_handle(owner, "disabled_owner").await.unwrap();
    sqlx::query("UPDATE accounts SET disabled_at=now() WHERE user_id=$1")
        .bind(owner)
        .execute(&f.pool)
        .await
        .unwrap();
    assert!(matches!(
        f.store.claim_handle(owner, "disabled_owner").await,
        Err(StoreError::Forbidden)
    ));
    assert!(f
        .store
        .resolve_handle("disabled_owner")
        .await
        .unwrap()
        .is_none());
    assert!(matches!(
        f.store
            .register_device(owner, Uuid::new_v4(), Uuid::new_v4(), &[1; 32], &[2])
            .await,
        Err(StoreError::Forbidden)
    ));
    assert!(matches!(
        f.store.set_role(group, owner, target, Role::Member).await,
        Err(StoreError::Forbidden)
    ));
    f.finish().await;
}
