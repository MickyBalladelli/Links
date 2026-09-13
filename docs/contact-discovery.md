# Local contact hashing

`links-client-core::contact_discovery` hashes address-book phone numbers before
any future contact-discovery protocol. It does not read contacts, retain raw
numbers, or send hashes to the server.

The client must normalize each number to canonical E.164 using an explicit
user-selected region before hashing. The core accepts `+` followed by 8–15
ASCII digits and never guesses a country or silently removes formatting.

`ContactHashSalt::generate` creates a random 16-byte salt. The client stores
that salt in its local protected state and reuses it for the life of the local
contact-hash set. Rotating the salt intentionally changes every result.

`hash_phone` prepends the domain `links/contact-discovery/v1` and derives a
32-byte result with Argon2id v1.3 using:

| Parameter | Value |
| --- | --- |
| Memory | 32 MiB |
| Iterations | 3 |
| Lanes | 1 |
| Output | 32 bytes |
| Salt | Random 16-byte client salt |

`hash_phones` bounds one snapshot at 10,000 numbers. `ContactPhoneHash` exposes
raw bytes for the future PSI boundary, but the present implementation does not
upload them. Hashing reduces accidental plaintext exposure and cross-client
precomputation; it is not PSI or an enumeration-proof protocol. The next TODO
item must define the server/network zero-knowledge matching flow before any
contact hashes leave the device.
