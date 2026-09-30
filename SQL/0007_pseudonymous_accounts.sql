-- Username-only accounts have no phone-derived authentication subject.
ALTER TABLE accounts
    ALTER COLUMN auth_subject_hash DROP NOT NULL;

ALTER TABLE accounts
    DROP CONSTRAINT accounts_auth_subject_hash_check;

ALTER TABLE accounts
    ADD CONSTRAINT accounts_auth_subject_hash_check
    CHECK (auth_subject_hash IS NULL OR octet_length(auth_subject_hash) = 32);
