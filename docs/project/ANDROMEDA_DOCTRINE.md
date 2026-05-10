# Andromeda doctrine

> **Status:** Normative project doctrine  
> **Audience:** Andromeda maintainers, engine developers, architects, reviewers  
> **Language:** American English  
> **Baseline:** Rust 1.95.0, Rust 2024 Edition, x64 and ARM64 first

## In this article


- Define Andromeda's non-negotiable rules.
- Separate strict boundaries from adaptive internals.
- Establish the rejection logic for unsafe or ambiguous features.
- Preserve the enterprise-grade direction of the project.

## Executive summary

Andromeda is a modern relational transactional database engine. It keeps the scientific core of serious RDBMS design: relational modeling, explicit transactions, WAL-driven recovery, cost-based optimization, statistics, normalization, typed contracts, and operational accountability.

The project deliberately rejects ad hoc SQL as the native application surface. Applications call cataloged Procedures through a typed RPC contract.

```text
Client
-> QUIC Application Surface
-> custom typed RPC
-> Procedure contract
-> SRPL execution
-> TransactionScope
-> durable WAL
-> visible commit
-> typed ResultStream
```

## Core doctrine

```text
Strict at the boundaries.
Adaptive inside.
```

Strict boundaries reduce ambiguity before execution. Adaptive internals improve performance under policy.

## Non-negotiable invariants

| ID | Invariant | Criticality |
|---|---|---|
| INV-001 | No ad hoc SQL application surface. | C5 |
| INV-002 | Every application execution goes through a cataloged Procedure. | C5 |
| INV-003 | Every Procedure has a typed, hashed, versioned contract. | C5 |
| INV-004 | Every Procedure is transaction-scoped. | C5 |
| INV-005 | No visible commit without durable WAL. | C5 |
| INV-006 | RAM is never system truth. | C5 |
| INV-007 | System truth is a valid cold snapshot plus durable WAL since that snapshot. | C5 |
| INV-008 | GPU never participates in commit, rollback, WAL, recovery, MVCC visibility, or security-critical authorization. | C5 |
| INV-009 | Predictive evidence never decides alone. | C4 |
| INV-010 | Active plans are tied to `CatalogVersion`, `StatsVersion`, `PolicyVersion`, and `ContractHash`. | C4 |
| INV-011 | Result metadata precedes payload. | C4 |
| INV-012 | Every critical decision is observable and explainable after the fact. | C4 |

## Strict boundaries

| Boundary | Required rule |
|---|---|
| Type System | All critical data shapes are typed before execution. |
| Procedure Contract | Input, output, permissions, read/write sets, cardinality, and protocol layout are contract metadata. |
| Catalog | Published definitions are versioned and immutable for their version. |
| Transaction | Mutations happen inside a transaction scope. |
| WAL | Durable WAL is required before visible commit. |
| Network | Clients use typed RPC frames and Procedure contracts. |
| Security | Certificates prove cryptographic identity, not logical authorization. |
| Import | Catalog changes use validated `DefinitionBatch` operations. |
| Audit | Security and administrative decisions emit audit evidence. |
| Recovery | Durable effects must be replayable or reconstructible. |

## Adaptive internals

| Zone | Allowed adaptation | Guardrail |
|---|---|---|
| Optimizer | Plan choice, join order, access path, plan class. | DecisionTrace, bounded plans, hysteresis. |
| Statistics | Histograms, skew detection, candidate refresh. | Validated `StatsVersion` publication. |
| Procedure Store | Runtime feedback. | Observes and informs; does not decide alone. |
| Hardware | SIMD, NVMe scheduling, GPU batch. | Runtime detection, CPU fallback, policy. |
| Maps | Immediate, incremental, deferred, or snapshot-only refresh. | Explicit consistency policy and budget. |
| Backpressure | Batch shrink, spool, reject, slow producer. | Resource policy and audit. |

## Rejection logic

A feature is rejected from the core when it fails any of these checks:

| Criterion | Required property | Reject when |
|---|---|---|
| Definability | Semantics can be written clearly. | Behavior depends on ambient runtime magic. |
| Determinism | Same input and state produce the same contract result. | It depends on unseeded randomness or physical order. |
| Typing | Shape is known before execution. | It returns shape-shifting records. |
| Boundedness | CPU, memory, I/O, and temp usage are bounded. | It can run without budget. |
| Observability | Effects are traced or measured. | It changes behavior invisibly. |
| Recovery | Durable effects are replayable or reconstructible. | It mutates state outside WAL. |
| Security | IAM and audit can constrain it. | It bypasses surface or permissions. |
| Versioning | It is tied to a stable version. | It changes semantics without a version. |
| Explainability | Decisions can be explained post-incident. | It is a black-box forced decision. |
| Disablement | It can be turned off safely. | It changes commit or storage truth. |

## Design posture

> [!IMPORTANT]
> Andromeda should become modern by reducing ambiguity. It should not become modern by adding unbounded capabilities.
