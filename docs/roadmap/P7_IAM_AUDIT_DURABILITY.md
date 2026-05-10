# P7 — IAM and audit durability

> **Status:** Roadmap phase  
> **Scope:** Sequenced work without calendar promises  
> **Parent:** `ROADMAP.md`

## In this article

- Define the phase purpose.
- State dependencies.
- Define deliverables.
- Define exit criteria and acceptance checks.

## Purpose

Persist security principals, certificates, permissions, policies, and audit evidence.

## Entry criteria

- Previous required phase exit criteria are satisfied.
- Required specifications exist for affected durable or security-sensitive structures.
- Required ADRs exist for architecture-affecting decisions.
- Required tests are designed before implementation is considered complete.

## Deliverables

| Deliverable | Required outcome |
|---|---|
| `UserPrincipal registry` | Implemented or specified with observable acceptance evidence. |
| `CertificateIdentity registry` | Implemented or specified with observable acceptance evidence. |
| `roles` | Implemented or specified with observable acceptance evidence. |
| `permissions` | Implemented or specified with observable acceptance evidence. |
| `surface policy` | Implemented or specified with observable acceptance evidence. |
| `audit ledger` | Implemented or specified with observable acceptance evidence. |
| `break-glass policy` | Implemented or specified with observable acceptance evidence. |

## Acceptance checks

- Disabled principal fails before transaction.
- Permission denied emits durable audit.
- Audit chain detects deletion.


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
