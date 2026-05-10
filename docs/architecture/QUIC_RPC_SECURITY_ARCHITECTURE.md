# QUIC, RPC, and security architecture

> **Status:** Architecture guidance  
> **Audience:** Andromeda maintainers, engine developers, architects, reviewers  
> **Language:** American English  
> **Baseline:** Rust 1.95.0, Rust 2024 Edition, x64 and ARM64 first

## In this article


- Understand the purpose of this architecture area.
- Identify owned responsibilities and forbidden dependencies.
- required invariants.
- surfaces.

## Purpose

Transport separation, typed frames, surfaces, admission, IAM, audit, and backpressure.

## Ownership rule

A component must own one clear responsibility and must not silently perform another engine's job.

> [!IMPORTANT]
> Cross-cutting rules are enforced by planes. They do not justify creating a God Engine.


## Required invariants

| Item | Rule |
|---|---|
| `QUIC is transport; Andromeda RPC is semantics.` | Must be represented explicitly in code, tests, documentation, or policy. |
| `Application, Administration, and HA/DR surfaces are separated.` | Must be represented explicitly in code, tests, documentation, or policy. |
| `Admission validates surface, identity, contract, payload, and budget before transaction creation.` | Must be represented explicitly in code, tests, documentation, or policy. |
| `mTLS certificate is not the logical user.` | Must be represented explicitly in code, tests, documentation, or policy. |
| `Audit cannot be globally disabled.` | Must be represented explicitly in code, tests, documentation, or policy. |

## Surfaces

| Item | Rule |
|---|---|
| `Application Surface` | Must be represented explicitly in code, tests, documentation, or policy. |
| `Administration Surface` | Must be represented explicitly in code, tests, documentation, or policy. |
| `HA/DR Cluster Surface` | Must be represented explicitly in code, tests, documentation, or policy. |

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
