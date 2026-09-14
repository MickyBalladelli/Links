//! Cryptographic verification badges.
//!
//! Badges are authority-signed public claims. An account or client must pin
//! the authority public key out of band; the badge signature alone does not
//! make an unknown key trusted.

use crate::{mls::MlsIdentitySigner, protocol, CoreError};
use uuid::Uuid;

pub const BADGE_KIND_PERSON: u32 = 1;
pub const BADGE_KIND_ORGANIZATION: u32 = 2;

/// Issue a badge with a hardware-backed authority signer.
pub fn issue_verification_badge<S: MlsIdentitySigner>(
    badge_id: Uuid,
    subject_user_id: Uuid,
    subject_handle: Option<String>,
    badge_kind: u32,
    issued_at_ms: u64,
    expires_at_ms: u64,
    authority: &S,
) -> Result<protocol::v1::VerificationBadge, CoreError> {
    let badge = protocol::v1::VerificationBadge {
        protocol_version: protocol::VERSION,
        badge_id: badge_id.to_string(),
        subject_user_id: subject_user_id.to_string(),
        subject_handle,
        issuer_public_key: authority.public_key()?.to_vec(),
        badge_kind,
        issued_at_ms,
        expires_at_ms,
        signature: vec![0; 64],
    };
    protocol::validate_verification_badge(&badge)?;
    let signature = authority.sign(
        &links_identity::verification_badge_transcript(&badge)
            .map_err(|_| CoreError::Authentication)?,
    )?;
    let mut signed = badge;
    signed.signature = signature.to_vec();
    protocol::validate_verification_badge(&signed)?;
    Ok(signed)
}

/// Verify a badge against a pinned verification-authority key.
pub fn verify_verification_badge(
    badge: &protocol::v1::VerificationBadge,
    authority_public_key: &[u8; 32],
    now_ms: u64,
) -> Result<u32, CoreError> {
    links_identity::verify_verification_badge(badge, authority_public_key, now_ms)
        .map_err(|_| CoreError::Authentication)?;
    Ok(badge.badge_kind)
}
