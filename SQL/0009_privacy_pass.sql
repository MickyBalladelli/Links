-- Privacy Pass redemption state is deliberately identity-free.
-- The token hash is only a one-time replay marker and is not linked to an account.
CREATE TABLE privacy_pass_redeemed (
    token_hash BYTEA PRIMARY KEY CHECK (octet_length(token_hash) = 32),
    expires_at_ms BIGINT NOT NULL CHECK (expires_at_ms > 0)
);
CREATE INDEX privacy_pass_redeemed_expiry_idx
    ON privacy_pass_redeemed(expires_at_ms);
