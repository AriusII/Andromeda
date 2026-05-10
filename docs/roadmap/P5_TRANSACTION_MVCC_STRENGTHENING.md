# P5 — Transaction and MVCC strengthening

> **Status:** Roadmap phase  
> **Scope:** Sequenced work without calendar promises  
> **Parent:** `ROADMAP.md`

## In this article

- Define the phase purpose.
- State dependencies.
- Define deliverables.
- Define exit criteria and acceptance checks.

## Purpose

Strengthen isolation, MVCC versions, rollback, and GC pins.

## Entry criteria

- Previous required phase exit criteria are satisfied.
- Required specifications exist for affected durable or security-sensitive structures.
- Required ADRs exist for architecture-affecting decisions.
- Required tests are designed before implementation is considered complete.

## Deliverables

| Deliverable | Required outcome |
|---|---|
| `TransactionManager` | Implemented or specified with observable acceptance evidence. |
| `ActiveSnapshotRegistry` | Implemented or specified with observable acceptance evidence. |
| `MVCC row versions` | Implemented or specified with observable acceptance evidence. |
| `rollback evidence` | Implemented or specified with observable acceptance evidence. |
| `write conflict policy` | Implemented or specified with observable acceptance evidence. |
| `GC pins` | Implemented or specified with observable acceptance evidence. |

## Acceptance checks

- Snapshot visibility is deterministic.
- Write conflict behavior is defined.
- Rollback restores table, index, and Map state.


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
