-- Contact PSI stores only opaque OPRF directory tokens, never phone numbers.
ALTER TABLE accounts
    ADD COLUMN contact_directory_token BYTEA
    CHECK (contact_directory_token IS NULL OR octet_length(contact_directory_token) = 32);

CREATE UNIQUE INDEX accounts_contact_directory_token_idx
    ON accounts(contact_directory_token)
    WHERE contact_directory_token IS NOT NULL;

ALTER TABLE auth_challenges
    ADD COLUMN contact_directory_token BYTEA
    CHECK (contact_directory_token IS NULL OR octet_length(contact_directory_token) = 32);
