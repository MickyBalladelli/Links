-- Short-lived client proof-of-work challenges for pseudonymous accounts.
-- Store no source address and bind each challenge to one account/device.
CREATE TABLE proof_of_work_challenges (
    challenge_hash BYTEA PRIMARY KEY CHECK (octet_length(challenge_hash) = 32),
    user_id UUID NOT NULL,
    device_id UUID NOT NULL,
    difficulty_bits SMALLINT NOT NULL CHECK (difficulty_bits BETWEEN 12 AND 24),
    expires_at_ms BIGINT NOT NULL CHECK (expires_at_ms > 0),
    state TEXT NOT NULL CHECK (state IN ('issued', 'consumed')),
    FOREIGN KEY (user_id, device_id) REFERENCES devices(user_id, device_id) ON DELETE CASCADE
);
CREATE INDEX proof_of_work_challenges_expiry_idx
    ON proof_of_work_challenges(expires_at_ms);
