# Optimizer and statistics architecture

> **Status:** Architecture guidance  
> **Audience:** Andromeda maintainers, engine developers, architects, reviewers  
> **Language:** American English  
> **Baseline:** Rust 1.95.0, Rust 2024 Edition, x64 and ARM64 first

## In this article


- Understand the purpose of this architecture area.
- Identify owned responsibilities and forbidden dependencies.
- required invariants.
- cost terms.

## Purpose

Cost model, StatsVersion, Procedure Store feedback, ScenarioEvidence, PlanClass, and learned-component boundaries.

## Ownership rule

A component must own one clear responsibility and must not silently perform another engine's job.

> [!IMPORTANT]
> Cross-cutting rules are enforced by planes. They do not justify creating a God Engine.


## Required invariants

| Item | Rule |
|---|---|
| `Statistics are versioned.` | Must be represented explicitly in code, tests, documentation, or policy. |
| `ScenarioEvidence is non-authoritative.` | Must be represented explicitly in code, tests, documentation, or policy. |
| `PlanCacheKey includes ProcedureId, ContractHash, CatalogVersion, StatsVersion, PolicyVersion, PlanClass, and shape fingerprint.` | Must be represented explicitly in code, tests, documentation, or policy. |
| `Learned components require fallback.` | Must be represented explicitly in code, tests, documentation, or policy. |
| `DecisionTrace records candidates and rejection reasons.` | Must be represented explicitly in code, tests, documentation, or policy. |

## Cost terms

| Item | Rule |
|---|---|
| `CpuCost` | Must be represented explicitly in code, tests, documentation, or policy. |
| `LogicalIoCost` | Must be represented explicitly in code, tests, documentation, or policy. |
| `PhysicalIoCost` | Must be represented explicitly in code, tests, documentation, or policy. |
| `WalCost` | Must be represented explicitly in code, tests, documentation, or policy. |
| `TempCost` | Must be represented explicitly in code, tests, documentation, or policy. |
| `NetworkCost` | Must be represented explicitly in code, tests, documentation, or policy. |
| `RiskPenalty` | Must be represented explicitly in code, tests, documentation, or policy. |

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
