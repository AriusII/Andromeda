# Contract and Language Engine

> **Status:** Architecture guidance  
> **Audience:** Andromeda maintainers, engine developers, architects, reviewers  
> **Language:** American English  
> **Baseline:** Rust 1.95.0, Rust 2024 Edition, x64 and ARM64 first

## In this article


- Understand the purpose of this architecture area.
- Identify owned responsibilities and forbidden dependencies.
- required invariants.
- owned artifacts.

## Purpose

Catalog, types, Procedure contracts, SRPL, StructuredObjects, Maps, and DefinitionBatch publication.

## Ownership rule

A component must own one clear responsibility and must not silently perform another engine's job.

> [!IMPORTANT]
> Cross-cutting rules are enforced by planes. They do not justify creating a God Engine.


## Required invariants

| Item | Rule |
|---|---|
| `Published catalog versions are immutable.` | Must be represented explicitly in code, tests, documentation, or policy. |
| `ContractHash is computed from canonical contract shape, not raw SRPL text.` | Must be represented explicitly in code, tests, documentation, or policy. |
| `SRPL optional values require branch handling.` | Must be represented explicitly in code, tests, documentation, or policy. |
| `No dynamic SQL text exists in the native application surface.` | Must be represented explicitly in code, tests, documentation, or policy. |
| `DefinitionBatch publication is transactional and WAL-covered.` | Must be represented explicitly in code, tests, documentation, or policy. |

## Owned artifacts

| Item | Rule |
|---|---|
| `CatalogObjectModel` | Must be represented explicitly in code, tests, documentation, or policy. |
| `TypeSystem` | Must be represented explicitly in code, tests, documentation, or policy. |
| `ProcedureContract` | Must be represented explicitly in code, tests, documentation, or policy. |
| `SRPL Grammar` | Must be represented explicitly in code, tests, documentation, or policy. |
| `SRPL Binder` | Must be represented explicitly in code, tests, documentation, or policy. |
| `Semantic IR` | Must be represented explicitly in code, tests, documentation, or policy. |
| `DefinitionBatch` | Must be represented explicitly in code, tests, documentation, or policy. |
| `CompatibilityPolicy` | Must be represented explicitly in code, tests, documentation, or policy. |

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
