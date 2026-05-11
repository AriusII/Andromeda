---
name: mission-critical-release-gates
description: Evaluates C4/C5 readiness, release gates, CI gates, and acceptance when prompts mention release, readiness, CI gates, acceptance, or mission-critical status.
license: MIT
---

# mission-critical-release-gates

## When to use
- The user mentions release gate, readiness, C4, C5, acceptance, CI gate, production, enterprise hardening, or feature complete.
- A task prepares a PR summary, release note, or status update.
- A review asks whether validation is sufficient for mission-critical behavior.

## Purpose
Prevent readiness claims without evidence. The skill helps agents map changes to C4/C5 criticality, required CI gates, acceptance checklists, and feature-level proof before saying a capability is release-ready.

## Process
1. Read release gates, CI gates, acceptance checklist, feature acceptance gate, and crate criticality matrix.
2. Classify affected crates/features by criticality and map each to required evidence.
3. Check tests, fuzz/property/crash recovery, topology guards, observability, security, and runbook readiness as applicable.
4. Separate implemented, validated, documented, and release-ready states.
5. Do not claim C5 readiness until every required gate has passing evidence.

## Expected output
- A readiness matrix with evidence links or missing blockers.
- Required commands/tests and whether they passed.
- A conservative release recommendation: ready, not ready, or blocked with reasons.

## Reference docs
- `docs/testing/RELEASE_GATES.md`
- `docs/testing/CI_GATES.md`
- `docs/testing/ACCEPTANCE_CHECKLIST.md`
- `docs/project/FEATURE_ACCEPTANCE_GATE.md`
- `docs/project/CRATE_CLUSTER_CRITICALITY_MATRIX.md`

## Guardrails
- No readiness claim without validation evidence.
- Do not downgrade C5 requirements for convenience.
- Do not treat documentation intent as tested behavior.
- When doctrine is implicated, cite `docs/project/ANDROMEDA_DOCTRINE.md` and treat conflicts as blockers.

## Andromeda baseline
- Rust 1.95.0 / Edition 2024 / resolver 3.
- QUIC + custom Protobuf RPC.
- no gRPC.
- no JSON native protocol.
- procedure-only application surface through typed, cataloged, versioned Procedure contracts.
- WAL-before-visible-commit with recovery evidence.
- GPU and accelerated paths outside commit, rollback, recovery, MVCC visibility, security admission, and authorization paths.
