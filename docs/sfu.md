# Managed global SFU

Links uses LiveKit Cloud as the managed SFU for global voice, video, and live
stream transport. The gateway-side registry is in
`crates/gateway/src/sfu.rs`; the deployment contract is
`deploy/livekit/regions.example.yaml`.

## Placement flow

```text
authenticated call service
        |
        | opaque room name + short-lived LiveKit token
        v
Links region registry ---- preferred healthy region ----> LiveKit Cloud SFU
        ^                                                   |
        |                                                   | encrypted RTP
        +------------- MLS control / SFrame keys -----------+---- clients
```

The registry validates the public `wss://` and optional `turn:` endpoints,
requires at least two configured regions, tracks externally supplied health,
and selects the preferred healthy region with default/healthy fallback. A
pinned selector fails closed when the requested region is unavailable.

Room names must be opaque random values. Conversation IDs, handles, phone
numbers, and account IDs must stay out of room names and SFU logs.

## Media confidentiality

SFrame is mandatory in the placement contract. Clients exchange SFrame epoch
keys through authenticated MLS application control content, then encrypt media
frames at the WebRTC encoded-frame boundary. The SFU is allowed to route using
unencrypted RTP headers, but has no plaintext media, MLS state, or SFrame key
material. The next Phase 9 task configures and verifies that SFU behavior.

The call service gives each client a short-lived room token. API keys and
signing secrets remain server-side in a secret manager. LiveKit is not trusted
with Links account recovery, message delivery, or key-directory state.

## Global operations

Start with EU, US, and Asia region groups. Use latency-aware placement for
ordinary calls and pinned placement for data-residency policy. Drain a region
before deployment or maintenance, remove it from new placement, then mark it
unhealthy. New rooms fall back to the default or another healthy region;
existing rooms reconnect according to the client call flow.

Provisioning the external LiveKit Cloud project, regional quotas, DNS, and
credentials is an operator action and is not performed by this repository.
