# P0 — Specification baseline

> **Status:** Roadmap phase  
> **Scope:** Sequenced work without calendar promises  
> **Parent:** `ROADMAP.md`

## In this article

- Define the phase purpose.
- State dependencies.
- Define deliverables.
- Define exit criteria and acceptance checks.

## Purpose

Produce normative minimal specs before increasing feature breadth.

## Entry criteria

- Previous required phase exit criteria are satisfied.
- Required specifications exist for affected durable or security-sensitive structures.
- Required ADRs exist for architecture-affecting decisions.
- Required tests are designed before implementation is considered complete.

## Deliverables

| Deliverable | Required outcome |
|---|---|
| `Lexicon` | Implemented or specified with observable acceptance evidence. |
| `Catalog object model` | Implemented or specified with observable acceptance evidence. |
| `TypeSystem v0` | Implemented or specified with observable acceptance evidence. |
| `ProcedureContract v0` | Implemented or specified with observable acceptance evidence. |
| `RPC frame v0` | Implemented or specified with observable acceptance evidence. |
| `Page format v0` | Implemented or specified with observable acceptance evidence. |
| `WalRecord v0` | Implemented or specified with observable acceptance evidence. |
| `Manifest v0` | Implemented or specified with observable acceptance evidence. |
| `Transaction state machine` | Implemented or specified with observable acceptance evidence. |

## Acceptance checks

- All critical specs have invariants and rejection criteria.
- No roadmap item contains a calendar promise.
- Open decisions are explicit.


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
