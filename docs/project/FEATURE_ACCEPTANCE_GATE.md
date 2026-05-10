# Feature acceptance gate

> **Status:** Normative governance  
> **Audience:** Andromeda maintainers, engine developers, architects, reviewers  
> **Language:** American English  
> **Baseline:** Rust 1.95.0, Rust 2024 Edition, x64 and ARM64 first

## In this article


- Decide whether a feature can enter the core engine.
- Classify rejected, sandboxed, and accepted features.
- Provide review questions for ADRs and specifications.

## Gate summary

A feature can enter a C4 or C5 path only when it is definable, deterministic or explicitly bounded, typed, observable, recoverable, secure, versioned, explainable, and disableable.

A feature may be described as implemented only for the scope proven by retained evidence. It must not be described as production-ready, release-ready, operationally safe, or generally available until the validation report names the proof artifacts and commands required by `docs/project/CRITICALITY_MODEL.md`.

At phase level, the minimum proof chain is existing code, targeted tests, trace or audit evidence where the path is observable, and recovery behavior or fail-closed behavior when durable state or an external/security surface is touched.

## Review matrix

| Criterion | Required question | Evidence required | Failure outcome |
|---|---|---|---|
| Definability | Can the behavior be specified precisely? | Spec section and examples. | Reject or research. |
| Determinism | Does the same input and state produce the same result? | Deterministic tests or bounded stochastic policy. | Sandbox. |
| Typing | Is the shape known before execution? | Contract or type descriptor. | Reject from core. |
| Boundedness | Are CPU, memory, I/O, temp, GPU, and network usage bounded? | Resource policy and tests. | Reject from core. |
| Observability | Can its effects be traced and measured? | Trace schema and metrics. | Reject. |
| Recovery | Can durable effects be recovered after crash? | WAL/recovery behavior and crash tests. | Reject from durable paths. |
| Security | Can IAM and audit constrain it? | Admission and audit model. | Reject. |
| Versioning | Is behavior tied to a version? | Version fields and compatibility policy. | Reject. |
| Explainability | Can a reviewer explain the decision after an incident? | DecisionTrace or equivalent. | Sandbox. |
| Disablement | Can the feature be disabled safely? | Kill switch or fallback. | Reject from core. |

## Acceptance classes

| Class | Meaning | Allowed use |
|---|---|---|
| Accepted C5 | Non-negotiable core invariant. | Commit, WAL, recovery, catalog truth, security admission. |
| Accepted C4 | Mission-critical with tests. | Forensic start, backup restore, audit ledger. |
| Accepted C3 | Critical but policy-bound. | Procedure Store, Plan Cache, StatsVersion. |
| Accepted C2 | Important and observable. | SIMD acceleration, Map refresh. |
| Sandbox C1 | Opportunistic and removable. | GPU analytics, benchmark scenarios. |
| Experimental C0 | Research only. | Learned index prototypes, learned optimizer experiments. |
| Rejected | Violates the gate. | Dynamic SQL application surface, GPU commit path, unbounded SRPL loops. |

## Phase acceptance rule

For roadmap phase closure, the acceptance class is not enough. The phase must retain an evidence packet that ties changed files to:

| Evidence item | Required for |
|---|---|
| Source artifact or owner | Every accepted, sandboxed, or rejected feature decision. |
| Normative spec or ADR section | C3, C4, and C5 changes. |
| Targeted tests or deterministic validation command | Every implemented feature claim. |
| Trace, audit, metric, or DecisionTrace artifact | C3 and above, and any feature affecting admission or post-mortem explanation. |
| Crash/recovery or fail-closed evidence | Any durable-state, external-surface, security, admission, backup, restore, HA/DR, WAL, MVCC, catalog, or audit path. |
| Runbook, rollback, or disablement rule | C4/C5 operational paths and C1/C2 optional accelerators. |

If any required evidence is absent, the feature can remain specified, scaffolded, prototyped, or locally validated, but the phase report must keep production readiness blocked.

## Required review output

Every feature review must produce:

```text
Decision
Criticality
Accepted scope
Rejected scope
Required specification changes
Required ADR changes
Required tests
Required runbooks if operational
Fallback or disablement rule
```
