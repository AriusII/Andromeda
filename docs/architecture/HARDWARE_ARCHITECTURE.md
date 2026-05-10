# Hardware architecture

> **Status:** Architecture guidance  
> **Audience:** Andromeda maintainers, engine developers, architects, reviewers  
> **Language:** American English  
> **Baseline:** Rust 1.95.0, Rust 2024 Edition, x64 and ARM64 first

## In this article


- Understand the purpose of this architecture area.
- Identify owned responsibilities and forbidden dependencies.
- required invariants.
- hardware stack.

## Purpose

CPU, GPU, RAM, NVMe, SSD, HDD, and hardware policy boundaries.

## Ownership rule

A component must own one clear responsibility and must not silently perform another engine's job.

> [!IMPORTANT]
> Cross-cutting rules are enforced by planes. They do not justify creating a God Engine.


## Required invariants

| Item | Rule |
|---|---|
| `Runtime feature detection is required.` | Must be represented explicitly in code, tests, documentation, or policy. |
| `Scalar fallback exists for SIMD kernels.` | Must be represented explicitly in code, tests, documentation, or policy. |
| `CPU fallback exists for GPU jobs.` | Must be represented explicitly in code, tests, documentation, or policy. |
| `GPU jobs are cancelable.` | Must be represented explicitly in code, tests, documentation, or policy. |
| `NVMe WAL I/O outranks analytics and maintenance.` | Must be represented explicitly in code, tests, documentation, or policy. |
| `HDD is ColdStore, not commit path.` | Must be represented explicitly in code, tests, documentation, or policy. |

## Hardware stack

| Item | Rule |
|---|---|
| `CPU x64/ARM64` | Must be represented explicitly in code, tests, documentation, or policy. |
| `GPU analytics/statistics/vector workloads` | Must be represented explicitly in code, tests, documentation, or policy. |
| `RAM working set` | Must be represented explicitly in code, tests, documentation, or policy. |
| `NVMe/SSD WAL and hot store` | Must be represented explicitly in code, tests, documentation, or policy. |
| `HDD cold snapshots` | Must be represented explicitly in code, tests, documentation, or policy. |

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
