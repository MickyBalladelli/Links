-- Attachment bytes are client-encrypted before they reach this service.
CREATE TABLE encrypted_attachments (
    attachment_id UUID PRIMARY KEY CHECK (attachment_id <> '00000000-0000-0000-0000-000000000000'),
    ciphertext BYTEA NOT NULL CHECK (octet_length(ciphertext) BETWEEN 17 AND 33554448),
    ciphertext_size_bytes BIGINT NOT NULL CHECK (ciphertext_size_bytes = octet_length(ciphertext)),
    ciphertext_sha256 BYTEA NOT NULL CHECK (octet_length(ciphertext_sha256) = 32),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX encrypted_attachments_expiry_idx ON encrypted_attachments(created_at);
