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

## Phase evidence rule

Every roadmap phase must state which criticality level its changed path touches and must retain phase evidence before the phase can be closed. Evidence is retained only when it names the source artifact and validation command or review record that produced it.

Minimum retained evidence is:

| Changed path | Minimum phase evidence |
|---|---|
| C5 durable, catalog, transaction, security, or recovery truth | Code owner, normative spec or ADR, targeted tests, crash/recovery behavior where durable state is touched, audit or trace evidence, and runbook or rollback evidence where operational. |
| C4 mission-critical runtime or operations | Code owner, specification, integration or recovery tests, operational validation record, and trace or audit evidence. |
| C3 policy, compatibility, diagnostics, optimizer, or procedure evidence | Specification, version binding, compatibility or regression tests, metrics or DecisionTrace evidence. |
| C2 measured subsystem | Unit tests or benchmarks, metrics, resource budget, and fallback evidence. |
| C1 optional or advisory subsystem | Disablement rule, fallback or isolation boundary, and monitoring evidence if enabled. |
| C0 research | Explicit sandbox boundary and a statement that the work is not on a production path. |

Passing a generic workspace command is not enough by itself for C4/C5 closure. The retained evidence must prove the invariant named by the phase, including recovery or fail-closed behavior when the phase touches durable state, external surfaces, admission, security, or audit.

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

No document, roadmap phase, README, validation report, benchmark, or demo may claim production readiness, release readiness, or operational safety for a C4/C5 path unless the retained evidence above exists and is referenced. A crate that compiles, a feature that is scaffolded, or a demo that passes is evidence of local implementation only, not production readiness.

Examples:

```text
GPU runtime -> must not control WAL flush.
Predictive evidence -> must not force commit path behavior.
Map refresh -> must not starve WAL I/O.
Procedure Store -> must not bypass security admission.
```
