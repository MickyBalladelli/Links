# Phase 0: product scope and security baseline

Status: implementation baseline, not a releasable encrypted messenger.

## Product decision

**Scenario A: Centralized Architecture** is the v1 target. A single operator runs
account registration, the public-key directory, delivery gateways, mailbox queues,
and storage. Federation, blockchain identity, P2P routing, and decentralized media
are outside v1. The first usable slice is phone-based accounts, device identity,
and reliable one-to-one text on Android. iOS follows; Web and desktop are paired
companion devices. Media, groups, channels, and calls follow the text milestones.

The Phase 0 implementation chooses a Rust workspace: `links-protocol` generates
shared protobuf types, `links-client-core` owns platform-independent contracts and
validation, and `links-server-store` owns server storage boundaries. The core has
no socket, database, UI, or platform-keystore dependency. Mobile bindings and Web
bindings belong to their client phases; compiling the Rust core for WASM is not a
completed browser client. No unaudited encryption or pass-through crypto ships.

## Threat model

Assets are message and media plaintext, secret identity/session keys, group state,
account access, the integrity of device membership, and durable delivery state.
Adversaries include network observers, a compromised delivery/storage operator,
malicious directory responses, abusive accounts, stolen devices, replaying peers,
and a future adversary retaining today's ciphertext for later analysis.

Trust is limited to uncompromised endpoints, authenticated client distribution,
reviewed cryptographic providers, the platform random generator/keystore, and
explicitly verified peer credentials. TLS protects the network hop, not against
the operator. A malicious directory can substitute keys on first contact unless
clients verify them; authenticated enrollment, device-change warnings, and peer
verification are Phase 1 release blockers. Server RBAC is not MLS authorization:
clients must verify the corresponding signed membership/epoch changes.

| Threat | Required control | Boundary / current state |
| --- | --- | --- |
| Network or stored-payload disclosure | TLS plus client-side authenticated encryption | TLS gateway and real crypto are later phases; only opaque payload contracts exist now. |
| Forgery, replay, or stale membership | Verified MLS credentials, transcript/epoch validation, durable replay state | Provider contract exists; real engine and adversarial vectors required in Phase 1. |
| Compromised directory | Authenticated enrollment, peer verification and device-change handling | Not solved by OTP or protobuf public keys alone. |
| Stolen device | OS-keystore custody, revoke device, rotate affected MLS groups | SQL revocation exists; hardware custody and MLS removal are not implemented. |
| Queue loss or duplicates | Per-device monotonic cursors, idempotency, transactional checkpointing | Sync validator exists; durable distributed mailbox is Phase 2. |
| Abuse or routing hijack | Atomic rate buckets, bounded leases, authenticated session ownership | Reference state adapter exists; gateway authentication and distributed Redis are Phase 2. |
| Harvest-now/decrypt-later | Reviewed post-quantum session and group design | A design goal, not a security property of Phase 0. |

Endpoint compromise, screenshots/exports by recipients, a global traffic observer,
coercion, malicious client builds, and denial of service by the operator are not
prevented by E2EE. Forward secrecy and post-compromise security depend on the
chosen engine, secret deletion, and subsequent authenticated updates. They are
not implied by the interface names.

## Metadata privacy budget

| Data | Operator visibility | Retention goal |
| --- | --- | --- |
| Text, receipt contents, filenames, media keys, thumbnails | Forbidden outside E2EE payloads | No server plaintext copy. |
| Private identity keys and MLS secrets | Forbidden | Remain client-side; no export-to-server fallback. |
| Account lookup digest, public handle, device keys/nodes | Visible to account/directory services | Account lifetime; account erasure policy required before release. |
| Group identifier, membership, and role | Visible in PostgreSQL when groups are enabled | Group lifetime; this leaks a social graph. |
| Recipient device, ciphertext size, arrival/expiry, queue cursor | Visible to delivery/storage | Ciphertext: purge on durable queue acknowledgement or expiry, at most 30 days from acceptance. |
| IP address, authenticated connection, timing and push token | Visible to gateways/providers | Do not persist raw traffic logs by default; raw security diagnostics disabled in production. |
| Session-to-gateway route | Visible in ephemeral state | At most 120 seconds without renewal. |
| Rate-limit key | Opaque namespaced digest only | Expire after a full-refill idle interval. |

Sender identity and conversation identity are absent from the outer envelope.
This reduces stored routing metadata but does **not** hide the sender from an
authenticated gateway or defeat timing correlation. Sealed Sender integration is
not implemented in Phase 0. Operational metrics should aggregate counts without
account, device, conversation, IP, or payload labels. No frame/body SQL-parameter
logging is permitted. Backup, replica, and log deletion must meet the same payload
retention ceiling; provider TTL alone does not prove timely physical erasure.
Account/directory retention needs a separate erasure procedure before launch.

## Identity and cryptography guardrails

Generate identity material from a CSPRNG. Phone numbers and OTPs identify an
account but are not high-entropy key seeds. Each physical device has a unique
identity and MLS node. A one-to-one chat has two users, not necessarily two MLS
leaves after multi-device pairing. Device membership changes require verified MLS
commits as well as server metadata changes.

Do not invent a PQXDH/MLS composition. [RFC 9420](https://www.rfc-editor.org/rfc/rfc9420.html)
defines MLS authentication and epoch-based group security; its specified suites
are not a blanket post-quantum guarantee. A reviewed protocol composition, concrete
cipher suites, interop vectors, and external cryptographic review are required
before advertising post-quantum resilience. Also verify native hardware algorithm
support and key migration constraints before implementing the roadmap's recovery
and hardware-keystore proposals. A passkey is not automatically a backup of an
arbitrary private key. These are Phase 1 decisions, not Phase 0 implementations.

## Platform plan

| Target | Order and baseline | Gate |
| --- | --- | --- |
| Shared Rust core | First; native host and `wasm32-unknown-unknown` compile checks | No platform dependencies; provider failure is closed. |
| Android | Primary client; proposed API 28+ | Real-device keystore, reconnect, background delivery, battery tests. |
| iOS | Second primary client; proposed iOS 16+ | Real-device key custody, APNs/background limits, parity tests. |
| Web | Paired companion; current and previous major evergreen browsers | WASM binding, secure pairing, browser-storage and XSS threat review. |
| Desktop | Paired companion; macOS, Windows, Linux | OS-keystore adapters, signed builds, update security, parity tests. |

OS floors above are initial product baselines, not promises of implemented support.
Bindings and target-specific CI must be added before each client release.

## Release gates

| Gate | Evidence required to pass |
| --- | --- |
| P0: contracts and core | Protobuf compilation/compatibility fixture; validated envelope and sync APIs; fail-closed provider tests; PostgreSQL migrations and RBAC/concurrency tests; ephemeral-state contract tests; native tests/lint and WASM compile. |
| P1: security foundation | Real reviewed identity/MLS/envelope providers; credentials verified end to end; authenticated device enrollment/removal; replay/tamper and recovery vectors; protocol composition review. No production use before this gate. |
| Android internal text | Two physical devices exchange offline/online text without duplicate rendering or acknowledged-message loss; restart/reconnect and recovery tests; keystore and push behavior measured. |
| iOS internal text | Android interop plus the same durability tests; hardware custody and iOS background behavior measured on devices. |
| Companion clients | Explicit mobile pairing, per-device queues, credential revocation, cross-device sync and restart tests. |
| Media/groups/calls | Size and memory limits; authenticated group updates; media metadata removal; encrypted upload/call paths and abuse tests. |
| Public centralized release | Independent crypto/security audit with critical findings resolved; dependency and incident review; account/payload erasure verification; documented recovery limitations; battery, load, and availability results. |

The roadmap's sub-50ms / one-million-socket target is a benchmark to validate,
not a present capability or a launch claim. Decentralized benchmarks do not gate
centralized v1. Completion of P0 means the foundation is implemented and tested;
it does not mark any Phase 1 or Phase 2 delivery/security feature complete.
