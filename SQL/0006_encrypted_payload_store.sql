-- Ciphertext-only mailbox. Keep rows as tombstones after payload deletion so
-- cursor continuity and envelope-id idempotency survive the replay window.
CREATE TABLE encrypted_payload_cursors (
    recipient_device_id UUID PRIMARY KEY CHECK (recipient_device_id <> '00000000-0000-0000-0000-000000000000'),
    high_watermark BIGINT NOT NULL DEFAULT 0 CHECK (high_watermark >= 0)
);

CREATE TABLE encrypted_payloads (
    recipient_device_id UUID NOT NULL CHECK (recipient_device_id <> '00000000-0000-0000-0000-000000000000'),
    cursor BIGINT NOT NULL CHECK (cursor > 0),
    envelope_id UUID NOT NULL CHECK (envelope_id <> '00000000-0000-0000-0000-000000000000'),
    envelope_bytes BYTEA,
    envelope_fingerprint BYTEA NOT NULL CHECK (octet_length(envelope_fingerprint) = 32),
    accepted_at_ms BIGINT NOT NULL CHECK (accepted_at_ms >= 0),
    expires_at_ms BIGINT NOT NULL CHECK (expires_at_ms > accepted_at_ms),
    state TEXT NOT NULL CHECK (state IN ('live', 'acknowledged', 'expired')),
    PRIMARY KEY (recipient_device_id, cursor),
    UNIQUE (recipient_device_id, envelope_id),
    CHECK (envelope_bytes IS NULL OR octet_length(envelope_bytes) BETWEEN 1 AND 262144),
    CHECK ((state = 'live' AND envelope_bytes IS NOT NULL) OR (state <> 'live' AND envelope_bytes IS NULL))
);
CREATE INDEX encrypted_payloads_replay_idx
    ON encrypted_payloads(recipient_device_id, cursor);
CREATE INDEX encrypted_payloads_expiry_idx
    ON encrypted_payloads(expires_at_ms, recipient_device_id, cursor)
    WHERE state = 'live';
