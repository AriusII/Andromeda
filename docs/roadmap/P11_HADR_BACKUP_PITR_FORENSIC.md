# P11 — HA/DR, backup, PITR, and forensic drills

> **Status:** Roadmap phase  
> **Scope:** Sequenced work without calendar promises  
> **Parent:** `ROADMAP.md`

## In this article

- Define the phase purpose.
- State dependencies.
- Define deliverables.
- Define exit criteria and acceptance checks.

## Purpose

Complete production-oriented resilience workflows.

## Entry criteria

- Previous required phase exit criteria are satisfied.
- Required specifications exist for affected durable or security-sensitive structures.
- Required ADRs exist for architecture-affecting decisions.
- Required tests are designed before implementation is considered complete.

## Deliverables

| Deliverable | Required outcome |
|---|---|
| `WAL archive` | Implemented or specified with observable acceptance evidence. |
| `cold snapshot backup` | Implemented or specified with observable acceptance evidence. |
| `PITR by LSN` | Implemented or specified with observable acceptance evidence. |
| `cluster simulator` | Implemented or specified with observable acceptance evidence. |
| `quorum/fencing tests` | Implemented or specified with observable acceptance evidence. |
| `ForensicStart report` | Implemented or specified with observable acceptance evidence. |
| `restore drills` | Implemented or specified with observable acceptance evidence. |

## Acceptance checks

- Replica is not treated as backup.
- Promotion without quorum is refused.
- Restore to exact LSN is tested.
- ForensicStart blocks application traffic.


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
