use crate::AuthError;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use p256::{
    ecdsa::{Signature, VerifyingKey},
    EncodedPoint, FieldBytes,
};
use serde::Deserialize;
use serde_cbor::Value;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, io::Cursor};

pub const PASSKEY_CHALLENGE_BYTES: usize = 32;

#[derive(Clone)]
pub struct PasskeyConfig {
    pub rp_id: String,
    pub origin: String,
}

impl PasskeyConfig {
    pub fn new(rp_id: String, origin: String) -> Result<Self, AuthError> {
        if rp_id.is_empty()
            || rp_id.len() > 253
            || !valid_rp_id(&rp_id)
            || origin.is_empty()
            || origin.len() > 512
            || origin.contains('\n')
            || origin.contains('\r')
        {
            return Err(AuthError::Invalid);
        }
        let origin_host = origin_host(&origin).ok_or(AuthError::Invalid)?;
        if origin_host != rp_id && !origin_host.ends_with(&format!(".{rp_id}")) {
            return Err(AuthError::Invalid);
        }
        Ok(Self { rp_id, origin })
    }
}

pub struct RegisteredPasskey {
    pub credential_id: Vec<u8>,
    pub public_key: [u8; 64],
    pub sign_count: u32,
}

pub fn verify_registration(
    config: &PasskeyConfig,
    challenge: &[u8; PASSKEY_CHALLENGE_BYTES],
    expected_credential_id: &[u8],
    client_data_json: &[u8],
    attestation_object: &[u8],
) -> Result<RegisteredPasskey, AuthError> {
    verify_client_data(config, challenge, client_data_json, "webauthn.create")?;
    let attestation = decode_map(attestation_object)?;
    let fmt = map_text(&attestation, "fmt")?;
    if fmt != "none" {
        return Err(AuthError::Invalid);
    }
    let att_stmt =
        map_value(&attestation, &Value::Text("attStmt".into())).ok_or(AuthError::Invalid)?;
    if !matches!(att_stmt, Value::Map(map) if map.is_empty()) {
        return Err(AuthError::Invalid);
    }
    let auth_data = map_bytes(&attestation, "authData")?;
    if auth_data.len() < 55 || auth_data[..32] != Sha256::digest(config.rp_id.as_bytes())[..] {
        return Err(AuthError::Denied);
    }
    let flags = auth_data[32];
    if flags & 0x01 == 0 || flags & 0x04 == 0 || flags & 0x40 == 0 {
        return Err(AuthError::Denied);
    }
    let sign_count = u32::from_be_bytes(auth_data[33..37].try_into().unwrap());
    let mut offset = 37 + 16;
    let credential_len = u16::from_be_bytes(
        auth_data[offset..offset + 2]
            .try_into()
            .map_err(|_| AuthError::Invalid)?,
    ) as usize;
    offset += 2;
    if !(1..=1024).contains(&credential_len) || offset + credential_len >= auth_data.len() {
        return Err(AuthError::Invalid);
    }
    let credential_id = auth_data[offset..offset + credential_len].to_vec();
    if credential_id != expected_credential_id {
        return Err(AuthError::Denied);
    }
    offset += credential_len;
    let cose_key = decode_one(&auth_data[offset..])?;
    let public_key = cose_p256_key(&cose_key)?;
    Ok(RegisteredPasskey {
        credential_id,
        public_key,
        sign_count,
    })
}

pub fn verify_assertion(
    config: &PasskeyConfig,
    challenge: &[u8; PASSKEY_CHALLENGE_BYTES],
    client_data_json: &[u8],
    authenticator_data: &[u8],
    signature: &[u8],
    public_key: &[u8; 64],
    previous_sign_count: u32,
) -> Result<u32, AuthError> {
    verify_client_data(config, challenge, client_data_json, "webauthn.get")?;
    if authenticator_data.len() < 37
        || authenticator_data[..32] != Sha256::digest(config.rp_id.as_bytes())[..]
    {
        return Err(AuthError::Denied);
    }
    let flags = authenticator_data[32];
    if flags & 0x01 == 0 || flags & 0x04 == 0 {
        return Err(AuthError::Denied);
    }
    let sign_count = u32::from_be_bytes(authenticator_data[33..37].try_into().unwrap());
    if previous_sign_count != 0 && sign_count <= previous_sign_count {
        return Err(AuthError::Denied);
    }
    let mut signed = Vec::with_capacity(authenticator_data.len() + 32);
    signed.extend_from_slice(authenticator_data);
    signed.extend_from_slice(&Sha256::digest(client_data_json));
    let key = p256_key(public_key)?;
    let signature = Signature::from_der(signature).map_err(|_| AuthError::Denied)?;
    p256::ecdsa::signature::Verifier::verify(&key, &signed, &signature)
        .map_err(|_| AuthError::Denied)?;
    Ok(sign_count)
}

fn verify_client_data(
    config: &PasskeyConfig,
    challenge: &[u8; PASSKEY_CHALLENGE_BYTES],
    client_data_json: &[u8],
    expected_type: &str,
) -> Result<(), AuthError> {
    let data: ClientData =
        serde_json::from_slice(client_data_json).map_err(|_| AuthError::Invalid)?;
    if data.kind != expected_type || data.origin != config.origin {
        return Err(AuthError::Denied);
    }
    let received = URL_SAFE_NO_PAD
        .decode(data.challenge)
        .map_err(|_| AuthError::Denied)?;
    if received.as_slice() != challenge {
        return Err(AuthError::Denied);
    }
    Ok(())
}

#[derive(Deserialize)]
struct ClientData {
    #[serde(rename = "type")]
    kind: String,
    challenge: String,
    origin: String,
}

fn decode_map(bytes: &[u8]) -> Result<BTreeMap<Value, Value>, AuthError> {
    match serde_cbor::from_slice(bytes).map_err(|_| AuthError::Invalid)? {
        Value::Map(map) => Ok(map),
        _ => Err(AuthError::Invalid),
    }
}

fn decode_one(bytes: &[u8]) -> Result<Value, AuthError> {
    let mut cursor = Cursor::new(bytes);
    let value = serde_cbor::from_reader(&mut cursor).map_err(|_| AuthError::Invalid)?;
    if cursor.position() == 0 {
        return Err(AuthError::Invalid);
    }
    Ok(value)
}

fn map_value<'a>(map: &'a BTreeMap<Value, Value>, key: &Value) -> Option<&'a Value> {
    map.get(key)
}

fn map_text(map: &BTreeMap<Value, Value>, key: &str) -> Result<String, AuthError> {
    match map_value(map, &Value::Text(key.into())) {
        Some(Value::Text(value)) => Ok(value.clone()),
        _ => Err(AuthError::Invalid),
    }
}

fn map_bytes(map: &BTreeMap<Value, Value>, key: &str) -> Result<Vec<u8>, AuthError> {
    match map_value(map, &Value::Text(key.into())) {
        Some(Value::Bytes(value)) => Ok(value.clone()),
        _ => Err(AuthError::Invalid),
    }
}

fn integer(map: &BTreeMap<Value, Value>, key: i128) -> Result<i128, AuthError> {
    match map_value(map, &Value::Integer(key)) {
        Some(Value::Integer(value)) => Ok(*value),
        _ => Err(AuthError::Invalid),
    }
}

fn cose_bytes(map: &BTreeMap<Value, Value>, key: i128) -> Result<Vec<u8>, AuthError> {
    match map_value(map, &Value::Integer(key)) {
        Some(Value::Bytes(value)) => Ok(value.clone()),
        _ => Err(AuthError::Invalid),
    }
}

fn cose_p256_key(value: &Value) -> Result<[u8; 64], AuthError> {
    let Value::Map(map) = value else {
        return Err(AuthError::Invalid);
    };
    if integer(map, 1)? != 2 || integer(map, 3)? != -7 || integer(map, -1)? != 1 {
        return Err(AuthError::Invalid);
    }
    let x = cose_bytes(map, -2)?;
    let y = cose_bytes(map, -3)?;
    if x.len() != 32 || y.len() != 32 {
        return Err(AuthError::Invalid);
    }
    let mut public_key = [0u8; 64];
    public_key[..32].copy_from_slice(&x);
    public_key[32..].copy_from_slice(&y);
    p256_key(&public_key)?;
    Ok(public_key)
}

fn p256_key(public_key: &[u8; 64]) -> Result<VerifyingKey, AuthError> {
    let point = EncodedPoint::from_affine_coordinates(
        FieldBytes::from_slice(&public_key[..32]),
        FieldBytes::from_slice(&public_key[32..]),
        false,
    );
    VerifyingKey::from_encoded_point(&point).map_err(|_| AuthError::Denied)
}

fn valid_rp_id(value: &str) -> bool {
    if value == "localhost" {
        return true;
    }
    value.split('.').all(|label| {
        !label.is_empty()
            && label.len() <= 63
            && label
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
            && !label.starts_with('-')
            && !label.ends_with('-')
    })
}

fn origin_host(origin: &str) -> Option<&str> {
    let authority = if let Some(value) = origin.strip_prefix("https://") {
        value
    } else if let Some(value) = origin.strip_prefix("http://localhost") {
        if !value.is_empty() && !value.starts_with(':') {
            return None;
        }
        return Some("localhost");
    } else {
        return None;
    };
    let authority = authority.split('/').next()?;
    if authority.is_empty()
        || authority.contains('@')
        || authority.contains('?')
        || authority.contains('#')
    {
        return None;
    }
    Some(authority.split(':').next()?)
}
