---
name: nvme-resource-pressure
description: Handles NVMe pressure, slow client, backpressure, and replica lag operations when prompts mention resource pressure, NVMe, slow clients, backpressure, or lag.
license: MIT
---

# nvme-resource-pressure

## When to use
- The user mentions NVMe pressure, disk saturation, slow client, backpressure, queue growth, resource pressure, replica lag, or throttling.
- A change touches resource admission, WAL pressure, RPC streaming, result backpressure, storage IO, or replica catch-up.
- An operational answer must choose mitigation steps.

## Purpose
Guide operational triage and implementation choices under storage and client pressure without hiding durability or correctness failures. The skill connects resource symptoms to runbooks and backpressure behavior instead of adding retries or unbounded queues.

## Process
1. Read the NVMe, slow-client backpressure, and replica-lag runbooks before recommending mitigation.
2. Classify the bottleneck: WAL fsync, page IO, compaction, ResultStream client, admission queue, or replica apply lag.
3. Prefer bounded queues, explicit admission decisions, cancellation/backpressure propagation, and metrics over blind retry loops.
4. Check that pressure handling never skips WAL durability, security admission, audit, or recovery evidence.
5. Return immediate triage steps plus code/test changes only if the prompt asks for implementation.

## Expected output
- A pressure classification and runbook-driven mitigation list.
- Backpressure boundaries, metrics, and alert signals to inspect.
- Risks to durability, client correctness, or replica freshness.

## Reference docs
- `docs/runbooks/RUNBOOK_NVME_PRESSURE.md`
- `docs/runbooks/RUNBOOK_SLOW_CLIENT_BACKPRESSURE.md`
- `docs/runbooks/RUNBOOK_REPLICA_LAG.md`

## Guardrails
- Do not fix pressure by dropping committed data or bypassing fsync.
- Do not add unbounded buffers.
- Do not mask persistent lag with retries alone.
- When doctrine is implicated, cite `docs/project/ANDROMEDA_DOCTRINE.md` and treat conflicts as blockers.

## Andromeda baseline
- Rust 1.95.0 / Edition 2024 / resolver 3.
- QUIC + custom Protobuf RPC.
- no gRPC.
- no JSON native protocol.
- procedure-only application surface through typed, cataloged, versioned Procedure contracts.
- WAL-before-visible-commit with recovery evidence.
- GPU and accelerated paths outside commit, rollback, recovery, MVCC visibility, security admission, and authorization paths.
