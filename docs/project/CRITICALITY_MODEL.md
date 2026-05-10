# Criticality model

> **Status:** Normative classification  
> **Audience:** Andromeda maintainers, engine developers, architects, reviewers  
> **Language:** American English  
> **Baseline:** Rust 1.95.0, Rust 2024 Edition, x64 and ARM64 first

## In this article


- Classify components by risk.
- Define required evidence per criticality level.
- Align implementation, tests, and operations.

## Criticality levels

| Level | Name | Meaning | Evidence required |
|---:|---|---|---|
| C5 | Non-negotiable | Guaranteed by design and not bypassable. | Spec, ADR, crash tests, fuzz/property tests, runbook, audit evidence. |
| C4 | Mission critical | Required for production-grade safety. | Spec, integration tests, recovery tests, operational validation. |
| C3 | Critical | Versioned, audited, and policy-bound. | Spec, metrics, compatibility tests, regression tests. |
| C2 | Important | Measured and budgeted. | Benchmarks, metrics, unit tests, fallback. |
| C1 | Opportunistic | Disableable without correctness impact. | Kill switch, fallback, monitoring. |
| C0 | Experimental | Isolated research. | Explicit sandbox boundary. |

## Example classifications

| Component | Criticality | Reason |
|---|---:|---|
| WAL durable flush before commit | C5 | Commit truth depends on it. |
| Transaction state machine | C5 | Incorrect transitions corrupt visibility. |
| System Database | C5 | Controls users, certificates, policies, and catalog history. |
| CatalogVersion publication | C5 | Contracts and plans depend on stable catalog versions. |
| RecoveryReport | C4 | Required for support, forensic, and automated validation. |
| ForensicStart | C4 | Controls safe investigation mode. |
| Procedure Store | C3 | Critical for diagnostics and optimizer feedback, but not commit truth. |
| Plan Cache | C3 | Impacts performance and stability, not durable truth. |
| SIMD kernels | C2 | Useful when validated; scalar fallback exists. |
| GPU analytics | C1/C2 | Batch accelerator only. |
| Learned indexes | C0/C1 | Research or optional analytics with fallback. |

## Rule

> [!IMPORTANT]
> A lower-criticality subsystem must not control a higher-criticality invariant.

Examples:

```text
GPU runtime -> must not control WAL flush.
Predictive evidence -> must not force commit path behavior.
Map refresh -> must not starve WAL I/O.
Procedure Store -> must not bypass security admission.
```
