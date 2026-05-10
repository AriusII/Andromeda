# Fuzzing plan

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
| `srpl_parser` | Must have explicit test evidence or a documented exclusion. |
| `rpc_frame` | Must have explicit test evidence or a documented exclusion. |
| `structured_object` | Must have explicit test evidence or a documented exclusion. |
| `wal_record` | Must have explicit test evidence or a documented exclusion. |
| `page_header` | Must have explicit test evidence or a documented exclusion. |
| `manifest` | Must have explicit test evidence or a documented exclusion. |
| `segment_index` | Must have explicit test evidence or a documented exclusion. |
| `contract_hash` | Must have explicit test evidence or a documented exclusion. |


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
