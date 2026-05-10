# Release gates

> **Status:** Testing guidance  
> **Audience:** Maintainers, QA, engine developers, release owners  
> **Baseline:** Rust 1.95.0

## In this article

- Define the purpose of this test area.
- State required evidence.
- Provide acceptance checks.

## Scope

This document applies to Andromeda documentation, implementation planning, and release readiness.

## Required coverage

| Area | Requirement |
|---|---|
| `no unreviewed unsafe` | Must have explicit test evidence or a documented exclusion. |
| `audit clean or exception` | Must have explicit test evidence or a documented exclusion. |
| `deny clean` | Must have explicit test evidence or a documented exclusion. |
| `vet gaps closed or accepted` | Must have explicit test evidence or a documented exclusion. |
| `crash recovery passed` | Must have explicit test evidence or a documented exclusion. |
| `backup restore passed` | Must have explicit test evidence or a documented exclusion. |
| `forensic start passed` | Must have explicit test evidence or a documented exclusion. |
| `release artifacts identified` | Must have explicit test evidence or a documented exclusion. |
| `Backup/PITR/HA/DR` | Must have retained restore drill, PITR, quorum, fencing, promotion, and audit evidence or a documented release exclusion. |


## General rules

- Tests are evidence, not ceremony.
- C5 paths require crash or recovery evidence when durable state is affected.
- Fuzz untrusted or semi-trusted byte parsers.
- Property-test codecs, ordering, hashes, and state machines.
- Do not normalize flaky tests through blind retries.

## Minimum evidence record

```text
TestId
Component
Criticality
Input model
Expected behavior
Observed behavior
TraceId when applicable
RecoveryReport when applicable
Decision
```

## Rejection criteria

Reject release readiness when:

- commit visibility is not crash-tested;
- recovery does not emit RecoveryReport;
- WAL parser has no malformed-input tests;
- RPC payload length is allocated before validation;
- a GPU path has no CPU fallback;
- a security decision lacks audit evidence.
