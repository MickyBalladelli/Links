//! Platform-neutral client contracts and validated send/receive orchestration.
//! No cryptographic implementation is supplied by Phase 0. Providers fail closed.
pub mod crypto;
pub mod envelopes;
pub mod identity;
pub mod mls;
pub mod sync;
pub use links_protocol as protocol;

use thiserror::Error;
#[derive(Debug, Error, PartialEq, Eq)]
pub enum CoreError {
    #[error(transparent)]
    Protocol(#[from] protocol::ProtocolError),
    #[error("cryptographic provider unavailable")]
    CryptoUnavailable,
    #[error("authentication failed")]
    Authentication,
    #[error("invalid sync batch or checkpoint")]
    InvalidSync,
    #[error("provider failed")]
    Provider,
}
