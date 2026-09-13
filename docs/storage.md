# Storage contracts

## PostgreSQL: implemented relational foundation

`links-server-store::postgres::RelationalStore` provides account creation, handle
claim/lookup, device-node enrollment/list/revocation, group creation, role grants,
and member removal. SQLx uses bound parameters. Migrations are embedded and run
explicitly; application startup must not implicitly migrate an arbitrary database.

`accounts` stores a unique keyed 32-byte authentication-subject lookup digest, not
a raw phone number, OTP, or unkeyed phone hash. Pseudonymous username accounts
leave that subject NULL and set `account_kind='pseudonymous'`; their first-party
device key proves registration and later login. The auth service owns the secret
digest key and any separately encrypted contact record. `handles` enforces
lowercase ASCII `[a-z][a-z0-9_]{2,31}` and one active handle per user. Concurrent
claims are arbitrated by unique constraints; changing/recycling a handle is not
part of this API. Disabled accounts cannot claim, enroll, or change membership.

`devices` maps an account/device to a unique MLS node and public credentials.
Revocation is idempotent and excludes the device from active routing queries;
a revoked device ID cannot be re-enrolled. Registration accepts a 32-byte public
identity key; it does not prove private-key possession. Enrollment authentication
and credential validation must precede the repository call in Phase 1. Device
removal also needs MLS rekeying and queue/session invalidation before release.

`groups` and `group_memberships` implement account-level RBAC independently of MLS
leaves. Owner may grant/remove any role. Admin may add/remove members but cannot
change admins or owners. Members may leave themselves but cannot grant roles.
The final owner cannot leave or be demoted. Direct groups allow at most two account
members; each account may still have multiple MLS device leaves. Repository
mutations serialize on the group row; deferred database constraints also reject
ownerless or over-sized direct groups. Direct SQL writers must use the same
group-lock discipline for concurrent membership changes.
Database credentials are privileged: this is not row-level tenant security. Only
the trusted account service may connect; API clients never receive DB credentials.
Actor IDs must come from authenticated service context, never user-supplied bodies.

Foreign keys prevent orphaned records, public handle/node uniqueness is global,
and active-device/membership lookups have indexes. There is no message-content
column. No account deletion/retention job or operational service has been built.
Owner accounts must transfer/delete their groups before erasure. Additive migrations
are preferred; restoring a backup is a controlled operational action, not an
automatic destructive down migration. Take and test backups before production
schema changes. Use TLS verification, least-privilege runtime credentials, a
separate migration role, and managed secrets outside local development.

## Encrypted payload store: PostgreSQL adapter implemented

`RelationalStore` implements `EncryptedPayloadStore` against the append-only
`encrypted_payloads` mailbox migration. AppendRequest checks version, UUIDs,
nonempty sealed bytes, 256 KiB envelope limit, and a positive TTL no longer than
30 days. ReadRequest validates device, cursor and page size. Authentication must
still scope every operation. Opaque bytes are a type boundary, not proof that a
client encrypted them correctly.

The partition key is recipient device ID; ordering key is mailbox cursor. The
idempotency key is `(recipient_device_id, envelope_id)`. Allocate a cursor and
commit its record/idempotency result atomically. Identical retries return the same
cursor; conflicting bytes return Conflict. Keep only a cryptographic fingerprint
for conflict detection after deleting ciphertext, never a hidden ciphertext copy.
New retries must not extend the original expiry. Do not reuse IDs beyond the
30-day idempotency window.

Read returns contiguous live records or expiry/ack tombstones. Limit both count
and encoded byte size. The authenticated recipient's QueueAck is the server-side
delivery confirmation: the client sends it only after decrypting and durably
committing the batch. Acknowledgements reject a cursor ahead of the mailbox high
watermark and purge payload bytes cumulatively. Delayed native TTL deletion must
never expose expired ciphertext through reads. Compact tombstones/fingerprints
after the replay window and report CursorExpired when a requested cursor is gone.
Backend cleanup/replica/backup behavior needs operational retention verification.

A future DynamoDB adapter must use conditional writes/transactions and consistent
reads; a ScyllaDB adapter must prove equivalent cursor/idempotency atomicity with
its chosen data model and consistency settings. Do not assume multi-table
atomicity or generic eventual consistency meets this contract. The PostgreSQL
deployment still needs encrypted-at-rest storage, private networking, backups,
replica recovery, and load/chaos validation.

## S3-compatible encrypted blobs: adapter and CDN contract implemented

`links-server-store::blob::S3CompatibleBlobStore` stores only
client-encrypted attachment bytes under immutable UUID keys. Conditional writes
make retries idempotent; a conflicting body is rejected. Upload receipts carry
the exact size and SHA-256 digest, and downloads verify both before returning
bytes to the client. Cloudflare R2 or private Amazon S3 may implement the
`S3CompatibleObjectClient` boundary.

Put the bucket behind authenticated short-lived signed URLs and Cloudflare or
CloudFront edge caching. The CDN may cache ciphertext for the 30-day retention
window, but must not transform, sniff, list, or receive bearer tokens. See
[blob-storage.md](blob-storage.md) for bucket policies, cache rules, lifecycle
expiration, and deployment steps.

Required adapter conformance cases are concurrent appends, identical/conflicting
retries, retry after ack, TTL boundary reads, byte-limited paging, cursor overflow,
ack beyond high watermark, expiry GC, stale-cursor recovery, and crash between
allocation and commit. The repository contains the PostgreSQL implementation;
production conformance and recovery validation still belong to deployment.

## Redis state: implemented adapter and local reference implementation

`EphemeralState` defines atomic bind, renew, unbind, route and token consumption.
`RedisEphemeralState` implements the production-shaped adapter with one key per
session or bucket, Redis `TIME`, Lua atomicity and server-side TTLs.
`MemoryEphemeralState` remains a bounded single-process reference adapter with a
mutex; it has no network connection and loses all data on restart.
Session values contain gateway locators, not socket objects or bearer tokens.
Only the gateway process owns a socket; the shared route identifies its owner.

One device has one current connection. A new session replaces its route, old
renew/disconnect operations compare session IDs, and expiration is enforced on
lookup. Callers must authenticate the device and use fresh unpredictable session
IDs; delayed binds must be fenced by the connection manager. Renewals cannot
resurrect an expired lease. The maximum lease lifetime is 120 seconds.

Rate buckets use integer fractional tokens, atomic consume, upward-rounded retry
delays, no refill on clock rollback, and expiration after enough idle time to fully
refill. Invalid or changed active policies are rejected. The memory adapter bounds
each map independently to the constructor's entry limit and prunes expired entries
on writes. Invalid zero costs/capacities/refill rates and over-capacity costs fail.
Keys must be namespaced opaque digests supplied by the service, not raw PII.

`RedisEphemeralState` uses one-key Lua scripts for the same CAS and token math,
Redis server time and TTLs. Its keys use per-device/per-bucket hash tags for
Redis Cluster. Backend errors are errors, not permission to bypass rate limits;
gateways fail closed for new sends and reconnect rather than assuming a missing
route means a device is permanently offline. The concrete Redis client must
implement `RedisScriptExecutor` with TLS, authentication, timeouts, connection
pooling and metrics that omit keys and values.
