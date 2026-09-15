//! Local Links composition: PostgreSQL, username auth, mailbox storage,
//! ephemeral sessions, and the loopback WebSocket gateway.
//!
//! This binary is Debug-only by design. It creates no OTP bypass in Release
//! builds and refuses non-loopback listeners.
use axum::serve;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
#[cfg(debug_assertions)]
use links_account_auth::service::SystemClock;
use links_account_auth::{service::AccountAuth, web};
use links_gateway::{
    websocket::WebSocketAdapter, AccountAuthDeviceAuthenticator, Gateway, GatewayConfig,
    GatewayError, PushNotifier, PushWakeup, RegionBus,
};
use links_server_store::{ephemeral::MemoryEphemeralState, postgres::RelationalStore};
use sqlx::{postgres::PgPoolOptions, PgPool};
use std::{error::Error, net::SocketAddr, sync::Arc};
use tokio::net::TcpListener;
use zeroize::Zeroizing;

struct LocalRegionBus;

#[async_trait::async_trait]
impl RegionBus for LocalRegionBus {
    async fn forward(
        &self,
        _destination_gateway_id: &str,
        _delivery: links_gateway::ForwardedEnvelope,
    ) -> Result<(), GatewayError> {
        Err(GatewayError::Unavailable)
    }
}

struct LocalPushNotifier;

#[async_trait::async_trait]
impl PushNotifier for LocalPushNotifier {
    async fn notify(&self, _wakeup: PushWakeup) -> Result<(), GatewayError> {
        Ok(())
    }
}

#[cfg(debug_assertions)]
fn build_auth(
    pool: PgPool,
    lookup_key: Zeroizing<[u8; 32]>,
) -> Result<Arc<AccountAuth>, Box<dyn Error>> {
    Ok(Arc::new(AccountAuth::new_loopback_username_dev(
        pool,
        lookup_key,
        Arc::new(SystemClock),
    )?))
}

#[cfg(not(debug_assertions))]
fn build_auth(
    _pool: PgPool,
    _lookup_key: Zeroizing<[u8; 32]>,
) -> Result<Arc<AccountAuth>, Box<dyn Error>> {
    Err("links-local-dev requires a Debug build".into())
}

fn lookup_key() -> Result<Zeroizing<[u8; 32]>, Box<dyn Error>> {
    let encoded = std::env::var("AUTH_LOOKUP_KEY")
        .map_err(|_| "AUTH_LOOKUP_KEY must be a 32-byte base64url secret")?;
    let decoded = URL_SAFE_NO_PAD
        .decode(encoded.as_bytes())
        .map_err(|_| "AUTH_LOOKUP_KEY must be base64url without padding")?;
    Ok(Zeroizing::new(
        <[u8; 32]>::try_from(decoded.as_slice())
            .map_err(|_| "AUTH_LOOKUP_KEY must decode to exactly 32 bytes")?,
    ))
}

fn loopback_address(name: &str, default: &str) -> Result<SocketAddr, Box<dyn Error>> {
    let address: SocketAddr = std::env::var(name)
        .unwrap_or_else(|_| default.to_owned())
        .parse()?;
    if !address.ip().is_loopback() {
        return Err(format!("{name} must bind to loopback").into());
    }
    Ok(address)
}

async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    if std::env::var("AUTH_DEV_USERNAME_MODE").as_deref() != Ok("1") {
        return Err("set AUTH_DEV_USERNAME_MODE=1 for the loopback composition".into());
    }
    if !cfg!(debug_assertions) {
        return Err("AUTH_DEV_USERNAME_MODE is unavailable in Release builds".into());
    }

    let database_url = std::env::var("DATABASE_URL")?;
    let pool = PgPoolOptions::new()
        .max_connections(10)
        .connect(&database_url)
        .await?;
    let store = RelationalStore::from_pool(pool.clone());
    store.migrate().await?;
    let auth = build_auth(pool.clone(), lookup_key()?)?;

    let auth_address = loopback_address("AUTH_BIND", "127.0.0.1:8080")?;
    let gateway_address = loopback_address("GATEWAY_BIND", "127.0.0.1:8081")?;
    let gateway_id = std::env::var("GATEWAY_ID").unwrap_or_else(|_| "local-gateway".to_owned());
    let region = std::env::var("GATEWAY_REGION").unwrap_or_else(|_| "local".to_owned());
    let gateway = Arc::new(Gateway::new(
        GatewayConfig::new(gateway_id, region)?,
        Arc::new(MemoryEphemeralState::new(1_024)?),
        Arc::new(store.clone()),
        Arc::new(AccountAuthDeviceAuthenticator::new(auth.clone())),
        Arc::new(LocalRegionBus),
        Arc::new(LocalPushNotifier),
    ));
    let adapter = WebSocketAdapter::new(gateway);
    let auth_listener = TcpListener::bind(auth_address).await?;
    let gateway_listener = TcpListener::bind(gateway_address).await?;
    println!("Links local auth: http://{auth_address}");
    println!("Links local gateway: ws://{gateway_address}/v1/connect");

    let auth_server = serve(
        auth_listener,
        web::router(auth).into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal());
    let gateway_server =
        serve(gateway_listener, adapter.router()).with_graceful_shutdown(shutdown_signal());
    tokio::select! {
        result = auth_server => result?,
        result = gateway_server => result?,
    }
    pool.close().await;
    Ok(())
}
