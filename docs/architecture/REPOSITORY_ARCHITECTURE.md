# Repository architecture

> **Status:** Architecture guidance  
> **Audience:** Andromeda maintainers, engine developers, architects, reviewers  
> **Language:** American English  
> **Baseline:** Rust 1.95.0, Rust 2024 Edition, x64 and ARM64 first

## In this article


- Understand the purpose of this architecture area.
- Identify owned responsibilities and forbidden dependencies.
- current-style crate groups.
- workspace rules.

## Purpose

Cargo workspace governance, crate responsibilities, documentation placement, tests, tooling, and supply-chain controls.

## Ownership rule

A component must own one clear responsibility and must not silently perform another engine's job.

> [!IMPORTANT]
> Cross-cutting rules are enforced by planes. They do not justify creating a God Engine.


## Current-style crate groups

| Item | Rule |
|---|---|
| `catalog` | Must be represented explicitly in code, tests, documentation, or policy. |
| `srpl` | Must be represented explicitly in code, tests, documentation, or policy. |
| `proto` | Must be represented explicitly in code, tests, documentation, or policy. |
| `quic` | Must be represented explicitly in code, tests, documentation, or policy. |
| `exec` | Must be represented explicitly in code, tests, documentation, or policy. |
| `tx` | Must be represented explicitly in code, tests, documentation, or policy. |
| `storage` | Must be represented explicitly in code, tests, documentation, or policy. |
| `observe` | Must be represented explicitly in code, tests, documentation, or policy. |
| `bench` | Must be represented explicitly in code, tests, documentation, or policy. |
| `cli` | Must be represented explicitly in code, tests, documentation, or policy. |
| `core` | Must be represented explicitly in code, tests, documentation, or policy. |

## Workspace rules

| Item | Rule |
|---|---|
| `Rust 1.95.0 baseline` | Must be represented explicitly in code, tests, documentation, or policy. |
| `Rust 2024 Edition` | Must be represented explicitly in code, tests, documentation, or policy. |
| `resolver = 3` | Must be represented explicitly in code, tests, documentation, or policy. |
| `no global target-cpu=native` | Must be represented explicitly in code, tests, documentation, or policy. |
| `supply-chain tooling from first release gate` | Must be represented explicitly in code, tests, documentation, or policy. |

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
