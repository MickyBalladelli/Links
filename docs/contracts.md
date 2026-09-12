# Shared protocol and transport contracts

## Wire schema

`proto/links/v1` is the source of truth. Rust types and a file descriptor set are
generated at build time with vendored protoc. `links-protocol::v1` is the common
public API; `links-client-core::protocol` re-exports it. Do not fork models per
client. Use the `.proto` files when generating native-language bindings later.

All IDs are lowercase, hyphenated, non-nil UUIDs. Client-generated IDs must be
random and unpredictable; validators check shape, not randomness. All times are
UTC Unix milliseconds. Queue cursors are unsigned on the wire, bounded by signed
64-bit maximum for cross-store compatibility. JavaScript clients must use BigInt
or decimal strings, not floating-point Number, for uint64 values.

`Message` contains private sender/conversation metadata and one text, media, or
receipt body. `MediaMetadata` carries secret media keys and belongs **inside**
E2EE, never in a public upload request. `Receipts` communicates delivered/read
status to peers. `QueueAck` only means ciphertext/state was durably committed on
the receiving device; it is neither a read receipt nor proof of successful peer
decryption. `User` and `Device` are directory records, not authentication proofs.

`Envelope` contains version, envelope ID, recipient device, expiry and opaque
sealed bytes. No sender, conversation, phone number, or plaintext content field
is allowed. Core envelope adapters bind a domain-separated encoding of the outer
header as authenticated associated data. MLS-authenticated sender/conversation
identifiers must agree with the decrypted Message. Structural validation cannot
prove encryption: only the reviewed crypto provider may produce sealed bytes.

Enums reserve zero as unspecified. Unknown fields are tolerated by protobuf;
unknown versions, required semantic bodies and enum values are rejected by the
relevant validators. Generated Rust decoders do not retain unknown fields, so
forwarders must retain the original bytes rather than decode/re-encode messages
whose extensions they do not understand. New fields use new numbers; deleted
fields must reserve their number and name. Never change the wire type or reuse a
field number. Breaking behavior requires a new package/version and migration.
The receipt byte fixture is a compatibility smoke test, not a full interop suite.

## Decision: WebSocket first

Public bidirectional transport is **WebSocket over TLS (`wss`)**, not two parallel
client protocols. The endpoint contract is `/v1/connect`, subprotocol `links.v1`.
Each binary WebSocket message contains exactly one protobuf ClientFrame or
ServerFrame. Text frames and compression extensions are disabled. Require TLS
1.3 for public connections. The gateway itself is implemented in Phase 2.

HTTP/2 gRPC is reserved for future internal service RPCs; there is no public gRPC
client or HTTP/2 requirement for the initial WebSocket handshake. This avoids
assuming native bidirectional gRPC support in companion browsers.

The first frame must be Hello within 5 seconds. It contains protocol version,
device ID, a short-lived device-scoped access token, and the last durable cursor.
Do not put tokens into query strings or log them. The future gateway validates
the token against an active account/device, binds all replay/ack actions to that
device, validates browser Origin against an allowlist, and returns Welcome.
The request ID is a UUID used only for response correlation, not authentication
or durable message idempotency. Outbound envelopes may target another device;
inbound queue reads and acks may only target the authenticated local device.

Heartbeat interval is 30 seconds; close after 90 seconds without liveness. Native
clients may use ping/pong; browser clients rely on server-originated ping/pong.
Retry connections with exponential backoff and full jitter, initially 1 second,
capped at 30 seconds. A new session replaces the old route; renewal and disconnect
use compare-and-swap on the session ID so an old socket cannot clear the new one.
Session routing expires after at most 120 seconds without renewal. Reject or
backpressure sends when a dependency is unavailable; do not silently drop them.

Limits: 64 KiB encoded Message, 256 KiB encoded Envelope, 1 MiB complete transport
frame, at most 100 entries per sync batch. Batch producers must also fit a byte
budget of `MAX_FRAME_BYTES - 128` so wrapping a batch in ServerFrame fits the frame
cap. Enforce size limits **before** decoding or decompression/allocation. Receipt
batches contain at most 100 unique message IDs. Core decoders cover message and
envelope boundaries; the future gateway must validate authentication, transport
frames, rate limits and connection state before using generated types.

`Accepted` is returned only after durable enqueue and idempotency commit; it says
nothing about recipient delivery. Retries reuse the exact envelope ID and bytes.
Structured errors carry stable codes and optional retry delays, not identifiers,
SQL errors or exception text. Unsupported versions require a client update;
unauthenticated connections must re-authenticate, not retry indefinitely.

## Sync and durability

Every recipient device has an independent mailbox. Appending allocates a strictly
increasing positive cursor atomically with the record. No global/conversation
counter is exposed in the routing envelope. Fanout allocates a separate mailbox
entry per active recipient device; group/application ordering is authenticated
inside the encrypted protocol rather than inferred from mailbox order.

Replay starts strictly after the last durable checkpoint. A SyncBatch declares
the requested `after_cursor`, a snapshot high watermark, and the last included
cursor. Items must be contiguous. Purged or expired entries remain tombstones
for the replay window; absent data is never silently interpreted as delivered.
An empty batch is legal only when its cursor equals the high watermark. A page
may stop before that watermark due to the item or byte limit.

`SyncState::prepare` validates the batch without advancing state. The host first
persists received entries, MLS state, message-ID deduplication and the returned
checkpoint in one local transaction. Only after success does it call `commit`
and emit QueueAck. Crash before commit causes replay; crash after commit resumes
from the saved cursor. Do not ack messages that exist only in memory. Expired
entries can advance as tombstones; invalid/authentication-failed entries require
an explicit durable quarantine/error policy, never a silent skip.

Tombstones and idempotency fingerprints persist for 30 days from original
acceptance. After compaction, an old cursor yields `CURSOR_EXPIRED`; clients must
use an explicit authenticated resync/recovery flow. Server-only recovery cannot
restore ciphertext that has already been purged. Client-owned encrypted history
backup and resync UX are separate future work. Core rejects gaps rather than
inventing a history or moving the cursor to a high watermark.

## Provider and platform boundaries

`IdentityStore` exposes keystore references and signing, not secret key export.
`MlsEngine` owns authenticated group/epoch processing. `EnvelopeCrypto` owns the
sender-hiding wrapper and routing-header binding. Both unavailable implementations
return `CryptoUnavailable`; there is no production mock, homemade cipher, or
plaintext fallback. Providers must authenticate peer key discovery and credentials.

Provider state mutations are not automatically transactional. Mobile/Web hosts
must coordinate durable MLS state with inbox/outbox transactions before installing
a real provider. `seal_message` is a single-recipient orchestration boundary, not
an exactly-once send service or multi-device fanout implementation. Persist its
output and reuse it on retry. Adapter error and crash-injection tests must prove
state recovery before the security/delivery gates pass.

SecretBytes and KeyHandle zeroize their owned buffers and omit Debug. Generated
protobuf structs omit Debug, but remain cloneable and are not zeroizing storage;
the caller owns plaintext lifetime and must avoid unnecessary copies or logs.
Zeroization is not a promise that OS snapshots or every caller copy is erased.
