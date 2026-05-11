---
name: maps-analytics-summarizability
description: Guides Maps, analytical projections, grain, refresh, and summarizability when prompts mention maps, analytics, projections, grain, refresh, or aggregate correctness.
license: MIT
---

# maps-analytics-summarizability

## When to use
- The prompt mentions Maps, analytical projections, summarizability, grain, aggregate correctness, refresh, map refresh failure, or CPU-first analytics.
- A change touches andromeda-maps, analytics, columnar, optimizer stats, or map refresh operations.
- A review concerns stale projections or aggregate correctness.

## Purpose
Keep analytical projections useful without corrupting transactional truth. The skill makes agents define grain, refresh semantics, summarizability rules, and optimizer/statistics relationships before changing Maps or analytics behavior.

## Process
1. Read optimizer/statistics architecture, map refresh runbook, and P10 CPU-first roadmap phase.
2. Define the map grain, source Procedure/contract, refresh trigger, staleness model, and summarizability constraints.
3. Keep Maps as derived analytical structures, not transactional truth or commit authority.
4. Plan refresh failure behavior: quarantine, retry policy, observability, and client-visible status.
5. Add tests for grain correctness, refresh idempotency, stale-read signaling, and aggregate rollup constraints.

## Expected output
- A map/projection contract with grain, source, refresh, and staleness.
- Summarizability and aggregate correctness checklist.
- Refresh failure handling and validation tests.

## Reference docs
- `docs/architecture/OPTIMIZER_STATS_ARCHITECTURE.md`
- `docs/runbooks/RUNBOOK_MAP_REFRESH_FAILURE.md`
- `docs/roadmap/phases/P10_MAPS_ANALYTICS_CPU_FIRST.md`

## Guardrails
- Do not use maps as source-of-truth for commits.
- Do not hide stale projection status.
- Do not allow GPU acceleration to affect transactional correctness.
- When doctrine is implicated, cite `docs/project/ANDROMEDA_DOCTRINE.md` and treat conflicts as blockers.

## Andromeda baseline
- Rust 1.95.0 / Edition 2024 / resolver 3.
- QUIC + custom Protobuf RPC.
- no gRPC.
- no JSON native protocol.
- procedure-only application surface through typed, cataloged, versioned Procedure contracts.
- WAL-before-visible-commit with recovery evidence.
- GPU and accelerated paths outside commit, rollback, recovery, MVCC visibility, security admission, and authorization paths.
