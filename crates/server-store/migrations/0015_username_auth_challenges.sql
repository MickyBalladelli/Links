-- One-time server challenges prevent replay of signed username authentication requests.
CREATE TABLE username_auth_challenges (
    challenge_id UUID PRIMARY KEY,
    purpose TEXT NOT NULL CHECK (purpose IN ('registration', 'login')),
    handle VARCHAR(32) COLLATE "C" NOT NULL CHECK (handle ~ '^[a-z][a-z0-9_]{2,31}$'),
    device_id UUID NOT NULL CHECK (device_id <> '00000000-0000-0000-0000-000000000000'),
    mls_node_id UUID NOT NULL CHECK (mls_node_id <> '00000000-0000-0000-0000-000000000000'),
    public_key BYTEA NOT NULL CHECK (octet_length(public_key) = 32),
    challenge BYTEA NOT NULL CHECK (octet_length(challenge) = 32),
    state TEXT NOT NULL CHECK (state IN ('pending', 'consumed', 'failed')),
    attempts SMALLINT NOT NULL DEFAULT 0 CHECK (attempts BETWEEN 0 AND 3),
    expires_at_ms BIGINT NOT NULL CHECK (expires_at_ms > 0),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX username_auth_challenges_expiry_idx
    ON username_auth_challenges(expires_at_ms);
CREATE INDEX username_auth_challenges_binding_idx
    ON username_auth_challenges(handle, device_id, purpose, state);
