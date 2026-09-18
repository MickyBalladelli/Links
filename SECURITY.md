# Security policy and remediation plan

Links is under active development and has not completed an independent security audit. Do not treat the current repository as production-ready for sensitive communications.

## Reporting a vulnerability

Do not open a public issue for a vulnerability that could expose accounts, identity keys, authentication tokens, message content, or infrastructure credentials. Report it privately to the project maintainers and include:

- the affected component and revision;
- reproduction steps or a minimal proof of concept;
- expected and observed behavior;
- likely impact and any known mitigations.

Do not access another person's account or data, disrupt shared infrastructure, or retain sensitive data while testing.

## Supported versions

Only the current default branch receives security fixes. No released version is currently supported for production use.

## Open authentication findings

### AUTH-001 — Replayable username authentication proof

**Severity:** High  
**Affected flows:** Username login; username registration uses the same client-generated freshness model.  
**Affected code:** `crates/account-auth/src/service.rs`, `crates/identity/src/lib.rs`, and native username-auth clients.

Username authentication signs a client-generated nonce. The server does not issue that nonce, persist it, or reject reuse. A captured valid login request can therefore be replayed to mint new bearer sessions. Current rate limits reduce request volume but do not prevent an attacker from maintaining access.

#### Required remediation

- [ ] Add a PostgreSQL migration for one-time username authentication challenges. Store a random server challenge, challenge ID, purpose (`registration` or `login`), intended handle/device binding, expiry, state, and attempt metadata. Do not store private key material.
- [ ] Add a challenge-start endpoint for username registration and login. Generate at least 32 random bytes with the existing CSPRNG and use a short expiry.
- [ ] Define versioned registration and login transcripts that bind the operation, challenge ID, server nonce, expiry, canonical handle, device ID, MLS node ID, and public key.
- [ ] Update `links-identity` and every FFI/native client implementation to construct the new transcript exactly.
- [ ] Require the challenge to be pending, unexpired, and bound to the submitted operation and identity before signature verification succeeds.
- [ ] Consume the challenge in the same database transaction that creates the account or session.
- [ ] Serialize concurrent finishes with a row lock or conditional state update so one challenge can issue at most one session.
- [ ] Mark ambiguous, failed, or interrupted challenge attempts unusable where replay safety cannot be guaranteed.
- [ ] Reject legacy nonce-only login requests after updated clients are deployed. Do not leave a compatibility path that preserves replayability.
- [ ] Add cleanup for expired username challenges without removing replay markers before their safety window ends.

#### Acceptance criteria

- Replaying an identical successful login request never creates a second session.
- Concurrent submissions of one challenge produce exactly one successful session.
- A registration challenge cannot be used for login or vice versa.
- A challenge for one handle, device, key, or node cannot authenticate another.
- Expired, consumed, superseded, and malformed challenges fail closed.
- Restarting the service does not make a consumed challenge reusable.

### AUTH-002 — Unauthenticated requests can lock out a username

**Severity:** Medium  
**Affected flow:** Username login and registration.  
**Affected code:** `AccountAuth::enforce_username_rate_limits` and its callers.

The per-handle buckets are incremented before the submitted device and signature are authenticated. An attacker can submit syntactically valid requests for a public handle and exhaust its minute or hourly allowance without possessing the account key.

#### Required remediation

- [ ] Remove unauthenticated per-handle lockout from username login.
- [ ] Apply source-oriented limits before expensive parsing, database access, and signature verification.
- [ ] Apply account/device limits only after a server challenge and valid device signature prove possession. Successful proof should not create an attacker-controlled lockout condition.
- [ ] Give username registration separate abuse controls from returning-device login; do not share a bucket that lets registration traffic block login.
- [ ] Add bounded exponential backoff or risk-based controls without creating a durable denial-of-service primitive against a named account.
- [ ] Return consistent authentication failures so invalid device IDs, signatures, and handles do not reveal private account state beyond the intentionally public directory.
- [ ] Document exact limits and operational override procedures.

#### Acceptance criteria

- Requests without a valid registered-device signature cannot exhaust a target account's authenticated login allowance.
- Flooding one handle does not prevent its owner from completing a valid challenge.
- Invalid requests remain bounded by source and global abuse controls.

### AUTH-003 — Loopback proxy address collapses IP limits into a global bucket

**Severity:** Medium  
**Affected flows:** OTP start, username authentication, directory lookup, contact discovery, Privacy Pass, and proof of work.  
**Affected code:** `crates/account-auth/src/main.rs`, `crates/account-auth/src/web.rs`, and rate-limit callers in `service.rs`.

The service must bind to loopback behind a TLS terminator, while `ConnectInfo` identifies the direct socket peer. In the expected deployment that peer is the local proxy, so all users share one IP bucket. A small number of requests can exhaust a shared limit and block authentication for every user routed through that proxy.

#### Required remediation

- [ ] Define the production client-IP attribution contract between the TLS proxy and account-auth service.
- [ ] Prefer enforcing source-IP, connection, bot, geographical, provider-spend, and global limits at the trusted ingress.
- [ ] If the application consumes `Forwarded` or `X-Forwarded-For`, enable it only when the socket peer matches an explicit trusted-proxy allowlist.
- [ ] Configure the proxy to replace untrusted forwarding headers rather than append to attacker-supplied values.
- [ ] Parse forwarded addresses strictly, reject malformed or ambiguous chains, and never fall back to an attacker-controlled value.
- [ ] Keep service-side global and identifier-specific safeguards that do not depend on client IP.
- [ ] Add startup validation or deployment health checks that detect production mode without the required trusted ingress controls.
- [ ] Add operational alerts for sudden rate-limit saturation and OTP provider spend anomalies.

#### Acceptance criteria

- Two clients behind the same proxy receive independent source-IP limits.
- A direct client cannot spoof its address with forwarding headers.
- Requests from an untrusted proxy or malformed forwarding chain fail safely.
- Exhausting one source bucket does not exhaust the service-wide authentication allowance.
- A separate global ceiling still limits distributed abuse and provider cost.

### AUTH-004 — Sign-out does not revoke the server session

**Severity:** Medium  
**Affected flows:** All bearer-authenticated operations.  
**Affected code:** account-auth routing/session storage and iOS `signOut`/`clearAuthenticatedSession`.

Client sign-out clears the in-memory bearer but leaves its database session valid until the 15-minute expiry. A copied bearer therefore remains usable after the user signs out.

#### Required remediation

- [ ] Add `POST /v1/auth/logout` authenticated by the current bearer.
- [ ] Hash the supplied token exactly as `authenticate` does and delete only that session in one database operation.
- [ ] Make logout idempotent and return no token or sensitive session metadata.
- [ ] Add an authenticated “revoke other sessions” operation for account recovery and suspected compromise, while preserving or explicitly revoking the current session according to the API contract.
- [ ] Update Apple, Android, web, and desktop clients to call server logout before clearing local session state.
- [ ] Clear local authentication state even when the network request fails, while communicating that remote revocation could not be confirmed.
- [ ] Ensure device revocation and account disablement continue to invalidate all associated sessions immediately.
- [ ] Avoid logging authorization headers or token hashes during logout.

#### Acceptance criteria

- A bearer rejected after logout cannot access `/v1/auth/me` or any other authenticated endpoint.
- Repeating logout is safe and does not disclose whether a token previously existed.
- A failed network logout never leaves the client UI authenticated.
- Revoking a device invalidates every session for that device.

## Required authentication test coverage

The PostgreSQL integration suite currently exercises the OTP flow extensively but lacks equivalent username and passkey coverage.

- [ ] Add username registration tests for valid creation, duplicate handles, duplicate device/node IDs, malformed keys, invalid signatures, atomic rollback, and disabled accounts.
- [ ] Add username login tests for valid login, wrong handle/device/node/key, revoked devices, disabled accounts, session expiry, and service restart.
- [ ] Add the replay, concurrent-consumption, cross-purpose, cross-account, expiry, and supersession tests required by AUTH-001.
- [ ] Add targeted-lockout and source-limit tests required by AUTH-002 and AUTH-003.
- [ ] Add HTTP tests for body limits, uniform error bodies, `Cache-Control: no-store`, authorization parsing, and absence of secrets in errors.
- [ ] Add logout, repeated logout, expired-token logout, device revocation, and revoke-other-sessions tests.
- [ ] Add passkey ceremony tests for origin/RP mismatch, user-presence and user-verification flags, challenge replay, credential substitution, signature failure, counter rollback, concurrent assertions, disabled accounts, and revoked devices.
- [ ] Add client tests confirming that login challenges are validated before signing and that session/account/device fields in responses match the request.
- [ ] Add client tests confirming that sign-out clears local state when remote revocation is unavailable.

## Release gates

The following work is required before production release:

- [ ] Complete AUTH-001 through AUTH-004 and their acceptance tests.
- [ ] Run the full Rust, PostgreSQL, Apple, Android, web, and desktop authentication test matrix in CI.
- [ ] Test OTP delivery and fraud controls against a dedicated non-production provider account.
- [ ] Validate TLS termination, trusted-proxy handling, request-body logging policy, redaction, rate limits, provider spend caps, and security alerting in a production-like environment.
- [ ] Perform physical-device tests for identity creation, restart, device lock, key invalidation, restoration, and sign-out.
- [ ] Commission an independent review of the authentication protocol, native key custody, session lifecycle, account recovery, and deployment configuration.
- [ ] Resolve all critical and high findings before release; document explicit risk acceptance for any remaining medium finding.

## Existing controls to preserve

Remediation must retain these properties:

- Session tokens contain 32 random bytes and are stored only as hashes.
- Every authenticated request checks expiry, account disablement, and device revocation.
- OTP challenges are attempt-limited, expire, and are consumed transactionally.
- Account, device, MLS credential, session, and challenge state commit atomically where required.
- Authentication request bodies are bounded and responses use `Cache-Control: no-store`.
- Raw phone numbers, OTP codes, identity seeds, private keys, and bearer tokens are not persisted or logged.
- Production account-auth remains loopback-only behind correctly configured TLS termination.
- Development authentication bypasses remain unavailable in release builds.
