---
name: gpu-batch-outside-commit
description: "Use for GPU analytics/statistics/vector batch work."
category: andromeda-hardware
---

# gpu-batch-outside-commit

## When to use
GPU acceleration is proposed.

## Purpose
Use for GPU analytics/statistics/vector batch work.

## Process
- Allow only histograms, cardinality, skew, analytics, benchmarks, vectors outside commit path.
- Require CPU fallback and cancellation.
- Publish stats only after validation.
- Forbid GPU in commit, WAL, rollback, recovery, MVCC short visibility or security-critical logic.

## Expected output
- GPU eligibility decision.

## Guardrails
- No GPU dependency for transaction truth.

## Andromeda baseline
- Rust 1.95.0 / Edition 2024 / resolver 3.
- QUIC + custom Protobuf RPC; no gRPC and no JSON native protocol.
- Procedure-only application surface and SRPL contracts.
- WAL before visible commit; recovery and audit are mission-critical.
- Worker artifacts belong under `.work/codex/<task-slug>/`.
