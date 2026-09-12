use crate::{
    service::{AccountAuth, FinishRequest, StartRequest},
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
        .route("/v1/auth/me", get(me))
        .layer(DefaultBodyLimit::max(4096));
    let prekey_routes = Router::new()
        .route("/v1/prekeys", put(upload_prekeys))
        .route("/v1/prekeys/status", get(prekey_inventory))
        .route("/v1/prekeys/{device_id}/claim", post(claim_prekeys))
        .layer(DefaultBodyLimit::max(MAX_PREKEY_UPLOAD_BYTES));
    Router::new()
        .merge(auth_routes)
        .merge(prekey_routes)
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
async fn me(
    State(auth): State<Arc<AccountAuth>>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, AuthError> {
    Ok(Json(auth.authenticate(bearer(&headers)?).await?))
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
