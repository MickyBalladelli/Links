-- Passkey public credentials and opaque client-side encrypted identity backups.
-- The server never receives WebAuthn PRF output or a plaintext private key.
CREATE TABLE passkey_credentials (
    user_id UUID NOT NULL REFERENCES accounts(user_id) ON DELETE CASCADE,
    credential_id BYTEA NOT NULL CHECK (octet_length(credential_id) BETWEEN 1 AND 1024),
    public_key BYTEA NOT NULL CHECK (octet_length(public_key) = 64),
    sign_count BIGINT NOT NULL DEFAULT 0 CHECK (sign_count >= 0),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_used_at TIMESTAMPTZ,
    PRIMARY KEY (user_id, credential_id),
    UNIQUE (credential_id)
);

CREATE TABLE passkey_challenges (
    challenge_id UUID PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES accounts(user_id) ON DELETE CASCADE,
    purpose TEXT NOT NULL CHECK (purpose IN ('registration', 'assertion')),
    challenge BYTEA NOT NULL CHECK (octet_length(challenge) = 32),
    state TEXT NOT NULL CHECK (state IN ('pending', 'consumed', 'failed')),
    expires_at_ms BIGINT NOT NULL CHECK (expires_at_ms > 0)
);
CREATE INDEX passkey_challenges_expiry_idx ON passkey_challenges(expires_at_ms);

CREATE TABLE encrypted_key_backups (
    backup_id UUID PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES accounts(user_id) ON DELETE CASCADE,
    device_id UUID NOT NULL,
    credential_id BYTEA NOT NULL CHECK (octet_length(credential_id) BETWEEN 1 AND 1024),
    encrypted_envelope BYTEA NOT NULL CHECK (octet_length(encrypted_envelope) BETWEEN 128 AND 8192),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (user_id, device_id),
    FOREIGN KEY (user_id, device_id) REFERENCES devices(user_id, device_id) ON DELETE CASCADE,
    FOREIGN KEY (user_id, credential_id) REFERENCES passkey_credentials(user_id, credential_id) ON DELETE RESTRICT
);
CREATE INDEX encrypted_key_backups_user_idx ON encrypted_key_backups(user_id, updated_at DESC);
