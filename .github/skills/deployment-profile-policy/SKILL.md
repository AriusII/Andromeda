---
name: deployment-profile-policy
description: Validates deployment profile and GPU policy gates when prompts mention OperationalProfile, deployment profile, GPU policy, environment gates, or runtime profile.
license: MIT
---

# deployment-profile-policy

## When to use
- The prompt mentions deployment profile, OperationalProfile, profile gate, GPU execution policy, environment validation, production profile, or hardware enablement.
- A change adds runtime feature switches, deployment configuration, or GPU policy checks.
- A review asks whether a profile can enable acceleration safely.

## Purpose
Ensure deployment profiles encode explicit operational constraints and hardware policy gates. The skill helps agents validate whether optional acceleration and environment capabilities are allowed while keeping correctness-critical paths independent of GPU availability.

## Process
1. Read deployment profile operations, GPU execution policy spec, and GPU-outside-commit ADR.
2. Identify the profile field being introduced or consumed and whether it affects correctness or only advisory/performance behavior.
3. Require explicit validation errors for unsupported combinations and safe defaults for absent optional hardware.
4. Ensure GPU enablement remains outside commit, rollback, WAL, recovery, MVCC visibility, and security authorization.
5. Add profile parsing, validation, rejection, and fallback tests.

## Expected output
- A profile gate matrix with allowed, rejected, and fallback states.
- GPU policy impact assessment.
- Tests proving scalar/CPU fallback and rejection of unsafe profile combinations.

## Reference docs
- `docs/operations/DEPLOYMENT_PROFILE.md`
- `docs/specifications/SPEC_GPU_EXECUTION_POLICY_V0.md`
- `docs/adr/ADR-0009-GPU_OUTSIDE_COMMIT_PATH.md`

## Guardrails
- No correctness dependency on optional GPU hardware.
- No silent profile downgrade for security or durability settings.
- No deployment profile that enables forbidden protocol surfaces.
- When doctrine is implicated, cite `docs/project/ANDROMEDA_DOCTRINE.md` and treat conflicts as blockers.

## Andromeda baseline
- Rust 1.95.0 / Edition 2024 / resolver 3.
- QUIC + custom Protobuf RPC.
- no gRPC.
- no JSON native protocol.
- procedure-only application surface through typed, cataloged, versioned Procedure contracts.
- WAL-before-visible-commit with recovery evidence.
- GPU and accelerated paths outside commit, rollback, recovery, MVCC visibility, security admission, and authorization paths.
