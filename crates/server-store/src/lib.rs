//! Server-side metadata only. This crate must never depend on links-client-core.
pub mod ephemeral;
pub mod payload;
pub mod postgres;

use thiserror::Error;
#[derive(Debug, Error)]
pub enum StoreError {
    #[error("invalid storage argument")]
    Invalid,
    #[error("conflicting write")]
    Conflict,
    #[error("resource not found")]
    NotFound,
    #[error("operation forbidden")]
    Forbidden,
    #[error("cursor no longer available; resync required")]
    CursorExpired,
    #[error("storage backend unavailable")]
    Unavailable,
    #[error(transparent)]
    Protocol(#[from] links_protocol::ProtocolError),
    #[error("database operation failed")]
    Database(#[source] sqlx::Error),
    #[error("database migration failed")]
    Migration(#[source] sqlx::migrate::MigrateError),
}
impl From<sqlx::Error> for StoreError {
    fn from(error: sqlx::Error) -> Self {
        if let sqlx::Error::Database(db) = &error {
            match db.code().as_deref() {
                Some("23505") => return Self::Conflict,
                Some("23503" | "23514" | "23502" | "22P02") => return Self::Invalid,
                _ => {}
            }
        }
        Self::Database(error)
    }
}
