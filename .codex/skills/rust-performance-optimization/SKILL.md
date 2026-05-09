---
name: rust-performance-optimization
description: "Use for measured Rust performance improvement."
category: rust-performance
---

# rust-performance-optimization

## When to use
Code is slow or allocation-heavy.

## Purpose
Use for measured Rust performance improvement.

## Process
- Write hypothesis, baseline, change, measurement and regression gate.
- Prioritize algorithm/data layout before micro-optimization.
- Check memory, compile time, readability and rollback path.
- Use profiling before claiming optimization.

## Expected output
- Performance plan or evidence report.

## Guardrails
- No unmeasured optimization claims.

## Andromeda baseline
- Rust 1.95.0 / Edition 2024 / resolver 3.
- QUIC + custom Protobuf RPC; no gRPC and no JSON native protocol.
- Procedure-only application surface and SRPL contracts.
- WAL before visible commit; recovery and audit are mission-critical.
- Worker artifacts belong under `.work/codex/<task-slug>/`.
