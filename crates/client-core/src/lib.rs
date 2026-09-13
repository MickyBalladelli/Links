//! Platform-neutral client contracts and validated send/receive orchestration.
//! Ed25519 identity, PQXDH key agreement, and an OpenMLS TreeKEM provider are
//! available. The default outer envelope provider still fails closed; install
//! `crypto::SealedSenderCrypto` with a platform-backed key resolver for sends.
pub mod background;
pub mod crypto;
pub mod envelopes;
pub mod identity;
pub mod mls;
pub mod passkey_backup;
pub mod pqxdh;
pub mod prekeys;
pub mod sequences;
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
    #[error("invalid conversation sequence")]
    InvalidSequence,
    #[error("provider failed")]
    Provider,
}
