# Transaction architecture

> **Status:** Architecture guidance  
> **Audience:** Andromeda maintainers, engine developers, architects, reviewers  
> **Language:** American English  
> **Baseline:** Rust 1.95.0, Rust 2024 Edition, x64 and ARM64 first

## In this article


- Understand the purpose of this architecture area.
- Identify owned responsibilities and forbidden dependencies.
- required invariants.
- core sequence.

## Purpose

Transaction states, isolation policy, MVCC visibility, commit, rollback, WAL coverage, and recovery boundaries.

## Ownership rule

A component must own one clear responsibility and must not silently perform another engine's job.

> [!IMPORTANT]
> Cross-cutting rules are enforced by planes. They do not justify creating a God Engine.


## Required invariants

| Item | Rule |
|---|---|
| `No visible commit without durable WAL.` | Must be represented explicitly in code, tests, documentation, or policy. |
| `Poisoned transactions cannot continue normal execution.` | Must be represented explicitly in code, tests, documentation, or policy. |
| `Rollback is possible after controlled failure.` | Must be represented explicitly in code, tests, documentation, or policy. |
| `Isolation is described by anomalies and mechanisms, not labels alone.` | Must be represented explicitly in code, tests, documentation, or policy. |
| `Every durable mutation is WAL-covered.` | Must be represented explicitly in code, tests, documentation, or policy. |

## Core sequence

| Item | Rule |
|---|---|
| `Created` | Must be represented explicitly in code, tests, documentation, or policy. |
| `Active` | Must be represented explicitly in code, tests, documentation, or policy. |
| `Committing` | Must be represented explicitly in code, tests, documentation, or policy. |
| `Committed` | Must be represented explicitly in code, tests, documentation, or policy. |
| `Failed` | Must be represented explicitly in code, tests, documentation, or policy. |
| `Poisoned` | Must be represented explicitly in code, tests, documentation, or policy. |
| `RollingBack` | Must be represented explicitly in code, tests, documentation, or policy. |
| `RolledBack` | Must be represented explicitly in code, tests, documentation, or policy. |
| `Disposed` | Must be represented explicitly in code, tests, documentation, or policy. |

## Forbidden dependencies

```text
GPU Runtime -> Commit Protocol
Predictive Evidence -> Forced Plan Decision
Application Surface -> Administration Operation
Procedure -> External Network
Procedure -> External Filesystem
Certificate -> Permission Bypass
Replica -> Self Promotion Without Quorum
```

## Acceptance checks

- The document owner is clear.
- Critical invariants are testable.
- Error families are typed.
- Observability requirements are present.
- Recovery behavior is explicit when durable state is affected.
