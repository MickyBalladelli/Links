-- Organization feature controls. These flags gate client exposure only;
-- Mini-App capability grants and bot credentials stay in their own boundaries.
CREATE TABLE organization_controls (
    organization_id UUID PRIMARY KEY REFERENCES accounts(user_id) ON DELETE CASCADE,
    mini_apps_enabled BOOLEAN NOT NULL DEFAULT FALSE,
    bots_enabled BOOLEAN NOT NULL DEFAULT FALSE,
    revision BIGINT NOT NULL DEFAULT 1 CHECK (revision > 0),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

INSERT INTO organization_controls (organization_id)
SELECT user_id FROM accounts WHERE account_kind = 'organization'
ON CONFLICT (organization_id) DO NOTHING;
