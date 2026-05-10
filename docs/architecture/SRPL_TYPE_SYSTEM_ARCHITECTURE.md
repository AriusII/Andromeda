# SRPL and Type System architecture

> **Status:** Architecture guidance  
> **Audience:** Andromeda maintainers, engine developers, architects, reviewers  
> **Language:** American English  
> **Baseline:** Rust 1.95.0, Rust 2024 Edition, x64 and ARM64 first

## In this article


- Understand the purpose of this architecture area.
- Identify owned responsibilities and forbidden dependencies.
- required invariants.
- compiler stages.

## Purpose

Strict Relational Procedure Language, explicit absence, set semantics, cardinality, bounded loops, and typed contracts.

## Ownership rule

A component must own one clear responsibility and must not silently perform another engine's job.

> [!IMPORTANT]
> Cross-cutting rules are enforced by planes. They do not justify creating a God Engine.


## Required invariants

| Item | Rule |
|---|---|
| `Set semantics are the default.` | Must be represented explicitly in code, tests, documentation, or policy. |
| `Ambient NULL is rejected.` | Must be represented explicitly in code, tests, documentation, or policy. |
| `Cardinality is explicit.` | Must be represented explicitly in code, tests, documentation, or policy. |
| `Loops are bounded.` | Must be represented explicitly in code, tests, documentation, or policy. |
| `Procedure execution is transaction-scoped.` | Must be represented explicitly in code, tests, documentation, or policy. |
| `SRPL lowers into stable semantic IR.` | Must be represented explicitly in code, tests, documentation, or policy. |

## Compiler stages

| Item | Rule |
|---|---|
| `Lexer/Parser` | Must be represented explicitly in code, tests, documentation, or policy. |
| `AST` | Must be represented explicitly in code, tests, documentation, or policy. |
| `Binder` | Must be represented explicitly in code, tests, documentation, or policy. |
| `Type/cardinality/effect validation` | Must be represented explicitly in code, tests, documentation, or policy. |
| `Semantic IR` | Must be represented explicitly in code, tests, documentation, or policy. |
| `Physical plan candidates` | Must be represented explicitly in code, tests, documentation, or policy. |
| `Costing` | Must be represented explicitly in code, tests, documentation, or policy. |
| `Plan Cache` | Must be represented explicitly in code, tests, documentation, or policy. |

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
