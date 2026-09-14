-- Public, authority-signed badge only. Verification evidence and authority
-- private keys stay outside the account database.
ALTER TABLE accounts
    ADD COLUMN verification_badge BYTEA
        CHECK (verification_badge IS NULL OR octet_length(verification_badge) BETWEEN 1 AND 65536);
