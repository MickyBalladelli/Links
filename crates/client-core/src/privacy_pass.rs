//! Anonymous Privacy Pass issuance state for new-chat admission.
//!
//! The client generates the nonce and blind locally. The issuer sees only the
//! blinded P-384 point, while the resulting token is redeemed separately from
//! the authenticated issuance session.

use getrandom::fill;
use links_protocol::privacy_pass::{self, IssuerParameters, TokenRequest, TokenResponse};
use zeroize::Zeroize;

pub struct PrivacyPassClientState {
    state: privacy_pass::BlindState,
}

impl Drop for PrivacyPassClientState {
    fn drop(&mut self) {
        self.state.nonce.zeroize();
        self.state.challenge_digest.zeroize();
        self.state.token_key_id.zeroize();
        self.state.token_input.zeroize();
        self.state.blind.zeroize();
        self.state.blinded_message.zeroize();
    }
}

impl PrivacyPassClientState {
    /// Create a one-time blinded token request for an origin challenge.
    pub fn start(
        challenge: &[u8; privacy_pass::CHALLENGE_BYTES],
        parameters: &IssuerParameters,
    ) -> Result<(Self, TokenRequest), crate::CoreError> {
        let mut nonce = [0u8; privacy_pass::NONCE_BYTES];
        fill(&mut nonce).map_err(|_| crate::CoreError::Provider)?;
        let mut blind_bytes = [0u8; privacy_pass::SCALAR_BYTES];
        fill(&mut blind_bytes).map_err(|_| crate::CoreError::Provider)?;
        let result = privacy_pass::blind(challenge, &nonce, parameters, &blind_bytes);
        blind_bytes.zeroize();
        let (state, request) = result?;
        Ok((Self { state }, request))
    }

    /// Verify the issuer's blind signature and finalize the anonymous token.
    pub fn finish(
        &self,
        parameters: &IssuerParameters,
        response: &TokenResponse,
    ) -> Result<privacy_pass::PrivacyPassToken, crate::CoreError> {
        Ok(privacy_pass::finalize(&self.state, parameters, response)?)
    }

    pub fn blinded_message(&self) -> &[u8; privacy_pass::POINT_BYTES] {
        &self.state.blinded_message
    }
}
