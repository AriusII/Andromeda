---
name: gpu-batch-outside-commit
description: Keeps GPU analytics, statistics, vector, and batch paths outside commit, recovery, and security paths when prompts mention GPU, CUDA, batch acceleration, or vector analytics.
license: MIT
---

# gpu-batch-outside-commit

## When to use
- The prompt mentions GPU, CUDA, ROCm, accelerator, vector batch, analytics batch, GPU statistics, or optional acceleration.
- A change touches hardware, maps, optimizer statistics, analytics, or deployment GPU policy.
- A review asks whether GPU can be used for commit, recovery, security, or MVCC visibility.

## Purpose
Permit optional GPU acceleration only where Andromeda doctrine allows it: advisory analytics, statistics, vector, or batch work with CPU fallback and no correctness authority. The skill blocks GPU dependencies from mission-critical durability, visibility, and authorization paths.

## Process
1. Read GPU execution policy and the GPU-outside-commit ADR.
2. Classify the proposed GPU use as allowed advisory/batch work or forbidden critical-path work.
3. Require CPU/scalar fallback, deterministic equivalence tests, bounded resource use, and profile gating.
4. Ensure GPU outputs cannot decide commit, rollback, WAL durability, recovery, MVCC visibility, or authorization.
5. Add disablement tests and traces proving execution remains correct when GPU is absent or fails.

## Expected output
- An allowed/forbidden GPU boundary decision.
- Fallback and OperationalProfile validation requirements.
- Tests for equivalence, disablement, and critical-path exclusion.

## Reference docs
- `docs/specifications/SPEC_GPU_EXECUTION_POLICY_V0.md`
- `docs/adr/ADR-0009-GPU_OUTSIDE_COMMIT_PATH.md`

## Guardrails
- Never place GPU in commit/recovery/security-critical paths.
- No correctness dependency on GPU output.
- No unbounded GPU batch resource use.
- When doctrine is implicated, cite `docs/project/ANDROMEDA_DOCTRINE.md` and treat conflicts as blockers.

## Andromeda baseline
- Rust 1.95.0 / Edition 2024 / resolver 3.
- QUIC + custom Protobuf RPC.
- no gRPC.
- no JSON native protocol.
- procedure-only application surface through typed, cataloged, versioned Procedure contracts.
- WAL-before-visible-commit with recovery evidence.
- GPU and accelerated paths outside commit, rollback, recovery, MVCC visibility, security admission, and authorization paths.
