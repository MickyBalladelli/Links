//! Local address-book phone hashing for a future private contact-discovery
//! protocol. This module never reads contacts or sends hashes to a server.

use argon2::{Algorithm, Argon2, Params, Version};
use getrandom::fill;
use thiserror::Error;
use zeroize::Zeroize;

pub const CONTACT_SALT_BYTES: usize = 16;
pub const CONTACT_HASH_BYTES: usize = 32;
pub const MAX_CONTACTS_PER_BATCH: usize = 10_000;

const ARGON2_MEMORY_KIB: u32 = 32 * 1024;
const ARGON2_ITERATIONS: u32 = 3;
const ARGON2_LANES: u32 = 1;
const PHONE_CONTEXT: &[u8] = b"links/contact-discovery/v1\0";

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ContactHashError {
    #[error("phone must be canonical E.164")]
    InvalidPhone,
    #[error("contact salt must be exactly 16 bytes")]
    InvalidSalt,
    #[error("contact batch is too large")]
    TooManyContacts,
    #[error("contact hash provider unavailable")]
    Provider,
}

/// Random salt persisted by one client for its local contact set.
///
/// The salt is not a phone number, account identifier, or server secret. A
/// client must keep it stable while comparing successive contact snapshots;
/// rotating it intentionally invalidates all old hashes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ContactHashSalt([u8; CONTACT_SALT_BYTES]);

impl ContactHashSalt {
    pub fn generate() -> Result<Self, ContactHashError> {
        let mut bytes = [0u8; CONTACT_SALT_BYTES];
        fill(&mut bytes).map_err(|_| ContactHashError::Provider)?;
        Ok(Self(bytes))
    }

    pub fn from_bytes(bytes: [u8; CONTACT_SALT_BYTES]) -> Self {
        Self(bytes)
    }

    pub fn from_slice(bytes: &[u8]) -> Result<Self, ContactHashError> {
        Ok(Self(
            bytes
                .try_into()
                .map_err(|_| ContactHashError::InvalidSalt)?,
        ))
    }

    pub fn as_bytes(&self) -> &[u8; CONTACT_SALT_BYTES] {
        &self.0
    }
}

/// Argon2id output for one canonical E.164 phone number.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContactPhoneHash([u8; CONTACT_HASH_BYTES]);

impl ContactPhoneHash {
    pub fn as_bytes(&self) -> &[u8; CONTACT_HASH_BYTES] {
        &self.0
    }

    pub fn into_bytes(self) -> [u8; CONTACT_HASH_BYTES] {
        self.0
    }
}

/// Hash one already-normalized address-book number locally.
///
/// Callers must normalize using an explicit user-selected region before this
/// function. No default country is guessed, and formatting variants are not
/// silently merged. The server never receives the raw phone input here.
pub fn hash_phone(
    phone_e164: &str,
    salt: &ContactHashSalt,
) -> Result<ContactPhoneHash, ContactHashError> {
    validate_e164(phone_e164)?;
    let mut input = Vec::with_capacity(PHONE_CONTEXT.len() + phone_e164.len());
    input.extend_from_slice(PHONE_CONTEXT);
    input.extend_from_slice(phone_e164.as_bytes());
    let params = Params::new(
        ARGON2_MEMORY_KIB,
        ARGON2_ITERATIONS,
        ARGON2_LANES,
        Some(CONTACT_HASH_BYTES),
    )
    .map_err(|_| ContactHashError::Provider)?;
    let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut output = [0u8; CONTACT_HASH_BYTES];
    let result = argon
        .hash_password_into(&input, salt.as_bytes(), &mut output)
        .map_err(|_| ContactHashError::Provider);
    input.zeroize();
    result?;
    Ok(ContactPhoneHash(output))
}

/// Hash a bounded address-book snapshot using one stable client salt.
pub fn hash_phones<I, S>(
    phones_e164: I,
    salt: &ContactHashSalt,
) -> Result<Vec<ContactPhoneHash>, ContactHashError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let phones = phones_e164.into_iter();
    let (lower, upper) = phones.size_hint();
    if lower > MAX_CONTACTS_PER_BATCH || upper.is_some_and(|count| count > MAX_CONTACTS_PER_BATCH) {
        return Err(ContactHashError::TooManyContacts);
    }
    let mut hashes = Vec::with_capacity(lower);
    for phone in phones {
        if hashes.len() == MAX_CONTACTS_PER_BATCH {
            return Err(ContactHashError::TooManyContacts);
        }
        hashes.push(hash_phone(phone.as_ref(), salt)?);
    }
    Ok(hashes)
}

fn validate_e164(phone: &str) -> Result<(), ContactHashError> {
    let digits = phone
        .strip_prefix('+')
        .ok_or(ContactHashError::InvalidPhone)?;
    if !(8..=15).contains(&digits.len())
        || digits.starts_with('0')
        || !digits.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(ContactHashError::InvalidPhone);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_phone_and_salt_are_stable() {
        let salt = ContactHashSalt::from_bytes([7u8; CONTACT_SALT_BYTES]);
        assert_eq!(
            hash_phone("+12025550123", &salt),
            hash_phone("+12025550123", &salt)
        );
    }

    #[test]
    fn different_salts_do_not_match() {
        let first = ContactHashSalt::from_bytes([7u8; CONTACT_SALT_BYTES]);
        let second = ContactHashSalt::from_bytes([8u8; CONTACT_SALT_BYTES]);
        assert_ne!(
            hash_phone("+12025550123", &first),
            hash_phone("+12025550123", &second)
        );
    }

    #[test]
    fn formatting_and_non_e164_numbers_are_rejected() {
        let salt = ContactHashSalt::from_bytes([7u8; CONTACT_SALT_BYTES]);
        assert_eq!(
            hash_phone("+1 202 555 0123", &salt),
            Err(ContactHashError::InvalidPhone)
        );
        assert_eq!(
            hash_phone("2025550123", &salt),
            Err(ContactHashError::InvalidPhone)
        );
    }
}
