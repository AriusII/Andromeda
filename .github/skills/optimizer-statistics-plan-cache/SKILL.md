---
name: optimizer-statistics-plan-cache
description: Handles optimizer, statistics, plan cache, PlanClass, and DecisionTrace work when prompts mention stats, optimizer, plan cache, PlanClass, or trace decisions.
license: MIT
---

# optimizer-statistics-plan-cache

## When to use
- The prompt mentions optimizer, statistics, StatsVersion, plan cache, PlanCacheKey, PlanClass, DecisionTrace, cost model, or cardinality.
- A change touches andromeda-optimizer, statistics, plan-cache, decision-trace, or SRPL optimizer passes.
- A performance fix could alter correctness, cache identity, or observability.

## Purpose
Keep optimizer behavior explainable, versioned, cache-safe, and bounded by procedure contracts. The skill prevents statistics publication, plan cache keys, and decision traces from becoming invisible mutable state that changes procedure execution without evidence.

## Process
1. Read optimizer/statistics architecture plus stats, plan-cache-key, and decision-trace specs.
2. Identify the versioned inputs: ProcedureContract/ContractHash, CatalogVersion, StatsVersion, PlanClass, policy, and hardware profile if advisory.
3. Ensure cache keys include all correctness-relevant fields and exclude volatile non-determinism.
4. Publish statistics only through documented versioning and invalidation rules.
5. Emit DecisionTrace evidence sufficient to explain plan choice without leaking secrets or relying on learned opacity.

## Expected output
- A plan identity and invalidation matrix.
- DecisionTrace fields required for the plan choice.
- Tests for key equality/inequality, stats publication, and plan-class behavior.

## Reference docs
- `docs/architecture/OPTIMIZER_STATS_ARCHITECTURE.md`
- `docs/specifications/SPEC_STATS_OBJECT_V0.md`
- `docs/specifications/SPEC_PLAN_CACHE_KEY_V0.md`
- `docs/specifications/SPEC_DECISION_TRACE_V0.md`
- `docs/adr/ADR-0014-STATS_VERSION_PUBLICATION.md`
- `docs/adr/ADR-0015-PLAN_CACHE_KEY_AND_PLAN_CLASS.md`

## Guardrails
- Do not let advisory statistics override correctness.
- Do not omit correctness-relevant fields from plan cache keys.
- Do not use learned components without evidence boundaries.
- When doctrine is implicated, cite `docs/project/ANDROMEDA_DOCTRINE.md` and treat conflicts as blockers.

## Andromeda baseline
- Rust 1.95.0 / Edition 2024 / resolver 3.
- QUIC + custom Protobuf RPC.
- no gRPC.
- no JSON native protocol.
- procedure-only application surface through typed, cataloged, versioned Procedure contracts.
- WAL-before-visible-commit with recovery evidence.
- GPU and accelerated paths outside commit, rollback, recovery, MVCC visibility, security admission, and authorization paths.
