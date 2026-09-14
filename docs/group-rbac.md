# Group membership, devices, and RBAC

The account service owns only group metadata and authorization. It never stores
MLS ratchet state, plaintext messages, or private keys. The authenticated bearer
session supplies the actor for every control-plane mutation; clients cannot
choose an actor ID in a request body.

## HTTP control plane

- `POST /v1/groups` creates a group and makes the authenticated account its
  owner. The body is `{ "group_id": "...", "kind": "group" }`; `kind` may
  also be `direct` or `channel` when that product surface is enabled.
- `GET /v1/groups/{group_id}/members` returns the current account membership
  and role snapshot to an existing member.
- `PUT /v1/groups/{group_id}/members/{user_id}/role` accepts
  `{ "role": "owner" | "admin" | "member" }`.
- `DELETE /v1/groups/{group_id}/members/{user_id}` removes a member. A member
  may target itself to leave.
- `DELETE /v1/devices/{device_id}` revokes one physical device owned by the
  authenticated account. Revocation is idempotent and immediately removes the
  device from directory and session authentication queries.

Owners may grant or remove any role. Admins may add or remove members but cannot
grant admin/owner or change an owner. Members cannot grant roles. The final owner
cannot be removed or demoted. PostgreSQL locks the group row for each mutation,
and deferred constraints prevent an ownerless group or an over-sized direct
group.

## MLS change flow

An account membership change is not an MLS change by itself. After the RBAC write,
an authorized current MLS member performs the matching TreeKEM operation:

1. Query the authenticated active-device directory for the new member's MLS
   credentials and KeyPackages.
2. Call `add_group_members`, `remove_devices`, or `self_update` on the shared
   client core as appropriate. Every credential is verified before it becomes a
   leaf; every physical device is a separate leaf.
3. Persist the exact `PendingCommit` and its next epoch before delivering the
   authenticated MLS control message to group devices.
4. Merge the pending commit only after delivery acceptance. Recipients apply the
   ordered `GroupWelcome`/`GroupCommit` update before opening application
   envelopes.

When a device is revoked, every local group state that contains its verified
device ID must stage `OpenMlsEngine::remove_devices`. The revocation endpoint
does not pretend the server can see encrypted MLS trees. The remaining device
that owns each group performs the removal and distributes the new epoch.

`current_epoch` is the committed OpenMLS epoch. Hosts may use
`process_commit_at_epoch`, `process_group_commit_at_epoch`, or the direct variant
with their durable checkpoint; a mismatch fails closed before processing. OpenMLS
still performs the cryptographic commit validation, sender authentication, leaf
credential checks, and TreeKEM epoch advancement.
