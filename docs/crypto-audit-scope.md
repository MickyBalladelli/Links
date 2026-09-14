# Third-party cryptographic audit scope

Status: audit package prepared. External auditor and signed report are still
required. This document must not be treated as an audit result.

## Objective

Independently review the Links cryptographic composition for confidentiality,
authenticity, forward secrecy, post-compromise recovery, replay resistance,
key separation, nonce handling, downgrade resistance, secret lifetime, and
failure behavior. Review the implementation and its host boundaries, not only
the API names.

## In-scope code

| Area | Code | Questions the auditor must answer |
| --- | --- | --- |
| PQXDH | `crates/client-core/src/pqxdh.rs`, `docs/pqxdh.md` | Are the X25519 DH terms, ML-KEM-768 encapsulation, transcript bindings, HKDF domain separation, one-time-key handling, implicit rejection, and initial-payload authentication composed safely? |
| MLS / TreeKEM | `crates/client-core/src/mls.rs`, `crates/client-core/src/send.rs`, `crates/client-core/src/receive.rs`, `docs/mls.md` | Does OpenMLS own all ratchet operations correctly? Are credential checks, membership changes, epoch checkpoints, pending commits, welcomes, removals, and direct/group invariants safe under replay and concurrency? |
| Envelope protection | `crates/client-core/src/crypto.rs`, `crates/client-core/src/envelopes.rs`, `docs/pqxdh.md` | Are Sealed Sender routing headers authenticated without exposing sender metadata? Are recipient-key selection, ephemeral X25519, AEAD nonce/AAD, size limits, and key zeroization correct? |
| SFrame | `crates/client-core/src/sframe.rs`, `web/src/WebRtcSFrame.ts`, `native/apple/Sources/LinksClient/IOSCallSession.swift`, `native/android/client/src/main/java/ai/links/app/AndroidCallSession.java`, `docs/sframe.md` | Are frame transforms, cipher-suite selection, epoch/key-ID monotonicity, rotation overlap, receiver authentication, and raw-key lifetime correct? |
| Media routing | `crates/gateway/src/sfu.rs`, `crates/gateway/src/media_relay.rs`, client decentralized facades | Can any route, relay, token, or fallback cause media decryption, key leakage, unsigned endpoint use, token substitution, or SFrame downgrade? |
| Identity binding | `crates/identity/src/lib.rs`, `crates/client-core/src/identity.rs`, native keystore bridges | Are Ed25519 signing keys separated from X25519/KEM keys, and are hardware-provider failures closed rather than replaced by software persistence? |

Generated protobuf code, protocol validators, FFI shims, platform adapters,
dependency feature flags, and build/release configuration are supporting scope
where they affect the above paths.

## Required review evidence

The auditor receives a pinned source revision, `Cargo.lock`, Rust/Swift/Android
toolchain versions, dependency advisories, public protocol transcripts, and
sanitized test vectors. Each vector must include inputs, expected outputs, and
negative cases for malformed, stale, replayed, truncated, wrong-key, wrong-
epoch, and wrong-recipient data.

The project must provide independent vectors for:

1. PQXDH with and without an EC one-time prekey, wrong signing key, altered KEM
   ciphertext, all-zero X25519 output, and failed initial AEAD authentication;
2. MLS add/remove/self-update, pending-commit crash recovery, stale epoch,
   duplicate device, revoked credential, direct third-user, and multi-device
   group cases;
3. Sealed Sender altered routing header, recipient-key mismatch, nonce reuse,
   oversized ciphertext, and replayed envelope;
4. SFrame first key, rotation boundary, stale epoch, key-ID reuse, wrong key,
   unauthenticated frame, transform absence, and provider fallback cases; and
5. relay record/token/receipt signature alteration, expired capability,
   wrong-node token, wrong-session token, and open-relay SFrame downgrade cases.

## Auditor deliverables

- written threat-model and protocol-composition review;
- line-level findings with severity, exploitability, affected revision, and
  reproduction or proof;
- dependency and primitive review, including ML-KEM and OpenMLS status;
- verified test vectors and recommended additional negative tests;
- remediation review for all critical/high findings; and
- signed final report naming the audited commit and residual risks.

Critical or high findings, unresolved key-compromise or downgrade paths, and
missing reproducible vectors block the public release. A clean report does not
turn draft MLS/SFrame dependencies or unaudited platform SDKs into standards.

## Current known review gates

- ML-KEM-768 is implemented through the pinned RustCrypto dependency, whose
  independent-audit status must be confirmed by the auditor.
- The OpenMLS hybrid ML-KEM/X25519 ciphersuite is a draft interoperability
  point, not a final MLS post-quantum standard.
- Browser SFrame APIs are still capability-gated platform APIs; native media
  bindings need review together with the Rust key schedule.
- Secure Enclave/Keystore signing, FFI memory handling, and crash/restart key
  custody need real-device evidence in addition to source review.

## Sign-off record

```text
Auditor:
Firm / credentials:
Audited commit:
Scope exceptions:
Critical findings resolved:
High findings resolved:
Residual risks accepted by:
Final report / date:
```
