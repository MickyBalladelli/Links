use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use links_account_auth::{
    provider::TwilioVerify,
    service::{AccountAuth, SystemClock},
    web,
};
use sqlx::postgres::PgPoolOptions;
use std::{net::SocketAddr, sync::Arc, time::Duration};
use zeroize::Zeroizing;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let key_text =
        Zeroizing::new(std::env::var("AUTH_LOOKUP_KEY").map_err(|_| {
            "AUTH_LOOKUP_KEY is required (32 random bytes, base64url without padding)"
        })?);
    let key = Zeroizing::new(
        URL_SAFE_NO_PAD
            .decode(key_text.as_bytes())
            .map_err(|_| "invalid AUTH_LOOKUP_KEY encoding")?,
    );
    let key = Zeroizing::new(<[u8;32]>::try_from(key.as_slice())
        .map_err(|_| "AUTH_LOOKUP_KEY must encode 32 bytes")?);
    let provider = Arc::new(TwilioVerify::new(
        std::env::var("TWILIO_ACCOUNT_SID")?,
        std::env::var("TWILIO_AUTH_TOKEN")?,
        std::env::var("TWILIO_VERIFY_SERVICE_SID")?,
    )?);
    let address: SocketAddr = std::env::var("AUTH_BIND")
        .unwrap_or_else(|_| "127.0.0.1:8080".into())
        .parse()?;
    if !address.ip().is_loopback() {
        return Err("auth HTTP must bind to loopback behind a TLS terminator".into());
    }
    let pool = PgPoolOptions::new()
        .max_connections(10)
        .connect(&std::env::var("DATABASE_URL")?)
        .await?;
    // Run the explicit server-store migration command before launching this process.
    let auth = Arc::new(AccountAuth::new(
        pool.clone(),
        provider,
        key,
        Arc::new(SystemClock),
    )?);
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
        web::router(auth).into_make_service_with_connect_info::<SocketAddr>(),
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
