# Public-release readiness review

Status: review contract prepared. Public release remains blocked until every
row below has recorded device, build, and end-to-end evidence. Source review
does not replace physical-device, provider, or failure-injection acceptance.

## Gate matrix

| Area | Current evidence | Release decision | Required evidence |
| --- | --- | --- | --- |
| Battery use | Android and iOS clients use bounded background recovery, 30-second connected heartbeats, and utility/background queues. No device power measurements are stored in this repository. | Blocked | Measure idle connected, background wakeup, offline catch-up, reconnect storm, and active text cases on representative devices. Record elapsed time, battery percentage or OS energy report, wake count, network bytes, CPU, and build ID. |
| Reconnect behavior | Android `ConnectionManager`, iOS `IOSConnectionManager`, Web managers, and desktop session code implement binary `links.v1`, Hello deadlines, heartbeat, fenced sessions, and full-jitter 1–30 second backoff. | Contract ready; acceptance pending | Force socket close, DNS/TLS failure, gateway restart, regional route loss, and process suspension. Verify exactly one active session, no token leakage, no duplicate render, and replay from the last durable cursor. |
| Offline delivery | Gateway mailbox cursors, append-only encrypted payloads, replay, QueueAck ordering, APNs/FCM wakeups, and Android/iOS recovery workers exist. TTL deletion and live push/provider acceptance remain open. | Blocked | Send while each client is force-stopped, offline, backgrounded, and behind an expired cursor. Verify one decrypt/render, no acknowledgement before durable commit, push loss recovery, expiry purge, and ciphertext-only logs. |
| Key recovery | BIP-39 and passkey-PRF restore stay local, hardware vaults seal restored identities, and restore assigns a fresh device/node requiring enrollment. | Blocked | On physical iOS and Android devices, recover from seed and passkey after restart, lock/unlock, key invalidation, and app reinstall. Verify no server seed/PRF/plaintext key, fresh device enrollment, regenerated pre-keys, and old-device revocation. |
| Multi-device removal | Account service revokes devices and removes them from active directory/session queries. MLS `remove_devices` stages verified leaf removal, but revocation-driven rotation and host-wide group reconciliation are not wired. | Blocked | Revoke one device and delegated descendants. Verify current sessions stop, directory/fanout excludes them, every affected MLS group commits a removal, remaining devices receive the new epoch, and the removed device cannot decrypt new traffic. |

## Required test sequence

Run on signed release candidates with synthetic accounts and disposable
devices. Keep message contents and key material out of logs.

1. Record baseline battery, network, CPU, memory, socket, and cursor state.
2. Run connected idle and active-text battery cases for Android and iOS.
3. Inject reconnect failures while sending online and during mailbox replay.
4. Force-stop, background, and disconnect recipients; then deliver and lose
   push wakeups and reconcile through cursor replay.
5. Restore an identity from seed phrase and passkey on a fresh physical device;
   enroll it as a new node and verify pre-key availability.
6. Revoke the old device and every delegated child, then verify MLS removal and
   post-removal message rejection/decryption behavior.
7. Repeat after process restart and account-service/gateway restart. Attach raw
   device reports, sanitized traces, cursor reconciliation, and signed owner
   approval.

## Evidence record

```text
Release commit / build IDs:
Android models / OS / hardware security:
iOS models / OS / Secure Enclave result:
Battery reports:
Reconnect and failure-injection report:
Offline delivery and push report:
Seed/passkey recovery report:
Device revocation and MLS-removal report:
Privacy-log inspection:
Open defects / expiry:
Owner / date:
Security sign-off:
```

The release owner may close the TODO item only after all five decisions are
`Pass`, open defects have owners and expiry dates, and the signed evidence is
stored with the release record.
