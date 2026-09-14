# One-to-one chat proof of work

Links uses a small Hashcash-style proof for pseudonymous accounts. A phone-
verified account skips this step. The proof is an admission control measure,
not an identity credential.

## Flow

1. The authenticated pseudonymous client requests
   `GET /v1/chat-requests/proof-of-work/challenge`.
2. The server returns a random 32-byte challenge, expiry, and difficulty. The
   default is 18 leading zero bits; the accepted range is 12–24 bits.
3. A client background worker searches for a `u64` nonce whose
   `SHA-256("links/chat-request-pow/v1\\0" || challenge || nonce_be)` has the
   required leading zero bits.
4. The client sends the challenge and nonce to
   `POST /v1/chat-requests/proof-of-work/verify`. The server checks the account,
   device, expiry, difficulty, and digest, then atomically consumes the
   challenge before the new-chat host accepts the request.

Challenges live for five minutes. The server limits challenge issuance and
verification per account and source address. It stores no source address in
the challenge table: only a keyed challenge digest, account/device binding,
difficulty, expiry, and `issued`/`consumed` state are retained. Expired rows
are removed by the existing retention sweep.

Hosts must run solving off the UI thread and cancel it when the challenge
expires or the user abandons the connection. The server must keep the
challenge difficulty bounded and must never accept a client-supplied difficulty
instead of the stored value.
