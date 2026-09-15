-- The current public MLS KeyPackage for each active device.
-- KeyPackages are public bootstrap material; private OpenMLS state never enters
-- this table.
CREATE TABLE device_mls_key_packages (
    device_id UUID PRIMARY KEY REFERENCES devices(device_id) ON DELETE CASCADE,
    key_package BYTEA NOT NULL CHECK (octet_length(key_package) BETWEEN 1 AND 1048576),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
