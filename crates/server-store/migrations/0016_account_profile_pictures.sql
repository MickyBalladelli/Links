-- Public profile picture for a username account. Contacts read it from the
-- directory. Private identity material never enters this table.
CREATE TABLE account_profile_pictures (
    user_id UUID PRIMARY KEY REFERENCES accounts(user_id) ON DELETE CASCADE,
    jpeg BYTEA NOT NULL CHECK (octet_length(jpeg) BETWEEN 3 AND 131072),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
