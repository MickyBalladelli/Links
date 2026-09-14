# Organization Mini-App and bot controls

Organization accounts expose Mini-Apps and bots only through an authenticated
control-plane policy. Both features start disabled.

## API

`GET /v1/organization/controls` returns:

```json
{
  "organization_id": "…",
  "mini_apps_enabled": false,
  "bots_enabled": false,
  "revision": 1
}
```

`PUT /v1/organization/controls` accepts the same two feature flags. The bearer
session must belong to an active organization device. Any active device may
read the policy; only the organization owner or a delegated organization admin
may update it. Every update increments `revision` atomically.

The server stores only the control flags and revision. It does not store Mini-
App private keys, sandbox capability tokens, bot secrets, or message content.
The client must check the latest revision before exposing a Mini-App or bot
surface. A disabled feature must fail closed locally, even if an older session
cached it as enabled.

These account controls are the outer gate. Mini-App network and cryptographic
operations still require the separate host-mediated grants in
[`mini-app-permissions.md`](mini-app-permissions.md). Bot credentials and
automation policy remain host-owned.
