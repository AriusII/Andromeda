# P6 — QUIC application runtime

> **Status:** Roadmap phase  
> **Scope:** Sequenced work without calendar promises  
> **Parent:** `ROADMAP.md`

## In this article

- Define the phase purpose.
- State dependencies.
- Define deliverables.
- Define exit criteria and acceptance checks.

## Purpose

Enable feature-gated QUIC application runtime after admission gates are stable.

## Entry criteria

- Previous required phase exit criteria are satisfied.
- Required specifications exist for affected durable or security-sensitive structures.
- Required ADRs exist for architecture-affecting decisions.
- Required tests are designed before implementation is considered complete.

## Deliverables

| Deliverable | Required outcome |
|---|---|
| `runtime-quinn feature` | Implemented or specified with observable acceptance evidence. |
| `application frames` | Implemented or specified with observable acceptance evidence. |
| `mTLS development certificates` | Implemented or specified with observable acceptance evidence. |
| `surface separation` | Implemented or specified with observable acceptance evidence. |
| `bounded streams` | Implemented or specified with observable acceptance evidence. |
| `backpressure` | Implemented or specified with observable acceptance evidence. |

## Acceptance checks

- Admin frame rejected on Application Surface.
- Slow client triggers backpressure.
- Contract probing permission controlled.


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
