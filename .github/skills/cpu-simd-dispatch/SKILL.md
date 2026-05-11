---
name: cpu-simd-dispatch
description: Guides CPU SIMD, target features, runtime dispatch, and scalar fallback when prompts mention SIMD, CPU features, vectorization, target_feature, or hardware dispatch.
license: MIT
---

# cpu-simd-dispatch

## When to use
- The user mentions CPU SIMD, AVX, SSE, NEON, target_feature, runtime dispatch, vectorization, scalar fallback, or hardware profile.
- A change touches andromeda-hardware, optimizer hardware advisory code, analytics kernels, or batch execution.
- A review sees unsafe or architecture-specific intrinsics.

## Purpose
Allow performance improvements while preserving portability and deterministic correctness. The skill ensures SIMD paths are gated by runtime feature detection, covered by scalar fallback, and limited to advisory/performance domains rather than durable truth boundaries.

## Process
1. Read hardware architecture and advisory optimizer hardware spec.
2. Classify the accelerated operation as advisory/performance and identify correctness reference behavior.
3. Require runtime feature detection and a scalar implementation that is tested as the oracle.
4. Keep unsafe blocks private, documented, and minimal; never expose target-specific types as public contracts.
5. Add equivalence tests across scalar and SIMD paths, plus fallback tests when features are unavailable.

## Expected output
- A dispatch plan showing feature detection, selected kernel, and scalar fallback.
- Safety notes for unsafe/target_feature code.
- Equivalence and fallback tests required for acceptance.

## Reference docs
- `docs/architecture/HARDWARE_ARCHITECTURE.md`
- `docs/specifications/SPEC_ADVISORY_OPTIMIZER_HARDWARE_V0.md`

## Guardrails
- No SIMD dependency in commit, WAL, recovery, MVCC visibility, or security-critical code.
- No public API tied to CPU-specific layout.
- No unsafe intrinsic code without a documented invariant.
- When doctrine is implicated, cite `docs/project/ANDROMEDA_DOCTRINE.md` and treat conflicts as blockers.

## Andromeda baseline
- Rust 1.95.0 / Edition 2024 / resolver 3.
- QUIC + custom Protobuf RPC.
- no gRPC.
- no JSON native protocol.
- procedure-only application surface through typed, cataloged, versioned Procedure contracts.
- WAL-before-visible-commit with recovery evidence.
- GPU and accelerated paths outside commit, rollback, recovery, MVCC visibility, security admission, and authorization paths.
