-- Direct conversations have two account-level members. Device fan-out remains
-- an MLS concern: one account can still contribute several device leaves.
CREATE FUNCTION require_direct_group_member_limit() RETURNS TRIGGER LANGUAGE plpgsql AS $$
DECLARE
    gid UUID;
    v_group_kind TEXT;
    member_count BIGINT;
BEGIN
    gid := NEW.group_id;
    SELECT group_kind INTO v_group_kind FROM groups WHERE group_id = gid;
    IF v_group_kind = 'direct' THEN
        SELECT count(*) INTO member_count FROM group_memberships WHERE group_id = gid;
        IF member_count > 2 THEN
            RAISE EXCEPTION 'direct group requires at most two users' USING ERRCODE = '23514';
        END IF;
    END IF;
    RETURN NULL;
END;
$$;

CREATE CONSTRAINT TRIGGER direct_groups_max_two_members
AFTER INSERT OR UPDATE ON group_memberships
DEFERRABLE INITIALLY DEFERRED FOR EACH ROW
EXECUTE FUNCTION require_direct_group_member_limit();
