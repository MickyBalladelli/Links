-- Device roots keep their original account authority. Delegated devices carry
-- a public issuer certificate and a narrower device/admin role.
ALTER TABLE devices
    ADD COLUMN delegation_role TEXT NOT NULL DEFAULT 'owner'
        CHECK (delegation_role IN ('owner', 'admin', 'device')),
    ADD COLUMN delegated_by_device_id UUID REFERENCES devices(device_id),
    ADD COLUMN delegation_certificate BYTEA
        CHECK (delegation_certificate IS NULL OR octet_length(delegation_certificate) BETWEEN 1 AND 65536);

ALTER TABLE devices
    ADD CONSTRAINT delegated_device_certificate_consistency CHECK (
        (delegation_role = 'owner' AND delegated_by_device_id IS NULL AND delegation_certificate IS NULL)
        OR (delegation_role IN ('admin', 'device') AND delegated_by_device_id IS NOT NULL AND delegation_certificate IS NOT NULL)
    );
