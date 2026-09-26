//! Phone verification and proof-of-possession device enrollment.
//! Never log request bodies, provider responses, phone digests or session tokens.
pub mod passkeys;
pub mod provider;
pub mod service;
pub mod web;

use thiserror::Error;
#[derive(Debug, Error)]
pub enum AuthError {
    #[error("invalid authentication request")]
    Invalid,
    #[error("authentication failed")]
    Denied,
    #[error("resource not found")]
    NotFound,
    #[error("authentication rate limit exceeded")]
    RateLimited,
    #[error("conflicting authentication write")]
    Conflict,
    #[error("username already exists")]
    UsernameConflict,
    #[error("device already has a username")]
    DeviceConflict,
    #[error("authentication temporarily unavailable")]
    Unavailable,
}
impl From<sqlx::Error> for AuthError {
    fn from(_: sqlx::Error) -> Self {
        Self::Unavailable
    }
}
impl From<links_identity::IdentityError> for AuthError {
    fn from(_: links_identity::IdentityError) -> Self {
        Self::Denied
    }
}
impl From<links_server_store::StoreError> for AuthError {
    fn from(error: links_server_store::StoreError) -> Self {
        match error {
            links_server_store::StoreError::Invalid
            | links_server_store::StoreError::Protocol(_) => Self::Invalid,
            links_server_store::StoreError::Conflict => Self::Conflict,
            links_server_store::StoreError::Forbidden
            | links_server_store::StoreError::NotFound => Self::Denied,
            _ => Self::Unavailable,
        }
    }
}
