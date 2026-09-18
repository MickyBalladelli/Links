use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use links_account_auth::{
    passkeys::PasskeyConfig,
    provider::TwilioVerify,
    service::{AccountAuth, SystemClock},
    web,
};
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use std::{
    collections::HashSet,
    error::Error,
    net::{IpAddr, SocketAddr},
    sync::Arc,
    time::Duration,
};
use zeroize::Zeroizing;

const DEV_USERNAME_MODE_ENV: &str = "AUTH_DEV_USERNAME_MODE";

fn flag(name: &str) -> Result<bool, Box<dyn Error>> {
    match std::env::var(name) {
        Ok(value) if value == "1" => Ok(true),
        Ok(value) if value == "0" || value.is_empty() => Ok(false),
        Ok(_) => Err(format!("{name} must be exactly 1 or 0").into()),
        Err(std::env::VarError::NotPresent) => Ok(false),
        Err(std::env::VarError::NotUnicode(_)) => Err(format!("{name} is not valid UTF-8").into()),
    }
}

fn trusted_proxy_ips() -> Result<HashSet<IpAddr>, Box<dyn Error>> {
    let value = match std::env::var("AUTH_TRUSTED_PROXY_IPS") {
        Ok(value) => value,
        Err(std::env::VarError::NotPresent) => return Ok(HashSet::new()),
        Err(std::env::VarError::NotUnicode(_)) => {
            return Err("AUTH_TRUSTED_PROXY_IPS is not valid UTF-8".into())
        }
    };
    let mut proxies = HashSet::new();
    for entry in value.split(',').map(str::trim).filter(|entry| !entry.is_empty()) {
        proxies.insert(entry.parse::<IpAddr>().map_err(|_| {
            format!("AUTH_TRUSTED_PROXY_IPS contains an invalid IP address: {entry}")
        })?);
    }
    Ok(proxies)
}

fn build_production_auth(
    pool: PgPool,
    key: Zeroizing<[u8; 32]>,
    passkey: Option<PasskeyConfig>,
) -> Result<Arc<AccountAuth>, Box<dyn Error>> {
    let provider = Arc::new(TwilioVerify::new(
        std::env::var("TWILIO_ACCOUNT_SID")?,
        std::env::var("TWILIO_AUTH_TOKEN")?,
        std::env::var("TWILIO_VERIFY_SERVICE_SID")?,
    )?);
    let auth = match passkey {
        Some(passkey) => AccountAuth::new_with_passkey(
            pool,
            provider,
            key,
            Arc::new(SystemClock),
            passkey,
        )?,
        None => AccountAuth::new(pool, provider, key, Arc::new(SystemClock))?,
    };
    Ok(Arc::new(auth))
}

#[cfg(debug_assertions)]
fn build_auth(
    pool: PgPool,
    key: Zeroizing<[u8; 32]>,
    passkey: Option<PasskeyConfig>,
    dev_mode: bool,
) -> Result<Arc<AccountAuth>, Box<dyn Error>> {
    if !dev_mode {
        return build_production_auth(pool, key, passkey);
    }
    let auth = match passkey {
        Some(passkey) => AccountAuth::new_loopback_username_dev_with_passkey(
            pool,
            key,
            Arc::new(SystemClock),
            passkey,
        )?,
        None => AccountAuth::new_loopback_username_dev(pool, key, Arc::new(SystemClock))?,
    };
    Ok(Arc::new(auth))
}

#[cfg(not(debug_assertions))]
fn build_auth(
    pool: PgPool,
    key: Zeroizing<[u8; 32]>,
    passkey: Option<PasskeyConfig>,
    dev_mode: bool,
) -> Result<Arc<AccountAuth>, Box<dyn Error>> {
    if dev_mode {
        return Err("AUTH_DEV_USERNAME_MODE is unavailable in Release builds".into());
    }
    build_production_auth(pool, key, passkey)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dev_mode = flag(DEV_USERNAME_MODE_ENV)?;
    let trusted_proxies = Arc::new(trusted_proxy_ips()?);
    if dev_mode && !trusted_proxies.is_empty() {
        return Err("AUTH_TRUSTED_PROXY_IPS is unavailable in username development mode".into());
    }
    if dev_mode && !cfg!(debug_assertions) {
        return Err("AUTH_DEV_USERNAME_MODE is unavailable in Release builds".into());
    }
    if !cfg!(debug_assertions) && trusted_proxies.is_empty() {
        return Err("AUTH_TRUSTED_PROXY_IPS is required in Release builds".into());
    }
    let key_text =
        Zeroizing::new(std::env::var("AUTH_LOOKUP_KEY").map_err(|_| {
            "AUTH_LOOKUP_KEY is required (32 random bytes, base64url without padding)"
        })?);
    let key = Zeroizing::new(
        URL_SAFE_NO_PAD
            .decode(key_text.as_bytes())
            .map_err(|_| "invalid AUTH_LOOKUP_KEY encoding")?,
    );
    let key = Zeroizing::new(
        <[u8; 32]>::try_from(key.as_slice()).map_err(|_| "AUTH_LOOKUP_KEY must encode 32 bytes")?,
    );
    let address: SocketAddr = std::env::var("AUTH_BIND")
        .unwrap_or_else(|_| "127.0.0.1:8080".into())
        .parse()?;
    if dev_mode && !address.ip().is_loopback() {
        return Err("AUTH_DEV_USERNAME_MODE requires a loopback AUTH_BIND".into());
    }
    if !address.ip().is_loopback() {
        return Err("auth HTTP must bind to loopback behind a TLS terminator".into());
    }
    let pool = PgPoolOptions::new()
        .max_connections(10)
        .connect(&std::env::var("DATABASE_URL")?)
        .await?;
    let passkey = match (
        std::env::var("PASSKEY_RP_ID"),
        std::env::var("PASSKEY_ORIGIN"),
    ) {
        (Ok(rp_id), Ok(origin)) => Some(PasskeyConfig::new(rp_id, origin)?),
        (Err(std::env::VarError::NotPresent), Err(std::env::VarError::NotPresent)) => None,
        _ => return Err("PASSKEY_RP_ID and PASSKEY_ORIGIN must be set together".into()),
    };
    // Run the explicit server-store migration command before launching this process.
    let auth = build_auth(pool.clone(), key, passkey, dev_mode)?;
    if dev_mode {
        println!(
            "Links auth development mode: loopback username accounts only; OTP disabled."
        );
    }
    auth.purge_expired().await?; // Fail startup on missing schema, not on first user request.
    let cleanup = auth.clone();
    let job = tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(60));
        loop {
            interval.tick().await;
            if cleanup.purge_expired().await.is_err() {
                eprintln!("Authentication cleanup unavailable; retrying on next interval.");
            }
        }
    });
    let listener = tokio::net::TcpListener::bind(address).await?;
    println!(
        "Links auth listening on loopback; TLS termination is required outside local development."
    );
    let result = axum::serve(
        listener,
        web::router_with_trusted_proxies(auth, trusted_proxies)
            .into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(async {
        let _ = tokio::signal::ctrl_c().await;
    })
    .await;
    job.abort();
    pool.close().await;
    result?;
    Ok(())
}
