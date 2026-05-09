# Open Decisions

This registry tracks architecture questions that still require explicit ADRs.
It is intentionally small: status reports, historical migration notes, and
temporary execution plans belong outside the active documentation set.

## Current Open Items

| Decision | Owner area | Status | Blocking scope |
| --- | --- | --- | --- |
| Page size policy | Storage | Open | Physical storage format and page-store benchmarks. |
| Contract hash canonicalization | Catalog and procedure contracts | Open | Procedure compatibility and generated client stability. |
| Catalog version granularity | Catalog and DefinitionBatch | Open | Batch publication, recovery replay, and plan-cache invalidation. |
| Optimizer criticality level | Optimizer and statistics | Open | Release gates and fallback behavior for plan selection. |
| HA/DR quorum strategy | HA/DR and recovery | Open | Promotion, fencing, witness policy, and split-brain prevention. |
| Analytics acceleration policy | Analytics and optimizer | Open | Off-commit acceleration scope and evidence requirements. |

## Decision Rules

- Each decision must land as a dedicated ADR before it becomes a release
  contract.
- Candidate designs must cite implementation impact, validation strategy, and
  rollback constraints.
- Performance-sensitive decisions require benchmark evidence.
- Durability-sensitive decisions require owner tests and recovery evidence.
- Security-sensitive decisions require audit and permission boundary tests.

## Closed Doctrine

| Doctrine | Status |
| --- | --- |
| QUIC-only RPC, no gRPC application surface | Decided. |
| Procedure-based application interaction, no ad hoc SQL public API | Decided. |
| GPU work stays outside commit, WAL replay, rollback, and short-visibility paths | Decided. |

## References

- ADR index: `docs/adr/README.md`
- Architecture overview: `docs/architecture/overview.md`
- Dependency policy: `docs/architecture/dependency-policy.md`
- Release gates: `docs/governance/release-gates.md`
