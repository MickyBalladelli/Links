//! Hardware-friendly signed device sub-certificates.
//!
//! An existing account device signs a new device key and MLS node. The
//! private issuer key stays behind MlsIdentitySigner; only this public
//! certificate crosses the registration and directory boundaries.

use crate::{mls::MlsIdentitySigner, protocol, CoreError};
use uuid::Uuid;

pub const ROLE_DEVICE: u32 = links_identity::DEVICE_SUBCERTIFICATE_ROLE_DEVICE;
pub const ROLE_ADMIN: u32 = links_identity::DEVICE_SUBCERTIFICATE_ROLE_ADMIN;

/// Build a certificate and sign it with the approving device identity.
pub fn issue_device_subcertificate<S: MlsIdentitySigner>(
    user_id: Uuid,
    issuer_device_id: Uuid,
    issuer_mls_node_id: Uuid,
    subject_device_id: Uuid,
    subject_mls_node_id: Uuid,
    subject_public_key: [u8; 32],
    delegation_role: u32,
    issued_at_ms: u64,
    expires_at_ms: u64,
    signer: &S,
) -> Result<protocol::v1::DeviceSubCertificate, CoreError> {
    let certificate = protocol::v1::DeviceSubCertificate {
        protocol_version: protocol::VERSION,
        user_id: user_id.to_string(),
        issuer_device_id: issuer_device_id.to_string(),
        issuer_mls_node_id: issuer_mls_node_id.to_string(),
        issuer_public_key: signer.public_key()?.to_vec(),
        subject_device_id: subject_device_id.to_string(),
        subject_mls_node_id: subject_mls_node_id.to_string(),
        subject_public_key: subject_public_key.to_vec(),
        delegation_role,
        issued_at_ms,
        expires_at_ms,
        signature: vec![0; 64],
    };
    protocol::validate_device_subcertificate(&certificate)?;
    let signature = signer.sign(
        &links_identity::device_subcertificate_transcript(&certificate)
            .map_err(|_| CoreError::Authentication)?,
    )?;
    let mut signed = certificate;
    signed.signature = signature.to_vec();
    protocol::validate_device_subcertificate(&signed)?;
    Ok(signed)
}

/// Verify the issuer signature and validity window before trusting a
/// delegated device. Server account and role policy is checked separately.
pub fn verify_device_subcertificate(
    certificate: &protocol::v1::DeviceSubCertificate,
    now_ms: u64,
) -> Result<u32, CoreError> {
    links_identity::verify_device_subcertificate(certificate, now_ms)
        .map_err(|_| CoreError::Authentication)?;
    Ok(certificate.delegation_role)
}
