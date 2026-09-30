-- Public pre-key material only. Private key bytes never leave a device.
CREATE TABLE device_prekey_profiles (
    device_id UUID PRIMARY KEY REFERENCES devices(device_id) ON DELETE CASCADE,
    profile_revision BIGINT NOT NULL CHECK (profile_revision > 0),
    identity_dh_key BYTEA NOT NULL CHECK (octet_length(identity_dh_key) = 32),
    identity_binding_signature BYTEA NOT NULL CHECK (octet_length(identity_binding_signature) = 64),
    signed_curve_prekey_id BIGINT NOT NULL CHECK (signed_curve_prekey_id > 0),
    signed_curve_prekey BYTEA NOT NULL CHECK (octet_length(signed_curve_prekey) = 32),
    signed_curve_signature BYTEA NOT NULL CHECK (octet_length(signed_curve_signature) = 64),
    last_resort_kem_prekey_id BIGINT NOT NULL CHECK (last_resort_kem_prekey_id > 0),
    last_resort_kem_prekey BYTEA NOT NULL CHECK (octet_length(last_resort_kem_prekey) = 1184),
    last_resort_kem_signature BYTEA NOT NULL CHECK (octet_length(last_resort_kem_signature) = 64),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Retained for the active profile revision so a delayed retry cannot recreate
-- a one-time key that a recipient already claimed.
CREATE TABLE device_prekey_uploads (
    device_id UUID NOT NULL REFERENCES device_prekey_profiles(device_id) ON DELETE CASCADE,
    upload_id UUID NOT NULL,
    profile_revision BIGINT NOT NULL CHECK (profile_revision > 0),
    upload_digest BYTEA NOT NULL CHECK (octet_length(upload_digest) = 32),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (device_id, upload_id)
);

CREATE TABLE device_curve_one_time_prekeys (
    device_id UUID NOT NULL REFERENCES device_prekey_profiles(device_id) ON DELETE CASCADE,
    prekey_id BIGINT NOT NULL CHECK (prekey_id > 0),
    public_key BYTEA NOT NULL CHECK (octet_length(public_key) = 32),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (device_id, prekey_id)
);

CREATE TABLE device_kem_one_time_prekeys (
    device_id UUID NOT NULL REFERENCES device_prekey_profiles(device_id) ON DELETE CASCADE,
    prekey_id BIGINT NOT NULL CHECK (prekey_id > 0),
    public_key BYTEA NOT NULL CHECK (octet_length(public_key) = 1184),
    signature BYTEA NOT NULL CHECK (octet_length(signature) = 64),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (device_id, prekey_id)
);
