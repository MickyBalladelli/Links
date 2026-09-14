# Decentralized gossip benchmark

Status: benchmark runbook prepared. No propagation result is claimed until the
run is executed against 50 provisioned federation nodes and the signed report
is attached to the release revision.

## Measurement contract

Measure propagation of one signed, opaque `links.v1.FederatedEnvelopeBatch`
from an origin node through a multi-hop relay graph containing exactly 50
international nodes. The origin must not publish directly to every node. The
graph must use at least four peers per node, have a declared maximum hop count,
and include nodes in at least five geographic regions. Capture the first valid
receipt at every node and the final graph completion time.

Report propagation latency as:

- origin publish acknowledgement to first receipt at each hop;
- origin publish acknowledgement to first receipt at every region;
- origin publish acknowledgement to the last node receipt; and
- batch completion time after duplicate and retry traffic is settled.

Use monotonic clocks on each node. Calibrate clock offsets against the report
collector before each run and record the maximum clock uncertainty. A latency
number without clock uncertainty, node ID, hop number, and batch ID is not a
valid measurement.

## Correctness and privacy gates

Every accepted synthetic envelope must arrive at the intended nodes exactly
once after deduplication. The run must report lost batches, lost envelopes,
duplicate claims, expired batches, signature failures, retry counts, and
partition recovery. Relay nodes may validate routing, expiry, digest, and the
Ed25519 batch signature; they must not decrypt or inspect `sealed_payload`.

Use synthetic device IDs and pre-generated ciphertext only. Logs, traces,
metrics, packet captures, and crash output must contain no plaintext, MLS
state, private key, bearer token, sender identity, conversation ID, or raw
sealed payload. Scrub batch IDs and envelope IDs from exported public reports;
retain their mapping only with the restricted run evidence.

## Workload

Use the topology and rates in
`deploy/benchmarks/decentralized-gossip-50.yaml`:

1. warm all 50 nodes and verify authenticated peer sessions;
2. publish signed batches from rotating origin nodes so one node is not a
   permanent hot spot;
3. send traffic over local, regional, intercontinental, and maximum-hop paths;
4. inject bounded duplicate publishes and consumer redeliveries;
5. take one controlled link partition, recover it, and measure catch-up; and
6. stop new publishes, wait for all valid batches to settle, and reconcile
   every receipt against the origin ledger.

Run the minimum warmup, hold, partition, recovery, and settle durations in the
deployment contract. Repeat the complete workload three times with a fresh
synthetic batch namespace. The slowest propagation result and worst correctness
result are the release evidence.

Record the build commit, protocol version, node IDs and regions, graph edges,
relay provider and versions, instance sizes, TLS/mTLS settings, clock
calibration uncertainty, batch and envelope sizes, publish rate, retry policy,
partition window, retention, and run timestamps.

## Report

```text
Commit:
Topology / node count / regions:
Graph degree / maximum hops:
Relay provider and versions:
Batch and envelope profiles:
Publish rate / retry policy:
Clock uncertainty:
Warmup / hold / partition / recovery / settle duration:
Per-hop p50 / p95 / p99 / max:
Per-region p50 / p95 / p99 / max:
All-node completion p50 / p95 / p99 / max:
Lost batches / envelopes:
Duplicate claims:
Signature or expiry failures:
Partition recovery lag:
Resource saturation:
Privacy-log inspection result:
Run 1:
Run 2:
Run 3:
Worst-run result:
Owner / date:
Signature:
```

Attach raw histograms, topology snapshots, resource graphs, correctness
reconciliation, and the redacted configuration. Keep synthetic ciphertext,
credential material, and restricted ID mappings out of the repository.
