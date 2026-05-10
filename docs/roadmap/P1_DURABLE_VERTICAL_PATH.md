# P1 — Durable vertical path

> **Status:** Roadmap phase  
> **Scope:** Sequenced work without calendar promises  
> **Parent:** `ROADMAP.md`

## In this article

- Define the phase purpose.
- State dependencies.
- Define deliverables.
- Define exit criteria and acceptance checks.

## Purpose

Prove Procedure to WAL to Recovery to ResultStream.

## Entry criteria

- Previous required phase exit criteria are satisfied.
- Required specifications exist for affected durable or security-sensitive structures.
- Required ADRs exist for architecture-affecting decisions.
- Required tests are designed before implementation is considered complete.

## Deliverables

| Deliverable | Required outcome |
|---|---|
| `Create Database` | Implemented or specified with observable acceptance evidence. |
| `Create Namespace` | Implemented or specified with observable acceptance evidence. |
| `Create Table` | Implemented or specified with observable acceptance evidence. |
| `Create Enum` | Implemented or specified with observable acceptance evidence. |
| `Create StructuredObject` | Implemented or specified with observable acceptance evidence. |
| `Create Procedure` | Implemented or specified with observable acceptance evidence. |
| `Execute Procedure via local RPC harness` | Implemented or specified with observable acceptance evidence. |
| `Durable WAL` | Implemented or specified with observable acceptance evidence. |
| `Commit visible` | Implemented or specified with observable acceptance evidence. |
| `Recovery test` | Implemented or specified with observable acceptance evidence. |

## Acceptance checks

- Crash before WAL flush leaves mutation invisible.
- Crash after WAL flush recovers mutation.
- Procedure Store records terminal outcome.


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
