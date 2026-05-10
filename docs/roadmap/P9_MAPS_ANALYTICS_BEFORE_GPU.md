# P9 — Maps and analytics before GPU

> **Status:** Roadmap phase  
> **Scope:** Sequenced work without calendar promises  
> **Parent:** `ROADMAP.md`

## In this article

- Define the phase purpose.
- State dependencies.
- Define deliverables.
- Define exit criteria and acceptance checks.

## Purpose

Build Maps and analytics semantics before optional GPU acceleration.

## Entry criteria

- Previous required phase exit criteria are satisfied.
- Required specifications exist for affected durable or security-sensitive structures.
- Required ADRs exist for architecture-affecting decisions.
- Required tests are designed before implementation is considered complete.

## Deliverables

| Deliverable | Required outcome |
|---|---|
| `MapDescriptor` | Implemented or specified with observable acceptance evidence. |
| `declared grain` | Implemented or specified with observable acceptance evidence. |
| `summarizability checks` | Implemented or specified with observable acceptance evidence. |
| `Immediate bounded Maps` | Implemented or specified with observable acceptance evidence. |
| `Incremental delta Maps` | Implemented or specified with observable acceptance evidence. |
| `Deferred/SnapshotOnly Maps` | Implemented or specified with observable acceptance evidence. |
| `columnar segment layout` | Implemented or specified with observable acceptance evidence. |

## Acceptance checks

- Immediate Map cost is bounded.
- Map maintenance cannot starve WAL.
- SnapshotOnly Map serves large analytics.


## Non-goals

- No calendar commitment.
- No expansion into unrelated feature breadth.
- No shortcut that bypasses contract, security, WAL, recovery, or observability.

## Exit criteria

```text
all required deliverables have evidence
all acceptance checks pass
critical traces are emitted
recovery behavior is known when durable state is affected
security admission is enforced when external calls are involved
```

## Risk controls

| Risk | Control |
|---|---|
| Integration drift | Keep work PR-sized and evidence-bound. |
| Hidden dynamic behavior | Require specs, contracts, and typed errors. |
| Recovery gap | Add crash/recovery tests before claiming completion. |
| Security bypass | Admission and audit must run before transaction creation. |
| Performance overreach | Keep adaptive work bounded and observable. |
