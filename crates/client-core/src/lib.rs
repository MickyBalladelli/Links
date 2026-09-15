//! Platform-neutral client contracts and validated send/receive orchestration.
//! Ed25519 identity, PQXDH key agreement, and an OpenMLS TreeKEM provider are
//! available. The default outer envelope provider still fails closed; install
//! `crypto::SealedSenderCrypto` with a platform-backed key resolver for sends.
pub mod attachments;
pub mod badges;
pub mod background;
pub mod broadcast;
pub mod contact_discovery;
pub mod content_addressed;
pub mod decentralized;
pub mod crypto;
pub mod delegation;
pub mod envelopes;
pub mod identity;
pub mod images;
pub mod mls;
pub mod p2p_transfer;
pub mod pairing;
pub mod passkey_backup;
pub mod pqxdh;
pub mod prekeys;
pub mod privacy_pass;
pub mod proof_of_work;
pub mod receive;
#[cfg(not(target_arch = "wasm32"))]
pub mod sandbox;
pub mod send;
pub mod sequences;
pub mod sframe;
pub mod sync;
pub mod surfaces;
pub mod video;
pub mod voice;
pub mod webrtc;
pub use links_protocol as protocol;

use thiserror::Error;
#[derive(Debug, Error, PartialEq, Eq)]
pub enum CoreError {
    #[error(transparent)]
    Protocol(#[from] protocol::ProtocolError),
    #[error(transparent)]
    Voice(#[from] voice::VoiceError),
    #[error(transparent)]
    Image(#[from] images::ImageError),
    #[error(transparent)]
    Video(#[from] video::VideoError),
    #[error(transparent)]
    P2pTransfer(#[from] p2p_transfer::P2pTransferError),
    #[error(transparent)]
    PrivacyPass(#[from] protocol::privacy_pass::PrivacyPassError),
    #[error(transparent)]
    ProofOfWork(#[from] protocol::proof_of_work::ProofOfWorkError),
    #[error("cryptographic provider unavailable")]
    CryptoUnavailable,
    #[error("authentication failed")]
    Authentication,
    #[error("another active session uses this device")]
    SessionConflict,
    #[error("invalid sync batch or checkpoint")]
    InvalidSync,
    #[error("invalid conversation sequence")]
    InvalidSequence,
    #[error(transparent)]
    SFrame(#[from] sframe::SFrameError),
    #[error("provider failed")]
    Provider,
    #[error(transparent)]
    Decentralized(#[from] decentralized::DecentralizedError),
}
