-- Control-plane metadata only. No plaintext messages, private keys, or phone numbers.
CREATE TABLE accounts (
    user_id UUID PRIMARY KEY CHECK (user_id <> '00000000-0000-0000-0000-000000000000'),
    auth_subject_hash BYTEA NOT NULL UNIQUE CHECK (octet_length(auth_subject_hash) = 32),
    account_kind TEXT NOT NULL DEFAULT 'consumer' CHECK (account_kind IN ('consumer', 'pseudonymous', 'organization')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    disabled_at TIMESTAMPTZ
);

CREATE TABLE handles (
    handle VARCHAR(32) COLLATE "C" PRIMARY KEY CHECK (handle ~ '^[a-z][a-z0-9_]{2,31}$'),
    user_id UUID NOT NULL UNIQUE REFERENCES accounts(user_id) ON DELETE CASCADE,
    claimed_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE devices (
    device_id UUID PRIMARY KEY CHECK (device_id <> '00000000-0000-0000-0000-000000000000'),
    user_id UUID NOT NULL REFERENCES accounts(user_id) ON DELETE CASCADE,
    mls_node_id UUID NOT NULL UNIQUE CHECK (mls_node_id <> '00000000-0000-0000-0000-000000000000'),
    identity_public_key BYTEA NOT NULL CHECK (octet_length(identity_public_key) = 32),
    mls_credential BYTEA NOT NULL CHECK (octet_length(mls_credential) BETWEEN 1 AND 65536),
    registered_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    revoked_at TIMESTAMPTZ CHECK (revoked_at >= registered_at),
    UNIQUE (user_id, device_id)
);
CREATE INDEX devices_active_user_idx ON devices(user_id) WHERE revoked_at IS NULL;

CREATE TABLE groups (
    group_id UUID PRIMARY KEY CHECK (group_id <> '00000000-0000-0000-0000-000000000000'),
    group_kind TEXT NOT NULL CHECK (group_kind IN ('direct', 'group', 'channel')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE TABLE group_memberships (
    group_id UUID NOT NULL REFERENCES groups(group_id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES accounts(user_id) ON DELETE CASCADE,
    role TEXT NOT NULL CHECK (role IN ('owner', 'admin', 'member')),
    joined_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (group_id, user_id)
);
CREATE INDEX memberships_user_idx ON group_memberships(user_id, group_id);

-- Deferred so group creation + initial owner insertion is a single transaction.
-- Repository writes lock the group row to serialize competing RBAC changes.
CREATE FUNCTION require_group_owner() RETURNS TRIGGER LANGUAGE plpgsql AS $$
DECLARE gid UUID;
BEGIN
    IF TG_OP = 'DELETE' THEN gid := OLD.group_id; ELSE gid := NEW.group_id; END IF;
    IF EXISTS (SELECT 1 FROM groups WHERE group_id = gid)
       AND NOT EXISTS (SELECT 1 FROM group_memberships WHERE group_id = gid AND role = 'owner') THEN
        RAISE EXCEPTION 'group requires an owner' USING ERRCODE = '23514';
    END IF;
    IF TG_OP = 'UPDATE' AND OLD.group_id <> NEW.group_id
       AND EXISTS (SELECT 1 FROM groups WHERE group_id = OLD.group_id)
       AND NOT EXISTS (SELECT 1 FROM group_memberships WHERE group_id = OLD.group_id AND role = 'owner') THEN
        RAISE EXCEPTION 'group requires an owner' USING ERRCODE = '23514';
    END IF;
    RETURN NULL;
END;
$$;
CREATE CONSTRAINT TRIGGER groups_require_owner AFTER INSERT OR UPDATE ON groups
    DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION require_group_owner();
CREATE CONSTRAINT TRIGGER memberships_require_owner AFTER INSERT OR UPDATE OR DELETE ON group_memberships
    DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION require_group_owner();
