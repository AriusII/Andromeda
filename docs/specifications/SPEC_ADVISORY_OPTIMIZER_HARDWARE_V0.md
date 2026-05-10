# Advisory Optimizer And Hardware

> **Status:** Normative V0 specification
> **Audience:** Andromeda maintainers, engine developers, architects, reviewers
> **Language:** American English
> **Baseline:** Rust 1.95.0, Rust 2024 Edition, x64 and ARM64 first

## In this article

- Define advisory optimizer and hardware evidence boundaries.
- State PlanCacheKey, ScenarioEvidence, statistics, Map refresh, and GPU publication gates.
- Preserve the rule that advisory evidence never becomes system truth by itself.

## Purpose

Define the V0 boundary for optimizer, statistics, Map analytics, ScenarioEvidence, and optional
hardware acceleration outputs. These surfaces can influence plan selection or candidate publication
only through bounded, versioned, traceable gates.

## Plan cache runtime gate

`PlanCacheKey v0` is exactly this identity tuple:

```text
`ProcedureId`
`ContractHash`
`CatalogVersion`
`StatsVersion`
`PolicyVersion`
`PlanClass`
`PlanShapeFingerprint`
```

Cache hits require exact key equality and matching key digest.

| Gate | Rejection rule |
|---|---|
| Advisory evidence count | Reject more than 8 scenario evidence records. |
| Plan class | Reject unbounded or caller-defined plan classes. |
| Version identity | Reject missing contract, catalog, stats, or policy version fields. |
| Traceability | Reject plan selection without CriticalDecisionTrace evidence. |

## Map refresh validation

## Validation gates

Map refresh and Map-derived statistics are advisory unless a typed owner publication makes their
role explicit. Validation must compare current catalog stats against `StatsVersion` and
`CatalogVersion`, reject staleness, prove summarizability at the requested grain, and require
durable publication before an active statistics switch.

Required rejection criteria:

- Reject analytics treated as truth.
- Reject missing decision trace.
- Reject stale `StatsVersion` or stale `CatalogVersion`.
- Reject Map refresh output that lacks grain and summarizability proof.
- Reject active statistics publication without durable publication evidence.

Map tests must prove grain, summarizability, staleness, durable publication, advisory-only
consumption, and trace evidence.

## Hardware execution boundary

GPU and specialized hardware work is optional, cancelable, outside commit, outside rollback,
outside WAL, outside recovery, and outside security-critical authorization. Hardware output can
produce candidate statistics or advisory evidence only after CPU-verifiable validation.

## Acceptance summary

This specification is acceptable when implementation, tests, and documentation can prove that
optimizer and hardware evidence is bounded, versioned, traceable, and never treated as durable
truth without owner publication.
