# Observability architecture

> **Status:** Architecture guidance  
> **Audience:** Andromeda maintainers, engine developers, architects, reviewers  
> **Language:** American English  
> **Baseline:** Rust 1.95.0, Rust 2024 Edition, x64 and ARM64 first

## In this article


- Understand the purpose of this architecture area.
- Identify owned responsibilities and forbidden dependencies.
- required traces.
- required fields.

## Purpose

Traces, metrics, audit evidence, RecoveryReport, DecisionTrace, Procedure Store evidence, and post-incident explainability.

## Ownership rule

A component must own one clear responsibility and must not silently perform another engine's job.

> [!IMPORTANT]
> Cross-cutting rules are enforced by planes. They do not justify creating a God Engine.


## Required traces

| Item | Rule |
|---|---|
| `ProcedureInvocationTrace` | Must be represented explicitly in code, tests, documentation, or policy. |
| `TransactionTrace` | Must be represented explicitly in code, tests, documentation, or policy. |
| `PlanDecisionTrace` | Must be represented explicitly in code, tests, documentation, or policy. |
| `CatalogChangeTrace` | Must be represented explicitly in code, tests, documentation, or policy. |
| `DefinitionBatchApplyTrace` | Must be represented explicitly in code, tests, documentation, or policy. |
| `SecurityAuditTrace` | Must be represented explicitly in code, tests, documentation, or policy. |
| `AdminOperationTrace` | Must be represented explicitly in code, tests, documentation, or policy. |
| `RecoveryTrace` | Must be represented explicitly in code, tests, documentation, or policy. |
| `ClusterEventTrace` | Must be represented explicitly in code, tests, documentation, or policy. |

## Required fields

| Item | Rule |
|---|---|
| `TraceId` | Must be represented explicitly in code, tests, documentation, or policy. |
| `InvocationId` | Must be represented explicitly in code, tests, documentation, or policy. |
| `PrincipalId` | Must be represented explicitly in code, tests, documentation, or policy. |
| `ProcedureId` | Must be represented explicitly in code, tests, documentation, or policy. |
| `ContractHash` | Must be represented explicitly in code, tests, documentation, or policy. |
| `CatalogVersion` | Must be represented explicitly in code, tests, documentation, or policy. |
| `PolicyVersion` | Must be represented explicitly in code, tests, documentation, or policy. |
| `Result` | Must be represented explicitly in code, tests, documentation, or policy. |
| `ErrorKind` | Must be represented explicitly in code, tests, documentation, or policy. |

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
