---
name: optimizer-statistics-plan-cache
description: "Use for optimizer, statistics, plan cache, PlanClass and DecisionTrace."
category: andromeda-optimizer
---

# optimizer-statistics-plan-cache

## When to use
Planning, statistics, cost, cache or learned evidence is involved.

## Purpose
Use for optimizer, statistics, plan cache, PlanClass and DecisionTrace.

## Process
- Key plans by ProcedureId, ContractHash, CatalogVersion, StatsVersion, PolicyVersion, shape and PlanClass.
- Make decisions observable with DecisionTrace.
- Use bounded multi-plan classes and hysteresis.
- Publish stats through candidate validation.

## Expected output
- Optimizer/statistics plan or review.

## Guardrails
- No unbounded plan specialization.

## Andromeda baseline
- Rust 1.95.0 / Edition 2024 / resolver 3.
- QUIC + custom Protobuf RPC; no gRPC and no JSON native protocol.
- Procedure-only application surface and SRPL contracts.
- WAL before visible commit; recovery and audit are mission-critical.
- Worker artifacts belong under `.work/codex/<task-slug>/`.
