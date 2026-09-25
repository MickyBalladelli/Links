ALTER TABLE accounts
    ADD COLUMN display_name TEXT
    CHECK (display_name IS NULL OR octet_length(display_name) <= 80);

ALTER TABLE accounts
    ADD COLUMN display_name_set BOOLEAN NOT NULL DEFAULT FALSE;
