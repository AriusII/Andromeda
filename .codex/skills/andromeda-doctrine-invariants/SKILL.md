---
name: andromeda-doctrine-invariants
description: "Use to enforce Andromeda strict-boundary doctrine and C5 invariants."
category: andromeda-core
---

# andromeda-doctrine-invariants

## When to use
Any design or code change may affect core doctrine.

## Purpose
Use to enforce Andromeda strict-boundary doctrine and C5 invariants.

## Process
- Check no SQL ad hoc application surface.
- Check ProcedureContract, ContractHash, CatalogVersion, PolicyVersion and StatsVersion usage.
- Check WAL before visible commit and recovery capability.
- Check GPU/analytics/evidence remain outside commit/recovery/security-critical paths.
- Check decisions are versioned, observable, bounded, explainable and disableable.

## Expected output
- Invariant checklist and violations.

## Guardrails
- Do not weaken doctrine for convenience.

## Andromeda baseline
- Rust 1.95.0 / Edition 2024 / resolver 3.
- QUIC + custom Protobuf RPC; no gRPC and no JSON native protocol.
- Procedure-only application surface and SRPL contracts.
- WAL before visible commit; recovery and audit are mission-critical.
- Worker artifacts belong under `.work/codex/<task-slug>/`.
