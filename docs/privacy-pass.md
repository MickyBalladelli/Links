# Privacy Pass new-chat admission

Links uses the Privacy Pass issuance model from [RFC 9578](https://www.rfc-editor.org/rfc/rfc9578.html) with the P-384/SHA-384 VOPRF profile from [RFC 9497](https://www.rfc-editor.org/rfc/rfc9497.html).

The client requests a short-lived challenge, creates a fresh nonce and blind
locally, and sends only a blinded P-384 point to the authenticated issuer.
The issuer applies account and source-address quotas, evaluates the blinded
point, and returns a DLEQ proof. The client verifies the proof and unblinds a
one-time token. It redeems that token later without bearer authentication when
starting a new chat.

The anonymous redemption table contains only:

- `token_hash`, the SHA-256 digest used as a one-time replay marker
- `expires_at_ms`, the challenge expiry used for cleanup

There is no account, device, conversation, chat-request, or foreign-key link.
The redemption endpoint also does not accept an identity field. Malformed,
altered, expired, wrong-key, and already-redeemed tokens fail closed.

Issuance quotas are account- and source-address-bound because issuance is an
authenticated operation. Anonymous redemption has only a source-address safety
bucket and the token-digest replay marker. Deployments must keep issuance and
redemption logs, analytics, and operational correlation separate; otherwise
the service could re-link the two contexts through metadata.

The gateway or new-chat admission service must call redemption before accepting
the new-chat request. A token is single-use and challenge-bound, so it must not
be copied into fan-out envelopes or reused for multiple admission decisions.
Client-side blind state is cleared when the issuance session ends.
