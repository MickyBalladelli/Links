//! Phone verification and proof-of-possession device enrollment.
//! Never log request bodies, provider responses, phone digests or session tokens.
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
    #[error("authentication rate limit exceeded")]
    RateLimited,
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
