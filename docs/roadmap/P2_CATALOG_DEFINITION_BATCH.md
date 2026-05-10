# P2 — Catalog and DefinitionBatch durability

> **Status:** Roadmap phase  
> **Scope:** Sequenced work without calendar promises  
> **Parent:** `ROADMAP.md`

## In this article

- Define the phase purpose.
- State dependencies.
- Define deliverables.
- Define exit criteria and acceptance checks.

## Purpose

Make catalog publication durable, versioned, and recoverable.

## Entry criteria

- Previous required phase exit criteria are satisfied.
- Required specifications exist for affected durable or security-sensitive structures.
- Required ADRs exist for architecture-affecting decisions.
- Required tests are designed before implementation is considered complete.

## Deliverables

| Deliverable | Required outcome |
|---|---|
| `CatalogStore durable V0` | Implemented or specified with observable acceptance evidence. |
| `DryRun` | Implemented or specified with observable acceptance evidence. |
| `ApplyDefinitionBatch` | Implemented or specified with observable acceptance evidence. |
| `CatalogVersion publication` | Implemented or specified with observable acceptance evidence. |
| `Compatibility policy` | Implemented or specified with observable acceptance evidence. |

## Acceptance checks

- Crash mid-apply publishes no half-catalog.
- Invalid SRPL fails dry-run.
- Breaking changes require policy.


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
