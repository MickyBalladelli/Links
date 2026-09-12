use crate::{
    service::{AccountAuth, FinishRequest, StartRequest},
    AuthError,
};
use axum::{
    extract::{rejection::JsonRejection, ConnectInfo, DefaultBodyLimit, State},
    http::{header, HeaderMap, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use std::{net::SocketAddr, sync::Arc};

pub fn router(auth: Arc<AccountAuth>) -> Router {
    Router::new()
        .route("/v1/auth/start", post(start))
        .route("/v1/auth/finish", post(finish))
        .route("/v1/auth/me", get(me))
        .layer(DefaultBodyLimit::max(4096))
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
    let token = headers
        .get(header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
        .and_then(|h| h.strip_prefix("Bearer "))
        .ok_or(AuthError::Denied)?;
    Ok(Json(auth.authenticate(token).await?))
}
impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        let (status, code) = match self {
            Self::Invalid => (StatusCode::BAD_REQUEST, "invalid_request"),
            Self::Denied => (StatusCode::UNAUTHORIZED, "authentication_failed"),
            Self::RateLimited => (StatusCode::TOO_MANY_REQUESTS, "rate_limited"),
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
