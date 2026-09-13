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

`hash_phones` bounds one snapshot at 10,000 numbers. `ContactPhoneHash` raw
bytes remain local and are never uploaded.

## Network PSI

The authenticated contact-discovery API uses a verifiable OPRF over Ristretto:

1. The client creates a fresh random blind for every canonical phone and sends
   only the blinded points to `POST /v1/contact-discovery/query`.
2. The server evaluates each point with its stable directory key and returns a
   DLEQ proof that all evaluations use the advertised public key.
3. The client verifies every proof, unblinds the evaluations, derives opaque
   32-byte OPRF tokens, and checks them locally against the membership filter
   returned by `GET /v1/contact-discovery/parameters`.

Phone accounts store only those opaque OPRF directory tokens. Username-only
accounts have no phone token. Existing phone accounts created before this
feature are populated after their next successful OTP flow because the old
authentication digest cannot be converted into an OPRF token.

The server learns the authenticated account, source address, batch size, query
count, and timing. It does not receive raw phone numbers, local Argon2id hashes,
or unblinded OPRF values. The client receives an opaque pseudorandom membership
filter rather than the server's phone numbers. Rate limits cap requests at 10 batches per
minute and 60 per hour per account, plus 100 per hour per source address; each
batch has at most 256 contacts. This is a practical one-sided PSI boundary,
not a promise of metadata anonymity or a substitute for an external
cryptographic audit.
