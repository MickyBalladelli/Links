# Cryptographic verification badges

`VerificationBadge` is a public, authority-signed claim for an account. It
binds a badge ID, subject user, optional canonical handle, badge kind, issuer
public key, and bounded validity window. The signature covers every claim with
the domain `links/verification-badge/v1`.

Clients must pin the verification-authority public key through deployment
configuration or a trust-store update. A valid Ed25519 signature from an
unknown key is not enough. `links-client-core::badges::verify_verification_badge`
checks the pinned key, signature, and expiry before showing a badge.

The account service exposes only the public badge in username directory
responses. A trusted verification workflow calls
`AccountAuth::issue_verification_badge()` with an HSM-backed
`VerificationSigner`; evidence and authority private keys never enter the
database. `revoke_verification_badge()` removes the current claim, and expiry
also makes a cached badge invalid.
