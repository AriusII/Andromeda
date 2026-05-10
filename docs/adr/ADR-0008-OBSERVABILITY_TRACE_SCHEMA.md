# ADR-0008-OBSERVABILITY TRACE SCHEMA — Observability and trace schema

> **Status:** Accepted for V0 documentation baseline  
> **Scope:** Andromeda architecture and implementation governance  
> **Baseline:** Rust 1.95.0

## Context

Andromeda targets an enterprise-grade relational transactional engine with a strict Procedure surface, typed contracts, WAL-first durability, recovery evidence, and bounded adaptive internals.

## Decision

Every critical decision emits structured trace evidence.

Trace evidence is explanatory and correlation-oriented. It is not durable system truth and must not replace WAL, manifests, catalog versions, audit ledger records, recovery reports, security admission results, backup/restore evidence, or HA/DR quorum and fencing evidence.

Generic `DecisionTrace` is non-authoritative. A component may use it to explain why a decision was made, why evidence was ignored or stale, why fallback was selected, or why a feature was disabled. The component still owns the authority for the decision and must retain the owner-specific evidence required by its criticality class.

## Rationale

This decision reduces ambiguity and prevents implementation drift across architecture, code, tests, and operations.

## Consequences

### Positive

- The implementation boundary is explicit.
- Reviewers can reject incompatible shortcuts.
- Tests can be mapped to the decision.
- Operational behavior is easier to explain after an incident.
- Durable truth remains owned by the subsystem that can replay or validate it.

### Negative

- Some implementation shortcuts are intentionally unavailable.
- Additional tests and documentation are required.
- Experimental work must be isolated before entering critical paths.
- Trace-only implementations of security, recovery, commit, catalog publication, backup, restore, or HA/DR claims are rejected.

## Validation

This ADR is validated by:

- a matching specification when the decision affects a technical structure;
- a test plan when the decision affects runtime behavior;
- a runbook when the decision affects operations;
- trace or audit evidence when the decision affects security, durability, or recovery.
- negative tests proving a generic trace cannot satisfy durable, security, recovery, or publication authority.

## Rejection criteria

Reject implementation work that contradicts this decision without a superseding ADR.

Also reject:

- trace as durable truth;
- trace as security authority;
- trace as recovery authority;
- trace as commit, catalog publication, backup, restore, or HA/DR authority;
- unversioned trace schemas for critical decisions;
- secret-bearing trace export.
