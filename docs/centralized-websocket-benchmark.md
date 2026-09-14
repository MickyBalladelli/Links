# Centralized WebSocket benchmark

Status: benchmark runbook prepared. No one-million-socket result is claimed
until the run is executed against a provisioned environment and the report is
attached to the release revision.

## Pass condition

Run one million authenticated, steady-state `links.v1` WebSocket connections
across at least two gateway instances in one region, then measure the
end-to-end time from an opaque client `Send` frame to the target device's
opaque delivery frame. Pass only when:

- p99 delivery latency is below 50 ms for the selected payload profile;
- no accepted envelope is lost or delivered twice;
- reconnects, queue writes, and cursor replay remain correct;
- no gateway or queue instance exceeds its declared CPU, memory, socket, or
  network budget; and
- logs, traces, metrics, and crash output contain no plaintext, MLS state,
  private key, bearer token, or unredacted payload.

The benchmark must report p50, p95, p99, max, connection establishment,
acceptance, local delivery, cross-instance delivery, queue, and replay
latencies separately. A single average below 50 ms is not a pass.

## Workload

Use synthetic accounts and device IDs only. Each connection must complete the
normal Hello/authentication handshake and heartbeat. Use the smallest normal
sealed envelope and repeat with 64 KiB and the maximum supported envelope
profile. Keep the encrypted bytes generated before the run; the gateway must
not need a plaintext message generator.

Use a fixed sender/recipient distribution that includes:

1. local delivery on the same gateway;
2. delivery to a device leased on the other gateway instance;
3. no active socket, followed by cursor replay; and
4. duplicate send retries using the same envelope ID.

Measure send-to-target-delivery with monotonic clocks in the load clients.
Record the build commit, protocol version, region, instance sizes, kernel/file
descriptor limits, TLS configuration, Redis/PostgreSQL/NATS versions, queue
retention, payload size, connection count, message rate, and test duration.

## Run boundary

The load generator must run outside the gateway hosts. Do not count a local
socket-only benchmark as a one-million-client result. Warm the system for at
least five minutes, hold one million connections for at least ten minutes,
then run the delivery sample for at least five minutes. Repeat three times;
the worst run is the release result.

The example target and resource names are in
`deploy/benchmarks/centralized-websocket-1m.yaml`. Provisioning the load
generator, TLS certificates, synthetic accounts, gateway instances, and
observability backend is an operator action.

## Report

```text
Commit:
Region / topology:
Gateway instances and sizes:
Active connections:
Message rate / payload profiles:
Warmup / steady-state / sample duration:
p50 / p95 / p99 / max delivery latency:
Lost / duplicate / replay failures:
Resource saturation:
Privacy-log inspection result:
Run 1:
Run 2:
Run 3:
Worst-run result:
Owner / date:
```

Attach raw latency histograms, saturation graphs, error counts, and the
redacted configuration. Keep synthetic ciphertext and access credentials out
of the repository.
