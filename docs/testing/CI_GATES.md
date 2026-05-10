# CI gates

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
| `cargo fmt` | Must have explicit test evidence or a documented exclusion. |
| `cargo check` | Must have explicit test evidence or a documented exclusion. |
| `cargo test` | Must have explicit test evidence or a documented exclusion. |
| `cargo clippy` | Must have explicit test evidence or a documented exclusion. |
| `cargo nextest` | Must have explicit test evidence or a documented exclusion. |
| `cargo test --doc` | Must have explicit test evidence or a documented exclusion. |
| `cargo audit` | Must have explicit test evidence or a documented exclusion. |
| `cargo deny` | Must have explicit test evidence or a documented exclusion. |
| `cargo vet` | Must have explicit test evidence or a documented exclusion. |
| `Miri` | Must have explicit test evidence or a documented exclusion. |
| `cargo-fuzz` | Must have explicit test evidence or a documented exclusion. |
| `storage/recovery gate` | Must have explicit test evidence or a documented exclusion. |


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
