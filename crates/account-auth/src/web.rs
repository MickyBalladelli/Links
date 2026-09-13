use crate::{
    service::{
        AccountAuth, DeviceRegistrationRequest, EncryptedKeyBackupRequest, FinishRequest,
        PasskeyAssertionFinishRequest, PasskeyRegistrationFinishRequest, StartRequest,
    },
    AuthError,
};
use axum::{
    body::Bytes,
    extract::{rejection::JsonRejection, ConnectInfo, DefaultBodyLimit, Path, State},
    http::{header, HeaderMap, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post, put},
    Json, Router,
};
use links_protocol::{v1, MAX_PREKEY_UPLOAD_BYTES};
use prost::Message;
use std::{net::SocketAddr, sync::Arc};

pub fn router(auth: Arc<AccountAuth>) -> Router {
    let auth_routes = Router::new()
        .route("/v1/auth/start", post(start))
        .route("/v1/auth/finish", post(finish))
        .route("/v1/auth/username/register", post(username_register))
        .route("/v1/auth/username/login", post(username_login))
        .route("/v1/auth/me", get(me))
        .route("/v1/devices", post(register_device))
        .layer(DefaultBodyLimit::max(4096));
    let passkey_routes = Router::new()
        .route("/v1/passkeys/register/start", post(passkey_register_start))
        .route(
            "/v1/passkeys/register/finish",
            post(passkey_register_finish),
        )
        .route("/v1/passkeys/assert/start", post(passkey_assert_start))
        .route("/v1/passkeys/assert/finish", post(passkey_assert_finish))
        .route("/v1/passkey-backups", put(put_passkey_backup))
        .route("/v1/passkey-backups/{backup_id}", get(get_passkey_backup))
        .layer(DefaultBodyLimit::max(16 * 1024));
    let prekey_routes = Router::new()
        .route("/v1/prekeys", put(upload_prekeys))
        .route("/v1/prekeys/status", get(prekey_inventory))
        .route("/v1/prekeys/{device_id}/claim", post(claim_prekeys))
        .layer(DefaultBodyLimit::max(MAX_PREKEY_UPLOAD_BYTES));
    let directory_routes = Router::new()
        .route("/v1/directory/{handle}", get(directory_lookup))
        .layer(DefaultBodyLimit::max(4096));
    Router::new()
        .merge(auth_routes)
        .merge(passkey_routes)
        .merge(prekey_routes)
        .merge(directory_routes)
        .layer(middleware::from_fn(no_store))
        .with_state(auth)
}
async fn no_store(request: axum::extract::Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
    response
}
async fn start(
    State(auth): State<Arc<AccountAuth>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    request: Result<Json<StartRequest>, JsonRejection>,
) -> Result<impl IntoResponse, AuthError> {
    Ok(Json(
        auth.start(request.map_err(|_| AuthError::Invalid)?.0, peer.ip())
            .await?,
    ))
}
async fn finish(
    State(auth): State<Arc<AccountAuth>>,
    request: Result<Json<FinishRequest>, JsonRejection>,
) -> Result<impl IntoResponse, AuthError> {
    Ok(Json(
        auth.finish(request.map_err(|_| AuthError::Invalid)?.0)
            .await?,
    ))
}
async fn username_register(
    State(auth): State<Arc<AccountAuth>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    request: Result<Json<crate::service::UsernameRegistrationRequest>, JsonRejection>,
) -> Result<impl IntoResponse, AuthError> {
    Ok(Json(
        auth.register_username(request.map_err(|_| AuthError::Invalid)?.0, peer.ip())
            .await?,
    ))
}
async fn username_login(
    State(auth): State<Arc<AccountAuth>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    request: Result<Json<crate::service::UsernameLoginRequest>, JsonRejection>,
) -> Result<impl IntoResponse, AuthError> {
    Ok(Json(
        auth.login_username(request.map_err(|_| AuthError::Invalid)?.0, peer.ip())
            .await?,
    ))
}
async fn directory_lookup(
    State(auth): State<Arc<AccountAuth>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(handle): Path<String>,
) -> Result<Response, AuthError> {
    let handle = handle.strip_prefix('@').unwrap_or(&handle);
    if handle.is_empty() || handle.starts_with('@') {
        return Err(AuthError::Invalid);
    }
    match auth.lookup_username_directory(handle, peer.ip()).await? {
        Some(directory) => Ok(Json(directory).into_response()),
        None => Ok(StatusCode::NOT_FOUND.into_response()),
    }
}
async fn me(
    State(auth): State<Arc<AccountAuth>>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, AuthError> {
    Ok(Json(auth.authenticate(bearer(&headers)?).await?))
}
async fn register_device(
    State(auth): State<Arc<AccountAuth>>,
    headers: HeaderMap,
    request: Result<Json<DeviceRegistrationRequest>, JsonRejection>,
) -> Result<impl IntoResponse, AuthError> {
    Ok(Json(
        auth.register_device(
            bearer(&headers)?,
            request.map_err(|_| AuthError::Invalid)?.0,
        )
        .await?,
    ))
}
async fn passkey_register_start(
    State(auth): State<Arc<AccountAuth>>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, AuthError> {
    Ok(Json(
        auth.passkey_registration_start(bearer(&headers)?).await?,
    ))
}
async fn passkey_register_finish(
    State(auth): State<Arc<AccountAuth>>,
    headers: HeaderMap,
    request: Result<Json<PasskeyRegistrationFinishRequest>, JsonRejection>,
) -> Result<impl IntoResponse, AuthError> {
    Ok(Json(
        auth.passkey_registration_finish(
            bearer(&headers)?,
            request.map_err(|_| AuthError::Invalid)?.0,
        )
        .await?,
    ))
}
async fn passkey_assert_start(
    State(auth): State<Arc<AccountAuth>>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, AuthError> {
    Ok(Json(auth.passkey_assertion_start(bearer(&headers)?).await?))
}
async fn passkey_assert_finish(
    State(auth): State<Arc<AccountAuth>>,
    headers: HeaderMap,
    request: Result<Json<PasskeyAssertionFinishRequest>, JsonRejection>,
) -> Result<impl IntoResponse, AuthError> {
    Ok(Json(
        auth.passkey_assertion_finish(
            bearer(&headers)?,
            request.map_err(|_| AuthError::Invalid)?.0,
        )
        .await?,
    ))
}
async fn put_passkey_backup(
    State(auth): State<Arc<AccountAuth>>,
    headers: HeaderMap,
    request: Result<Json<EncryptedKeyBackupRequest>, JsonRejection>,
) -> Result<impl IntoResponse, AuthError> {
    auth.put_encrypted_key_backup(
        bearer(&headers)?,
        request.map_err(|_| AuthError::Invalid)?.0,
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}
async fn get_passkey_backup(
    State(auth): State<Arc<AccountAuth>>,
    Path(backup_id): Path<uuid::Uuid>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, AuthError> {
    Ok(Json(
        auth.get_encrypted_key_backup(bearer(&headers)?, backup_id)
            .await?,
    ))
}
async fn upload_prekeys(
    State(auth): State<Arc<AccountAuth>>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, AuthError> {
    require_protobuf(&headers)?;
    let upload = v1::PreKeyUpload::decode(body).map_err(|_| AuthError::Invalid)?;
    protobuf(auth.upload_prekeys(bearer(&headers)?, upload).await?)
}
async fn prekey_inventory(
    State(auth): State<Arc<AccountAuth>>,
    headers: HeaderMap,
) -> Result<Response, AuthError> {
    protobuf(auth.prekey_inventory(bearer(&headers)?).await?)
}
async fn claim_prekeys(
    State(auth): State<Arc<AccountAuth>>,
    Path(device_id): Path<uuid::Uuid>,
    headers: HeaderMap,
) -> Result<Response, AuthError> {
    protobuf(
        auth.claim_prekey_bundle(bearer(&headers)?, device_id)
            .await?,
    )
}
fn bearer(headers: &HeaderMap) -> Result<&str, AuthError> {
    headers
        .get(header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
        .and_then(|h| h.strip_prefix("Bearer "))
        .ok_or(AuthError::Denied)
}
fn require_protobuf(headers: &HeaderMap) -> Result<(), AuthError> {
    let content_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(';').next())
        .map(str::trim);
    if content_type != Some("application/x-protobuf") {
        return Err(AuthError::Invalid);
    }
    Ok(())
}
fn protobuf(message: impl Message) -> Result<Response, AuthError> {
    let mut body = Vec::with_capacity(message.encoded_len());
    message
        .encode(&mut body)
        .map_err(|_| AuthError::Unavailable)?;
    Ok(([(header::CONTENT_TYPE, "application/x-protobuf")], body).into_response())
}
impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        let (status, code) = match self {
            Self::Invalid => (StatusCode::BAD_REQUEST, "invalid_request"),
            Self::Denied => (StatusCode::UNAUTHORIZED, "authentication_failed"),
            Self::RateLimited => (StatusCode::TOO_MANY_REQUESTS, "rate_limited"),
            Self::Conflict => (StatusCode::CONFLICT, "conflicting_write"),
            Self::Unavailable => (StatusCode::SERVICE_UNAVAILABLE, "temporarily_unavailable"),
        };
        let mut response = (status, Json(serde_json::json!({"error":code}))).into_response();
        if status == StatusCode::TOO_MANY_REQUESTS {
            response
                .headers_mut()
                .insert(header::RETRY_AFTER, "3600".parse().unwrap());
        }
        response
    }
}
