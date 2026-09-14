# Managed global SFU deployment

Links uses LiveKit Cloud as the managed SFU provider for the first global
rollout. This directory defines the configuration boundary; it does not hold
provider secrets or provision an external account.

## Configure

1. Create a LiveKit Cloud project and enable the required regional endpoints.
2. Set the environment variables named in
   [`regions.example.yaml`](regions.example.yaml) through the deployment secret
   manager. URLs are public connection metadata; API keys and signing secrets
   stay in the secret manager.
3. Build `links_gateway::sfu::LiveKitCloudDeployment` from the configured
   project, default region, and endpoint list. Keep at least two healthy
   regions for a global deployment.
4. Issue short-lived room access tokens from the trusted call-session service.
   Never put LiveKit API secrets, MLS keys, or SFrame keys in the client or in
   this repository.

## Security policy

- Use opaque random room names. Do not use conversation IDs, usernames, phone
  numbers, or account IDs as LiveKit room names.
- Set `require_sframe: true` and refuse to start a call when the client cannot
  install SFrame transforms. The SFU may route RTP headers, but it must not
  receive plaintext media or MLS/SFrame epoch keys.
- Disable recording, egress, and ingress for E2EE rooms unless a separately
  reviewed end-to-end encrypted workflow exists.
- Keep the MLS control channel on the authenticated Links gateway. LiveKit is
  a media transport, not a key directory or message router.

## Operations

Use the closest healthy region as the placement hint. On regional failure,
stop placing new rooms there, drain existing rooms, mark the endpoint
unhealthy, and allow new sessions to select the default or another healthy
region. Use pinned placement when policy forbids cross-region fallback.

Monitor connection success, ICE/TURN success, packet loss, jitter, RTT, room
join latency, and region health without logging room names, user IDs, media
frames, or key material. Validate failover, drain, token expiry, and SFrame
failure before release.

The actual LiveKit Cloud project, regional configuration, DNS, quotas, and
credentials still require an operator with provider access.

References:

- [LiveKit Cloud regions](https://docs.livekit.io/deploy/admin/regions/)
- [LiveKit self-hosted distributed deployment](https://docs.livekit.io/transport/self-hosting/distributed/)
