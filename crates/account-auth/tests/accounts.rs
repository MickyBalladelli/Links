use async_trait::async_trait;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use links_account_auth::{
    provider::{Channel, OtpProvider},
    service::{
        encode, AccountAuth, Challenge, Clock, FinishRequest, StartRequest,
        UsernameAuthPurpose, UsernameChallengeRequest, UsernameLoginRequest,
        UsernameRegistrationRequest, CHALLENGE_TTL_MS,
    },
    AuthError,
};
use links_identity::{DeviceBinding, IdentitySeed};
use links_server_store::postgres::RelationalStore;
use sqlx::{postgres::PgPoolOptions, PgPool};
use std::sync::{
    atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
    Arc,
};
use uuid::Uuid;
use zeroize::Zeroizing;

struct TestClock(AtomicU64);
impl Clock for TestClock {
    fn now_ms(&self) -> u64 {
        self.0.load(Ordering::SeqCst)
    }
}
struct Provider {
    starts: AtomicUsize,
    checks: AtomicUsize,
    fail: AtomicBool,
    advance_on_check: AtomicBool,
    reuse_sid: AtomicBool,
    clock: Arc<TestClock>,
}
#[async_trait]
impl OtpProvider for Provider {
    async fn start(&self, _: &str, _: Channel) -> Result<String, AuthError> {
        self.starts.fetch_add(1, Ordering::SeqCst);
        if self.fail.load(Ordering::SeqCst) {
            return Err(AuthError::Unavailable);
        }
        if self.reuse_sid.load(Ordering::SeqCst) {
            return Ok(format!("VE{}", "f".repeat(32)));
        }
        Ok(format!("VE{}", Uuid::new_v4().simple()))
    }
    async fn check(&self, _: &str, code: &str) -> Result<bool, AuthError> {
        self.checks.fetch_add(1, Ordering::SeqCst);
        if self.fail.load(Ordering::SeqCst) {
            return Err(AuthError::Unavailable);
        }
        if self.advance_on_check.load(Ordering::SeqCst) {
            self.clock.0.fetch_add(CHALLENGE_TTL_MS, Ordering::SeqCst);
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        Ok(code == "123456")
    }
}
struct Fixture {
    admin: PgPool,
    pool: PgPool,
    auth: Arc<AccountAuth>,
    provider: Arc<Provider>,
    clock: Arc<TestClock>,
    schema: String,
}
impl Fixture {
    async fn new() -> Self {
        let url = std::env::var("LINKS_TEST_DATABASE_URL")
            .expect("set LINKS_TEST_DATABASE_URL to disposable PostgreSQL");
        let admin = PgPoolOptions::new()
            .max_connections(2)
            .connect(&url)
            .await
            .unwrap();
        let schema = format!("links_auth_test_{}", Uuid::new_v4().simple());
        sqlx::query(&format!("CREATE SCHEMA {schema}"))
            .execute(&admin)
            .await
            .unwrap();
        let search_path = schema.clone();
        let pool = PgPoolOptions::new()
            .max_connections(5)
            .after_connect(move |conn, _| {
                let sql = format!("SET search_path TO {search_path}");
                Box::pin(async move {
                    sqlx::query(&sql).execute(conn).await?;
                    Ok(())
                })
            })
            .connect(&url)
            .await
            .unwrap();
        RelationalStore::from_pool(pool.clone())
            .migrate()
            .await
            .unwrap();
        let clock = Arc::new(TestClock(AtomicU64::new(1_000_000)));
        let provider = Arc::new(Provider {
            starts: AtomicUsize::new(0),
            checks: AtomicUsize::new(0),
            fail: AtomicBool::new(false),
            advance_on_check: AtomicBool::new(false),
            reuse_sid: AtomicBool::new(false),
            clock: clock.clone(),
        });
        let auth = Arc::new(
            AccountAuth::new(
                pool.clone(),
                provider.clone(),
                Zeroizing::new([7; 32]),
                clock.clone(),
            )
            .unwrap(),
        );
        Self {
            admin,
            pool,
            auth,
            provider,
            clock,
            schema,
        }
    }
    async fn start(&self, client: &Client) -> Challenge {
        self.auth
            .start(client.start(), "127.0.0.1".parse().unwrap())
            .await
            .unwrap()
    }
    async fn count(&self, table: &str) -> i64 {
        sqlx::query_scalar(&format!("SELECT count(*) FROM {table}"))
            .fetch_one(&self.pool)
            .await
            .unwrap()
    }
    async fn finish(self) {
        self.pool.close().await;
        sqlx::query(&format!("DROP SCHEMA {} CASCADE", self.schema))
            .execute(&self.admin)
            .await
            .unwrap();
        self.admin.close().await;
    }
}
struct Client {
    seed: IdentitySeed,
    device: Uuid,
    node: Uuid,
}
impl Client {
    fn new() -> Self {
        Self {
            seed: IdentitySeed::generate().unwrap(),
            device: Uuid::new_v4(),
            node: Uuid::new_v4(),
        }
    }
    fn start(&self) -> StartRequest {
        let public_key = self.seed.public_key();
        let transcript = links_identity::phone_auth_transcript(
            "+12025550123",
            "sms",
            self.device,
            self.node,
            &public_key,
        )
        .unwrap();
        StartRequest {
            phone: "+12025550123".into(),
            channel: Channel::Sms,
            device_id: self.device,
            mls_node_id: self.node,
            public_key: encode(&public_key),
            signature: encode(&self.seed.sign(&transcript)),
        }
    }
    fn finish(&self, challenge: &Challenge, code: &str) -> FinishRequest {
        let binding = DeviceBinding {
            user_id: challenge.user_id,
            device_id: challenge.device_id,
            mls_node_id: challenge.mls_node_id,
            public_key: self.seed.public_key(),
        };
        let nonce: [u8; 32] = URL_SAFE_NO_PAD
            .decode(&challenge.nonce)
            .unwrap()
            .try_into()
            .unwrap();
        let transcript = binding
            .enrollment_transcript(challenge.challenge_id, &nonce, challenge.expires_at_ms)
            .unwrap();
        assert_eq!(
            encode(&binding.mls_credential().unwrap()),
            challenge.mls_credential
        );
        FinishRequest {
            challenge_id: challenge.challenge_id,
            code: code.into(),
            signature: encode(&self.seed.sign(&transcript)),
        }
    }
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL via LINKS_TEST_DATABASE_URL"]
async fn enroll_login_restart_and_revoke() {
    let f = Fixture::new().await;
    let client = Client::new();
    let challenge = f.start(&client).await;
    let session = f
        .auth
        .finish(client.finish(&challenge, "123456"))
        .await
        .unwrap();
    assert_eq!(
        f.auth
            .authenticate(&session.access_token)
            .await
            .unwrap()
            .device_id,
        client.device
    );
    assert_eq!(f.count("accounts").await, 1);
    assert_eq!(f.count("devices").await, 1);
    assert!(f
        .auth
        .finish(client.finish(&challenge, "123456"))
        .await
        .is_err());
    assert_eq!(f.provider.checks.load(Ordering::SeqCst), 1);
    let restarted = AccountAuth::new(
        f.pool.clone(),
        f.provider.clone(),
        Zeroizing::new([7; 32]),
        f.clock.clone(),
    )
    .unwrap();
    assert!(restarted.authenticate(&session.access_token).await.is_ok());
    assert!(restarted
        .finish(client.finish(&challenge, "123456"))
        .await
        .is_err());
    f.clock.0.fetch_add(60_000, Ordering::SeqCst);
    let login = f.start(&client).await;
    assert_eq!(login.user_id, session.user_id);
    let second = f
        .auth
        .finish(client.finish(&login, "123456"))
        .await
        .unwrap();
    assert_eq!(f.count("accounts").await, 1);
    RelationalStore::from_pool(f.pool.clone())
        .revoke_device(session.user_id, client.device)
        .await
        .unwrap();
    assert!(f.auth.authenticate(&session.access_token).await.is_err());
    assert!(f.auth.authenticate(&second.access_token).await.is_err());
    f.finish().await;
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL via LINKS_TEST_DATABASE_URL"]
async fn rate_limits_proof_and_five_attempt_lockout() {
    let f = Fixture::new().await;
    let client = Client::new();
    let mut invalid = client.start();
    invalid.signature = encode(&[0; 64]);
    assert!(f
        .auth
        .start(invalid, "127.0.0.1".parse().unwrap())
        .await
        .is_err());
    assert_eq!(f.provider.starts.load(Ordering::SeqCst), 0);
    let challenge = f.start(&client).await;
    assert!(matches!(
        f.auth
            .start(client.start(), "127.0.0.1".parse().unwrap())
            .await,
        Err(AuthError::RateLimited)
    ));
    let mut forged = client.finish(&challenge, "123456");
    forged.signature = encode(&[0; 64]);
    assert!(f.auth.finish(forged).await.is_err());
    assert_eq!(f.provider.checks.load(Ordering::SeqCst), 0);
    for _ in 0..5 {
        assert!(f
            .auth
            .finish(client.finish(&challenge, "000000"))
            .await
            .is_err());
    }
    assert!(f
        .auth
        .finish(client.finish(&challenge, "123456"))
        .await
        .is_err());
    assert_eq!(f.provider.checks.load(Ordering::SeqCst), 5);
    for _ in 1..5 {
        f.clock.0.fetch_add(60_000, Ordering::SeqCst);
        f.start(&client).await;
    }
    f.clock.0.fetch_add(60_000, Ordering::SeqCst);
    assert!(matches!(
        f.auth
            .start(client.start(), "127.0.0.1".parse().unwrap())
            .await,
        Err(AuthError::RateLimited)
    ));
    assert_eq!(f.provider.starts.load(Ordering::SeqCst), 5);
    assert_eq!(f.count("accounts").await, 0);
    f.finish().await;
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL via LINKS_TEST_DATABASE_URL"]
async fn concurrent_approval_issues_exactly_one_session() {
    let f = Fixture::new().await;
    let client = Client::new();
    let challenge = f.start(&client).await;
    let (one, two) = tokio::join!(
        f.auth.finish(client.finish(&challenge, "123456")),
        f.auth.finish(client.finish(&challenge, "123456"))
    );
    assert_ne!(one.is_ok(), two.is_ok());
    assert_eq!(f.count("auth_sessions").await, 1);
    assert_eq!(f.provider.checks.load(Ordering::SeqCst), 1);
    f.finish().await;
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL via LINKS_TEST_DATABASE_URL"]
async fn provider_approval_cannot_be_reused_across_challenges() {
    let f = Fixture::new().await;
    f.provider.reuse_sid.store(true, Ordering::SeqCst);
    let client = Client::new();
    let first = f.start(&client).await;
    let session = f
        .auth
        .finish(client.finish(&first, "123456"))
        .await
        .unwrap();
    f.clock.0.fetch_add(60_000, Ordering::SeqCst);
    let second = f.start(&client).await;
    assert!(f
        .auth
        .finish(client.finish(&second, "123456"))
        .await
        .is_err());
    assert_eq!(f.count("auth_sessions").await, 1);
    assert!(f.auth.authenticate(&session.access_token).await.is_ok());
    f.finish().await;
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL via LINKS_TEST_DATABASE_URL"]
async fn otp_does_not_allow_replacing_existing_device_identity() {
    let f = Fixture::new().await;
    let original = Client::new();
    let challenge = f.start(&original).await;
    let session = f
        .auth
        .finish(original.finish(&challenge, "123456"))
        .await
        .unwrap();
    f.clock.0.fetch_add(60_000, Ordering::SeqCst);
    let replacement = Client::new();
    let denied = f.start(&replacement).await;
    assert_ne!(denied.user_id, session.user_id);
    assert!(f
        .auth
        .finish(replacement.finish(&denied, "123456"))
        .await
        .is_err());
    assert_eq!(f.count("accounts").await, 1);
    assert_eq!(f.count("devices").await, 1);
    assert!(f.auth.authenticate(&session.access_token).await.is_ok());
    f.finish().await;
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL via LINKS_TEST_DATABASE_URL"]
async fn expiry_provider_outage_and_superseded_challenges_fail_closed() {
    let f = Fixture::new().await;
    let client = Client::new();
    let first = f.start(&client).await;
    f.clock.0.fetch_add(60_000, Ordering::SeqCst);
    let second = f.start(&client).await;
    assert!(f
        .auth
        .finish(client.finish(&first, "123456"))
        .await
        .is_err());
    assert_eq!(f.provider.checks.load(Ordering::SeqCst), 0);
    f.provider.advance_on_check.store(true, Ordering::SeqCst);
    assert!(f
        .auth
        .finish(client.finish(&second, "123456"))
        .await
        .is_err());
    assert_eq!(f.count("accounts").await, 0);
    f.provider.advance_on_check.store(false, Ordering::SeqCst);
    let third = f.start(&client).await;
    f.provider.fail.store(true, Ordering::SeqCst);
    assert!(matches!(
        f.auth.finish(client.finish(&third, "123456")).await,
        Err(AuthError::Unavailable)
    ));
    f.provider.fail.store(false, Ordering::SeqCst);
    assert!(f
        .auth
        .finish(client.finish(&third, "123456"))
        .await
        .is_err());
    f.finish().await;
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL via LINKS_TEST_DATABASE_URL"]
async fn sessions_expire_and_disabled_accounts_are_rechecked() {
    let f = Fixture::new().await;
    let client = Client::new();
    let challenge = f.start(&client).await;
    let session = f
        .auth
        .finish(client.finish(&challenge, "123456"))
        .await
        .unwrap();
    sqlx::query("UPDATE accounts SET disabled_at=now() WHERE user_id=$1")
        .bind(session.user_id)
        .execute(&f.pool)
        .await
        .unwrap();
    assert!(f.auth.authenticate(&session.access_token).await.is_err());
    sqlx::query("UPDATE accounts SET disabled_at=NULL WHERE user_id=$1")
        .bind(session.user_id)
        .execute(&f.pool)
        .await
        .unwrap();
    assert!(f.auth.authenticate(&session.access_token).await.is_ok());
    f.clock.0.store(session.expires_at_ms, Ordering::SeqCst);
    assert!(f.auth.authenticate(&session.access_token).await.is_err());
    f.auth.purge_expired().await.unwrap();
    assert_eq!(f.count("auth_sessions").await, 0);
    f.finish().await;
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL via LINKS_TEST_DATABASE_URL"]
async fn username_challenges_are_one_time_and_logout_revokes_session() {
    let f = Fixture::new().await;
    let seed = IdentitySeed::generate().unwrap();
    let device_id = Uuid::new_v4();
    let mls_node_id = Uuid::new_v4();
    let public_key = seed.public_key();
    let peer = "127.0.0.1".parse().unwrap();

    let registration = f
        .auth
        .start_username_challenge(
            UsernameChallengeRequest {
                handle: "alice_test".into(),
                purpose: UsernameAuthPurpose::Registration,
                device_id,
                mls_node_id,
                public_key: encode(&public_key),
            },
            peer,
        )
        .await
        .unwrap();
    let registration_challenge: [u8; 32] = URL_SAFE_NO_PAD
        .decode(&registration.challenge)
        .unwrap()
        .try_into()
        .unwrap();
    let registration_signature = encode(&seed.sign(
        &links_identity::username_registration_transcript(
            registration.challenge_id,
            &registration.handle,
            device_id,
            mls_node_id,
            &public_key,
            &registration_challenge,
            registration.expires_at_ms,
        )
        .unwrap(),
    ));
    let first = f
        .auth
        .register_username(
            UsernameRegistrationRequest {
                challenge_id: registration.challenge_id,
                signature: registration_signature.clone(),
            },
            peer,
        )
        .await
        .unwrap();
    assert!(f
        .auth
        .register_username(
            UsernameRegistrationRequest {
                challenge_id: registration.challenge_id,
                signature: registration_signature,
            },
            peer,
        )
        .await
        .is_err());
    assert_eq!(f.count("auth_sessions").await, 1);
    let reverse_directory = f
        .auth
        .lookup_username_directory_by_user_id(
            &first.session.access_token,
            first.session.user_id,
            peer,
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(reverse_directory.handle, "alice_test");
    assert_eq!(reverse_directory.user_id, first.session.user_id);

    let attacker = IdentitySeed::generate().unwrap();
    for _ in 0..19 {
        f.auth
            .start_username_challenge(
                UsernameChallengeRequest {
                    handle: "alice_test".into(),
                    purpose: UsernameAuthPurpose::Registration,
                    device_id: Uuid::new_v4(),
                    mls_node_id: Uuid::new_v4(),
                    public_key: encode(&attacker.public_key()),
                },
                peer,
            )
            .await
            .unwrap();
    }
    assert!(f
        .auth
        .start_username_challenge(
            UsernameChallengeRequest {
                handle: "alice_test".into(),
                purpose: UsernameAuthPurpose::Registration,
                device_id: Uuid::new_v4(),
                mls_node_id: Uuid::new_v4(),
                public_key: encode(&attacker.public_key()),
            },
            peer,
        )
        .await
        .is_err());

    let login = f
        .auth
        .start_username_challenge(
            UsernameChallengeRequest {
                handle: "alice_test".into(),
                purpose: UsernameAuthPurpose::Login,
                device_id,
                mls_node_id,
                public_key: encode(&public_key),
            },
            peer,
        )
        .await
        .unwrap();
    let _parallel_login = f
        .auth
        .start_username_challenge(
            UsernameChallengeRequest {
                handle: "alice_test".into(),
                purpose: UsernameAuthPurpose::Login,
                device_id,
                mls_node_id,
                public_key: encode(&public_key),
            },
            peer,
        )
        .await
        .unwrap();
    let login_challenge: [u8; 32] = URL_SAFE_NO_PAD
        .decode(&login.challenge)
        .unwrap()
        .try_into()
        .unwrap();
    let login_signature = encode(&seed.sign(
        &links_identity::username_login_transcript(
            login.challenge_id,
            &login.handle,
            device_id,
            mls_node_id,
            &public_key,
            &login_challenge,
            login.expires_at_ms,
        )
        .unwrap(),
    ));
    assert!(f
        .auth
        .register_username(
            UsernameRegistrationRequest {
                challenge_id: login.challenge_id,
                signature: login_signature.clone(),
            },
            peer,
        )
        .await
        .is_err());

    let auth_one = f.auth.clone();
    let auth_two = f.auth.clone();
    let first_finish = auth_one.login_username(
        UsernameLoginRequest {
            challenge_id: login.challenge_id,
            signature: login_signature.clone(),
        },
        peer,
    );
    let second_finish = auth_two.login_username(
        UsernameLoginRequest {
            challenge_id: login.challenge_id,
            signature: login_signature,
        },
        peer,
    );
    let (first_result, second_result) = tokio::join!(first_finish, second_finish);
    assert_eq!(first_result.is_ok() as u8 + second_result.is_ok() as u8, 1);
    let second = first_result.or(second_result).unwrap();
    assert_eq!(f.count("auth_sessions").await, 2);

    let expiring = f
        .auth
        .start_username_challenge(
            UsernameChallengeRequest {
                handle: "alice_test".into(),
                purpose: UsernameAuthPurpose::Login,
                device_id,
                mls_node_id,
                public_key: encode(&public_key),
            },
            peer,
        )
        .await
        .unwrap();
    let expiring_bytes: [u8; 32] = URL_SAFE_NO_PAD
        .decode(&expiring.challenge)
        .unwrap()
        .try_into()
        .unwrap();
    let expiring_signature = encode(
        &seed.sign(
            &links_identity::username_login_transcript(
                expiring.challenge_id,
                &expiring.handle,
                device_id,
                mls_node_id,
                &public_key,
                &expiring_bytes,
                expiring.expires_at_ms,
            )
            .unwrap(),
        ),
    );
    f.clock.advance(CHALLENGE_TTL_MS);
    assert!(f
        .auth
        .login_username(
            UsernameLoginRequest {
                challenge_id: expiring.challenge_id,
                signature: expiring_signature,
            },
            peer,
        )
        .await
        .is_err());

    let failed = f
        .auth
        .start_username_challenge(
            UsernameChallengeRequest {
                handle: "alice_test".into(),
                purpose: UsernameAuthPurpose::Login,
                device_id,
                mls_node_id,
                public_key: encode(&public_key),
            },
            peer,
        )
        .await
        .unwrap();
    let failed_bytes: [u8; 32] = URL_SAFE_NO_PAD
        .decode(&failed.challenge)
        .unwrap()
        .try_into()
        .unwrap();
    let valid_after_failure = encode(
        &seed.sign(
            &links_identity::username_login_transcript(
                failed.challenge_id,
                &failed.handle,
                device_id,
                mls_node_id,
                &public_key,
                &failed_bytes,
                failed.expires_at_ms,
            )
            .unwrap(),
        ),
    );
    assert!(f
        .auth
        .login_username(
            UsernameLoginRequest {
                challenge_id: failed.challenge_id,
                signature: encode(&[0_u8; 64]),
            },
            peer,
        )
        .await
        .is_err());
    assert!(f
        .auth
        .login_username(
            UsernameLoginRequest {
                challenge_id: failed.challenge_id,
                signature: valid_after_failure,
            },
            peer,
        )
        .await
        .is_err());

    f.auth
        .revoke_other_sessions(&second.session.access_token)
        .await
        .unwrap();
    assert!(f.auth.authenticate(&first.session.access_token).await.is_err());
    assert!(f.auth.authenticate(&second.session.access_token).await.is_ok());
    f.auth.logout(&second.session.access_token).await.unwrap();
    f.auth.logout(&second.session.access_token).await.unwrap();
    assert!(f
        .auth
        .authenticate(&second.session.access_token)
        .await
        .is_err());
    f.finish().await;
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL via LINKS_TEST_DATABASE_URL"]
async fn http_flow_has_no_store_and_rejects_malformed_payloads() {
    use axum::{
        body::Body,
        extract::ConnectInfo,
        http::{Request, StatusCode},
    };
    use http_body_util::BodyExt;
    use tower::ServiceExt;
    let f = Fixture::new().await;
    let client = Client::new();
    let app = links_account_auth::web::router(f.auth.clone());
    let start = client.start();
    let json = serde_json::json!({"phone":start.phone,"channel":"sms","device_id":start.device_id,"mls_node_id":start.mls_node_id,"public_key":start.public_key,"signature":start.signature});
    let mut request = Request::post("/v1/auth/start")
        .header("content-type", "application/json")
        .body(Body::from(json.to_string()))
        .unwrap();
    request.extensions_mut().insert(ConnectInfo(
        "127.0.0.1:12345".parse::<std::net::SocketAddr>().unwrap(),
    ));
    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["cache-control"], "no-store");
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let challenge: Challenge = serde_json::from_slice(&bytes).unwrap();
    let finish = client.finish(&challenge, "123456");
    let json = serde_json::json!({"challenge_id":finish.challenge_id,"code":finish.code,"signature":finish.signature});
    let response = app
        .clone()
        .oneshot(
            Request::post("/v1/auth/finish")
                .header("content-type", "application/json")
                .body(Body::from(json.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let session: links_account_auth::service::Session = serde_json::from_slice(&bytes).unwrap();
    let response = app
        .clone()
        .oneshot(
            Request::get("/v1/auth/me")
                .header("authorization", format!("Bearer {}", session.access_token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let response = app
        .clone()
        .oneshot(
            Request::post("/v1/auth/finish")
                .header("content-type", "application/json")
                .body(Body::from("{\"code\":\"private\"}"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    assert!(!String::from_utf8_lossy(&bytes).contains("private"));
    f.finish().await;
}
