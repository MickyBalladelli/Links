use crate::{
    service::{
        AccountAuth, ChatProofOfWorkVerifyRequest, ContactPsiQueryRequest, CreateGroupRequest,
        DelegatedDeviceRegistrationRequest, DeviceRegistrationRequest,
        EncryptedKeyBackupRequest, FinishRequest,
        OrganizationControlsRequest,
        PasskeyAssertionFinishRequest, PasskeyRegistrationFinishRequest, PrivacyPassIssueRequest,
        PrivacyPassRedeemRequest, SetGroupRoleRequest, StartRequest, UsernameChallengeRequest,
    },
    AuthError,
};
use axum::{
    body::Bytes,
    extract::{rejection::JsonRejection, ConnectInfo, DefaultBodyLimit, Extension, Path, Query, State},
    http::{header, HeaderMap, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{delete, get, post, put},
    Json, Router,
};
use links_protocol::{v1, MAX_PREKEY_UPLOAD_BYTES};
use prost::Message;
use std::{
    collections::HashSet,
    net::{IpAddr, SocketAddr},
    sync::Arc,
};

#[derive(Clone, Copy)]
struct ClientIp(IpAddr);

pub fn router(auth: Arc<AccountAuth>) -> Router {
    router_with_trusted_proxies(auth, Arc::new(HashSet::new()))
}

pub fn router_with_trusted_proxies(
    auth: Arc<AccountAuth>,
    trusted_proxies: Arc<HashSet<IpAddr>>,
) -> Router {
    let mut auth_routes = Router::new()
        .route("/v1/auth/username/challenge", post(username_challenge))
        .route("/v1/auth/username/register", post(username_register))
        .route("/v1/auth/username/login", post(username_login))
        .route("/v1/auth/logout", post(logout))
        .route("/v1/auth/sessions/others", delete(revoke_other_sessions))
        .route("/v1/auth/me", get(me))
        .route(
            "/v1/organization/controls",
            get(organization_controls).put(set_organization_controls),
        )
        .route("/v1/devices", post(register_device))
        .route("/v1/devices/delegated", post(register_delegated_device))
        .route("/v1/devices/{device_id}", delete(revoke_device));
    if !auth.is_loopback_username_dev() {
        auth_routes = auth_routes
            .route("/v1/auth/start", post(start))
            .route("/v1/auth/finish", post(finish));
    }
    let auth_routes = auth_routes.layer(DefaultBodyLimit::max(4096));
    let group_routes = Router::new()
        .route("/v1/groups", post(create_group))
        .route("/v1/groups/{group_id}/members", get(group_members))
        .route(
            "/v1/groups/{group_id}/members/{user_id}/role",
            put(set_group_role),
        )
        .route(
            "/v1/groups/{group_id}/members/{user_id}",
            delete(remove_group_member),
        )
        .route("/v1/groups/{group_id}", delete(delete_group))
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
    let mls_routes = Router::new()
        .route("/v1/mls/key-package", put(upload_mls_key_package))
        .route("/v1/mls/key-package/{device_id}", get(download_mls_key_package))
        .layer(DefaultBodyLimit::max(links_protocol::MAX_FRAME_BYTES));
    let directory_routes = Router::new()
        .route("/v1/directory/users/{user_id}", get(directory_lookup_by_user_id))
        .route("/v1/directory/{handle}", get(directory_lookup))
        .layer(DefaultBodyLimit::max(4096));
    let profile_picture_routes = Router::new()
        .route(
            "/v1/profile/picture",
            put(put_profile_picture).delete(delete_profile_picture),
        )
        .route(
            "/v1/directory/{handle}/picture",
            get(directory_profile_picture),
        )
        .layer(DefaultBodyLimit::max(131_072));
    let admin_routes = Router::new()
        .route("/v1/admin/users", get(admin_users))
        .route("/v1/admin/users/{user_id}", delete(admin_delete_user))
        .route("/v1/admin/users/{user_id}/status", put(admin_set_user_status))
        .route(
            "/v1/admin/users/{user_id}/devices/{device_id}",
            delete(admin_revoke_device),
        )
        .layer(DefaultBodyLimit::max(16 * 1024));
    let contact_psi_routes = Router::new()
        .route(
            "/v1/contact-discovery/parameters",
            get(contact_psi_parameters),
        )
        .route("/v1/contact-discovery/query", post(contact_psi_query))
        .layer(DefaultBodyLimit::max(64 * 1024));
    let privacy_pass_routes = Router::new()
        .route("/v1/privacy-pass/parameters", get(privacy_pass_parameters))
        .route("/v1/privacy-pass/challenge", get(privacy_pass_challenge))
        .route("/v1/privacy-pass/issue", post(privacy_pass_issue))
        .route("/v1/privacy-pass/redeem", post(privacy_pass_redeem))
        .layer(DefaultBodyLimit::max(4096));
    let chat_pow_routes = Router::new()
        .route(
            "/v1/chat-requests/proof-of-work/challenge",
            get(chat_pow_challenge),
        )
        .route(
            "/v1/chat-requests/proof-of-work/verify",
            post(chat_pow_verify),
        )
        .layer(DefaultBodyLimit::max(4096));
    Router::new()
        .merge(auth_routes)
        .merge(group_routes)
        .merge(passkey_routes)
        .merge(prekey_routes)
        .merge(mls_routes)
        .merge(directory_routes)
        .merge(profile_picture_routes)
        .merge(admin_routes)
        .merge(contact_psi_routes)
        .merge(privacy_pass_routes)
        .merge(chat_pow_routes)
        .layer(middleware::from_fn_with_state(
            trusted_proxies,
            resolve_client_ip,
        ))
        .layer(middleware::from_fn(no_store))
        .with_state(auth)
}

async fn resolve_client_ip(
    State(trusted_proxies): State<Arc<HashSet<IpAddr>>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    mut request: axum::extract::Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let client_ip = client_ip_from_headers(peer.ip(), &trusted_proxies, request.headers())?;
    request.extensions_mut().insert(ClientIp(client_ip));
    Ok(next.run(request).await)
}

fn client_ip_from_headers(
    peer_ip: IpAddr,
    trusted_proxies: &HashSet<IpAddr>,
    headers: &HeaderMap,
) -> Result<IpAddr, StatusCode> {
    if !trusted_proxies.contains(&peer_ip) {
        return Ok(peer_ip);
    }
    let mut forwarded = headers.get_all("x-forwarded-for").iter();
    let value = forwarded.next().ok_or(StatusCode::BAD_REQUEST)?;
    if forwarded.next().is_some() {
        return Err(StatusCode::BAD_REQUEST);
    }
    let value = value.to_str().map_err(|_| StatusCode::BAD_REQUEST)?.trim();
    if value.contains(',') {
        return Err(StatusCode::BAD_REQUEST);
    }
    value.parse::<IpAddr>().map_err(|_| StatusCode::BAD_REQUEST)
}

async fn no_store(request: axum::extract::Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
    if response.status() == StatusCode::TOO_MANY_REQUESTS {
        eprintln!("Authentication rate limit saturated; inspect ingress and provider metrics.");
    }
    response
}
async fn start(
    State(auth): State<Arc<AccountAuth>>,
    Extension(ClientIp(peer_ip)): Extension<ClientIp>,
    request: Result<Json<StartRequest>, JsonRejection>,
) -> Result<impl IntoResponse, AuthError> {
    Ok(Json(
        auth.start(request.map_err(|_| AuthError::Invalid)?.0, peer_ip)
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
async fn username_challenge(
    State(auth): State<Arc<AccountAuth>>,
    Extension(ClientIp(peer_ip)): Extension<ClientIp>,
    request: Result<Json<UsernameChallengeRequest>, JsonRejection>,
) -> Result<impl IntoResponse, AuthError> {
    Ok(Json(
        auth.start_username_challenge(request.map_err(|_| AuthError::Invalid)?.0, peer_ip)
            .await?,
    ))
}
async fn username_register(
    State(auth): State<Arc<AccountAuth>>,
    Extension(ClientIp(peer_ip)): Extension<ClientIp>,
    request: Result<Json<crate::service::UsernameRegistrationRequest>, JsonRejection>,
) -> Result<impl IntoResponse, AuthError> {
    Ok(Json(
        auth.register_username(request.map_err(|_| AuthError::Invalid)?.0, peer_ip)
            .await?,
    ))
}
async fn username_login(
    State(auth): State<Arc<AccountAuth>>,
    Extension(ClientIp(peer_ip)): Extension<ClientIp>,
    request: Result<Json<crate::service::UsernameLoginRequest>, JsonRejection>,
) -> Result<impl IntoResponse, AuthError> {
    Ok(Json(
        auth.login_username(request.map_err(|_| AuthError::Invalid)?.0, peer_ip)
            .await?,
    ))
}
async fn logout(
    State(auth): State<Arc<AccountAuth>>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, AuthError> {
    auth.logout(bearer(&headers)?).await?;
    Ok(StatusCode::NO_CONTENT)
}
async fn revoke_other_sessions(
    State(auth): State<Arc<AccountAuth>>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, AuthError> {
    auth.revoke_other_sessions(bearer(&headers)?).await?;
    Ok(StatusCode::NO_CONTENT)
}
async fn directory_lookup_by_user_id(
    State(auth): State<Arc<AccountAuth>>,
    Extension(ClientIp(peer_ip)): Extension<ClientIp>,
    headers: HeaderMap,
    Path(user_id): Path<uuid::Uuid>,
) -> Result<Response, AuthError> {
    match auth
        .lookup_username_directory_by_user_id(bearer(&headers)?, user_id, peer_ip)
        .await?
    {
        Some(directory) => Ok(Json(directory).into_response()),
        None => Ok(StatusCode::NOT_FOUND.into_response()),
    }
}

async fn put_profile_picture(
    State(auth): State<Arc<AccountAuth>>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<impl IntoResponse, AuthError> {
    auth.put_profile_picture(bearer(&headers)?, body.to_vec())
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn delete_profile_picture(
    State(auth): State<Arc<AccountAuth>>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, AuthError> {
    auth.delete_profile_picture(bearer(&headers)?).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn directory_profile_picture(
    State(auth): State<Arc<AccountAuth>>,
    Extension(ClientIp(peer_ip)): Extension<ClientIp>,
    Path(handle): Path<String>,
) -> Result<Response, AuthError> {
    let handle = handle.strip_prefix('@').unwrap_or(&handle);
    match auth.lookup_profile_picture(handle, peer_ip).await? {
        Some(jpeg) => Ok(([(header::CONTENT_TYPE, "image/jpeg")], jpeg).into_response()),
        None => Ok(StatusCode::NOT_FOUND.into_response()),
    }
}

async fn directory_lookup(
    State(auth): State<Arc<AccountAuth>>,
    Extension(ClientIp(peer_ip)): Extension<ClientIp>,
    Path(handle): Path<String>,
) -> Result<Response, AuthError> {
    let handle = handle.strip_prefix('@').unwrap_or(&handle);
    if handle.is_empty() || handle.starts_with('@') {
        return Err(AuthError::Invalid);
    }
    match auth.lookup_username_directory(handle, peer_ip).await? {
        Some(directory) => Ok(Json(directory).into_response()),
        None => Ok(StatusCode::NOT_FOUND.into_response()),
    }
}

#[derive(serde::Deserialize)]
struct AdminUsersQuery {
    #[serde(default)]
    search: String,
    #[serde(default = "default_admin_user_limit")]
    limit: i64,
}

fn default_admin_user_limit() -> i64 {
    100
}

fn require_admin(auth: &AccountAuth, headers: &HeaderMap) -> Result<(), AuthError> {
    let key = headers
        .get("x-links-admin-key")
        .and_then(|value| value.to_str().ok())
        .ok_or(AuthError::Denied)?;
    auth.authorize_admin_key(key)
}

async fn admin_users(
    State(auth): State<Arc<AccountAuth>>,
    headers: HeaderMap,
    Query(query): Query<AdminUsersQuery>,
) -> Result<impl IntoResponse, AuthError> {
    require_admin(&auth, &headers)?;
    Ok(Json(auth.admin_users(&query.search, query.limit).await?))
}

async fn admin_set_user_status(
    State(auth): State<Arc<AccountAuth>>,
    headers: HeaderMap,
    Path(user_id): Path<uuid::Uuid>,
    request: Result<Json<crate::service::AdminUserStatusRequest>, JsonRejection>,
) -> Result<impl IntoResponse, AuthError> {
    require_admin(&auth, &headers)?;
    let request = request.map_err(|_| AuthError::Invalid)?.0;
    auth.admin_set_user_disabled(user_id, request.disabled).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn admin_delete_user(
    State(auth): State<Arc<AccountAuth>>,
    headers: HeaderMap,
    Path(user_id): Path<uuid::Uuid>,
) -> Result<impl IntoResponse, AuthError> {
    require_admin(&auth, &headers)?;
    auth.admin_delete_user(user_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn admin_revoke_device(
    State(auth): State<Arc<AccountAuth>>,
    headers: HeaderMap,
    Path((user_id, device_id)): Path<(uuid::Uuid, uuid::Uuid)>,
) -> Result<impl IntoResponse, AuthError> {
    require_admin(&auth, &headers)?;
    auth.admin_revoke_device(user_id, device_id).await?;
    Ok(StatusCode::NO_CONTENT)
}
async fn contact_psi_parameters(
    State(auth): State<Arc<AccountAuth>>,
    Extension(ClientIp(peer_ip)): Extension<ClientIp>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, AuthError> {
    Ok(Json(
        auth.contact_psi_parameters(bearer(&headers)?, peer_ip)
            .await?,
    ))
}
async fn contact_psi_query(
    State(auth): State<Arc<AccountAuth>>,
    Extension(ClientIp(peer_ip)): Extension<ClientIp>,
    headers: HeaderMap,
    request: Result<Json<ContactPsiQueryRequest>, JsonRejection>,
) -> Result<impl IntoResponse, AuthError> {
    Ok(Json(
        auth.contact_psi_query(
            bearer(&headers)?,
            peer_ip,
            request.map_err(|_| AuthError::Invalid)?.0,
        )
        .await?,
    ))
}
async fn privacy_pass_parameters(
    State(auth): State<Arc<AccountAuth>>,
) -> Result<impl IntoResponse, AuthError> {
    Ok(Json(auth.privacy_pass_parameters()?))
}
async fn privacy_pass_challenge(
    State(auth): State<Arc<AccountAuth>>,
    Extension(ClientIp(peer_ip)): Extension<ClientIp>,
) -> Result<impl IntoResponse, AuthError> {
    Ok(Json(auth.privacy_pass_challenge(peer_ip).await?))
}
async fn privacy_pass_issue(
    State(auth): State<Arc<AccountAuth>>,
    Extension(ClientIp(peer_ip)): Extension<ClientIp>,
    headers: HeaderMap,
    request: Result<Json<PrivacyPassIssueRequest>, JsonRejection>,
) -> Result<impl IntoResponse, AuthError> {
    Ok(Json(
        auth.privacy_pass_issue(
            bearer(&headers)?,
            peer_ip,
            request.map_err(|_| AuthError::Invalid)?.0,
        )
        .await?,
    ))
}
async fn privacy_pass_redeem(
    State(auth): State<Arc<AccountAuth>>,
    Extension(ClientIp(peer_ip)): Extension<ClientIp>,
    request: Result<Json<PrivacyPassRedeemRequest>, JsonRejection>,
) -> Result<impl IntoResponse, AuthError> {
    Ok(Json(
        auth.privacy_pass_redeem(peer_ip, request.map_err(|_| AuthError::Invalid)?.0)
            .await?,
    ))
}
async fn chat_pow_challenge(
    State(auth): State<Arc<AccountAuth>>,
    Extension(ClientIp(peer_ip)): Extension<ClientIp>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, AuthError> {
    Ok(Json(
        auth.chat_pow_challenge(bearer(&headers)?, peer_ip)
            .await?,
    ))
}
async fn chat_pow_verify(
    State(auth): State<Arc<AccountAuth>>,
    Extension(ClientIp(peer_ip)): Extension<ClientIp>,
    headers: HeaderMap,
    request: Result<Json<ChatProofOfWorkVerifyRequest>, JsonRejection>,
) -> Result<impl IntoResponse, AuthError> {
    Ok(Json(
        auth.chat_pow_verify(
            bearer(&headers)?,
            peer_ip,
            request.map_err(|_| AuthError::Invalid)?.0,
        )
        .await?,
    ))
}
async fn me(
    State(auth): State<Arc<AccountAuth>>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, AuthError> {
    Ok(Json(auth.authenticate(bearer(&headers)?).await?))
}
async fn organization_controls(
    State(auth): State<Arc<AccountAuth>>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, AuthError> {
    Ok(Json(auth.organization_controls(bearer(&headers)?).await?))
}
async fn set_organization_controls(
    State(auth): State<Arc<AccountAuth>>,
    headers: HeaderMap,
    request: Result<Json<OrganizationControlsRequest>, JsonRejection>,
) -> Result<impl IntoResponse, AuthError> {
    Ok(Json(
        auth.set_organization_controls(
            bearer(&headers)?,
            request.map_err(|_| AuthError::Invalid)?.0,
        )
        .await?,
    ))
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
async fn register_delegated_device(
    State(auth): State<Arc<AccountAuth>>,
    headers: HeaderMap,
    request: Result<Json<DelegatedDeviceRegistrationRequest>, JsonRejection>,
) -> Result<impl IntoResponse, AuthError> {
    Ok(Json(
        auth.register_delegated_device(
            bearer(&headers)?,
            request.map_err(|_| AuthError::Invalid)?.0,
        )
        .await?,
    ))
}
async fn revoke_device(
    State(auth): State<Arc<AccountAuth>>,
    Path(device_id): Path<uuid::Uuid>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, AuthError> {
    Ok(Json(
        auth.revoke_device(bearer(&headers)?, device_id).await?,
    ))
}
async fn create_group(
    State(auth): State<Arc<AccountAuth>>,
    headers: HeaderMap,
    request: Result<Json<CreateGroupRequest>, JsonRejection>,
) -> Result<impl IntoResponse, AuthError> {
    Ok(Json(
        auth.create_group(
            bearer(&headers)?,
            request.map_err(|_| AuthError::Invalid)?.0,
        )
        .await?,
    ))
}
async fn group_members(
    State(auth): State<Arc<AccountAuth>>,
    Path(group_id): Path<uuid::Uuid>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, AuthError> {
    Ok(Json(auth.group_members(bearer(&headers)?, group_id).await?))
}
async fn set_group_role(
    State(auth): State<Arc<AccountAuth>>,
    Path((group_id, user_id)): Path<(uuid::Uuid, uuid::Uuid)>,
    headers: HeaderMap,
    request: Result<Json<SetGroupRoleRequest>, JsonRejection>,
) -> Result<impl IntoResponse, AuthError> {
    auth.set_group_role(
        bearer(&headers)?,
        group_id,
        user_id,
        request.map_err(|_| AuthError::Invalid)?.0,
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}
async fn remove_group_member(
    State(auth): State<Arc<AccountAuth>>,
    Path((group_id, user_id)): Path<(uuid::Uuid, uuid::Uuid)>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, AuthError> {
    auth.remove_group_member(bearer(&headers)?, group_id, user_id)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
async fn delete_group(
    State(auth): State<Arc<AccountAuth>>,
    Path(group_id): Path<uuid::Uuid>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, AuthError> {
    auth.delete_group(bearer(&headers)?, group_id).await?;
    Ok(StatusCode::NO_CONTENT)
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

async fn upload_mls_key_package(
    State(auth): State<Arc<AccountAuth>>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<impl IntoResponse, AuthError> {
    auth.put_mls_key_package(bearer(&headers)?, body.to_vec())
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn download_mls_key_package(
    State(auth): State<Arc<AccountAuth>>,
    headers: HeaderMap,
    Path(device_id): Path<uuid::Uuid>,
) -> Result<Response, AuthError> {
    let package = auth
        .get_mls_key_package(bearer(&headers)?, device_id)
        .await?;
    let mut response = package.into_response();
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        "application/x-protobuf".parse().unwrap(),
    );
    Ok(response)
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
            Self::UsernameConflict => (StatusCode::CONFLICT, "username_exists"),
            Self::DeviceConflict => (StatusCode::CONFLICT, "device_already_registered"),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn untrusted_peer_cannot_spoof_forwarded_address() {
        let peer: IpAddr = "203.0.113.10".parse().unwrap();
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", "198.51.100.7".parse().unwrap());
        assert_eq!(
            client_ip_from_headers(peer, &HashSet::new(), &headers).unwrap(),
            peer
        );
    }

    #[test]
    fn trusted_proxy_requires_one_canonical_forwarded_address() {
        let proxy: IpAddr = "127.0.0.1".parse().unwrap();
        let trusted = HashSet::from([proxy]);
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", "198.51.100.7".parse().unwrap());
        assert_eq!(
            client_ip_from_headers(proxy, &trusted, &headers).unwrap(),
            "198.51.100.7".parse::<IpAddr>().unwrap()
        );
        headers.insert("x-forwarded-for", "198.51.100.7, 203.0.113.8".parse().unwrap());
        assert_eq!(
            client_ip_from_headers(proxy, &trusted, &headers),
            Err(StatusCode::BAD_REQUEST)
        );
    }
}
