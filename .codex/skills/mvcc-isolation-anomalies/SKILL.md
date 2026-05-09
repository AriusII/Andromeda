---
name: mvcc-isolation-anomalies
description: "Use for MVCC visibility, isolation policies, anomalies and long readers."
category: andromeda-transaction
---

# mvcc-isolation-anomalies

## When to use
Isolation, MVCC, version GC, or transaction semantics are involved.

## Purpose
Use for MVCC visibility, isolation policies, anomalies and long readers.

## Process
- Document guarantees by anomalies, not just labels.
- Distinguish Snapshot Isolation from Serializable.
- Ensure version GC respects active snapshots, recovery, backup and forensic windows.
- Make isolation visible through contract or policy.

## Expected output
- MVCC/isolation analysis.

## Guardrails
- No implicit isolation defaults for critical Procedures.

## Andromeda baseline
- Rust 1.95.0 / Edition 2024 / resolver 3.
- QUIC + custom Protobuf RPC; no gRPC and no JSON native protocol.
- Procedure-only application surface and SRPL contracts.
- WAL before visible commit; recovery and audit are mission-critical.
- Worker artifacts belong under `.work/codex/<task-slug>/`.
