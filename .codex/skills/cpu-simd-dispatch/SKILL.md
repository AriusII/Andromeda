---
name: cpu-simd-dispatch
description: "Use for CPU SIMD, target features, runtime dispatch and scalar fallback."
category: andromeda-hardware
---

# cpu-simd-dispatch

## When to use
SIMD or CPU-specific optimization is involved.

## Purpose
Use for CPU SIMD, target features, runtime dispatch and scalar fallback.

## Process
- Use runtime feature detection and scalar fallback.
- Isolate SIMD kernels.
- Test scalar and accelerated outputs with identical vectors.
- Benchmark before accepting complexity.

## Expected output
- SIMD plan or implementation checklist.

## Guardrails
- No target-feature call without guard.

## Andromeda baseline
- Rust 1.95.0 / Edition 2024 / resolver 3.
- QUIC + custom Protobuf RPC; no gRPC and no JSON native protocol.
- Procedure-only application surface and SRPL contracts.
- WAL before visible commit; recovery and audit are mission-critical.
- Worker artifacts belong under `.work/codex/<task-slug>/`.
