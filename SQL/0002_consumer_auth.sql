-- No raw phone numbers, OTP codes, private keys, or bearer tokens are persisted.
CREATE TABLE auth_challenges (
    challenge_id UUID PRIMARY KEY,
    subject_hash BYTEA NOT NULL CHECK (octet_length(subject_hash) = 32),
    user_id UUID NOT NULL,
    device_id UUID NOT NULL,
    mls_node_id UUID NOT NULL,
    public_key BYTEA NOT NULL CHECK (octet_length(public_key) = 32),
    nonce BYTEA NOT NULL CHECK (octet_length(nonce) = 32),
    enrolling BOOLEAN NOT NULL,
    permitted BOOLEAN NOT NULL,
    provider_sid TEXT,
    state TEXT NOT NULL CHECK (state IN ('reserved','pending','checking','consumed','failed')),
    attempts SMALLINT NOT NULL DEFAULT 0 CHECK (attempts BETWEEN 0 AND 5),
    expires_at_ms BIGINT NOT NULL CHECK (expires_at_ms > 0)
);
CREATE INDEX auth_challenges_expiry_idx ON auth_challenges(expires_at_ms);
-- Twilio can reuse a verification SID on resend. A provider approval can only
-- consume one local challenge, including after process restart or concurrent calls.
CREATE UNIQUE INDEX auth_provider_consumed_idx ON auth_challenges(provider_sid) WHERE state = 'consumed';
CREATE TABLE auth_rate_limits (
    key_hash BYTEA PRIMARY KEY CHECK (octet_length(key_hash) = 32),
    window_start_ms BIGINT NOT NULL,
    attempts INTEGER NOT NULL CHECK (attempts > 0)
);
CREATE TABLE auth_sessions (
    token_hash BYTEA PRIMARY KEY CHECK (octet_length(token_hash) = 32),
    user_id UUID NOT NULL,
    device_id UUID NOT NULL,
    expires_at_ms BIGINT NOT NULL,
    FOREIGN KEY (user_id, device_id) REFERENCES devices(user_id, device_id) ON DELETE CASCADE
);
CREATE INDEX auth_sessions_expiry_idx ON auth_sessions(expires_at_ms);
