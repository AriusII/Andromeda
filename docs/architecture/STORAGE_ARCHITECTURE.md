# Storage architecture

> **Status:** Architecture guidance  
> **Audience:** Andromeda maintainers, engine developers, architects, reviewers  
> **Language:** American English  
> **Baseline:** Rust 1.95.0, Rust 2024 Edition, x64 and ARM64 first

## In this article


- Understand the purpose of this architecture area.
- Identify owned responsibilities and forbidden dependencies.
- required invariants.
- physical families.

## Purpose

Segmented storage, WAL, HotStore, ColdStore, manifests, pages, SegmentIndex, backup, and recovery behavior.

## Ownership rule

A component must own one clear responsibility and must not silently perform another engine's job.

> [!IMPORTANT]
> Cross-cutting rules are enforced by planes. They do not justify creating a God Engine.


## Required invariants

| Item | Rule |
|---|---|
| `System truth is a valid cold snapshot plus durable WAL.` | Must be represented explicitly in code, tests, documentation, or policy. |
| `ColdStore is immutable after publication.` | Must be represented explicitly in code, tests, documentation, or policy. |
| `A file is trusted only when referenced by a valid manifest and verified.` | Must be represented explicitly in code, tests, documentation, or policy. |
| `SegmentIndex enables startup without full cold scan.` | Must be represented explicitly in code, tests, documentation, or policy. |
| `Temp and spill are never truth.` | Must be represented explicitly in code, tests, documentation, or policy. |

## Physical families

| Item | Rule |
|---|---|
| `.androot` | Must be represented explicitly in code, tests, documentation, or policy. |
| `.andmf` | Must be represented explicitly in code, tests, documentation, or policy. |
| `.andwal` | Must be represented explicitly in code, tests, documentation, or policy. |
| `.andhot` | Must be represented explicitly in code, tests, documentation, or policy. |
| `.andcold` | Must be represented explicitly in code, tests, documentation, or policy. |
| `.andidx` | Must be represented explicitly in code, tests, documentation, or policy. |
| `.andaudit` | Must be represented explicitly in code, tests, documentation, or policy. |

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
