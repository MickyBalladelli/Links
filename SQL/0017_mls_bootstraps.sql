-- Latest direct-chat MLS welcome for a device. The ciphertext mailbox cannot be
-- decrypted until this welcome is delivered, including when the recipient was
-- offline at send time.
CREATE TABLE device_mls_bootstraps (
    recipient_device_id UUID NOT NULL REFERENCES devices(device_id) ON DELETE CASCADE,
    conversation_id UUID NOT NULL,
    bootstrap BYTEA NOT NULL CHECK (octet_length(bootstrap) BETWEEN 1 AND 2097152),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (recipient_device_id, conversation_id)
);
