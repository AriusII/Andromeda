# ADR-0017-NO DYNAMIC SQL APPLICATION SURFACE — No dynamic SQL application surface

> **Status:** Accepted for V0 documentation baseline  
> **Scope:** Andromeda architecture and implementation governance  
> **Baseline:** Rust 1.95.0, Rust 2024 Edition, resolver 3, 89 crates

## Context

Andromeda targets an enterprise-grade relational transactional engine with a strict Procedure surface, typed contracts, WAL-first durability, recovery evidence, and bounded adaptive internals.

## Decision

Reject ad hoc SQL as a native application API.

Application execution must enter through cataloged Procedures with typed, hashed, versioned contracts. Dynamic SQL strings, generic query endpoints, SQL shells that execute application mutations, CLI bypasses, transport handlers, benchmark harnesses, or diagnostic tools must not become alternate application surfaces.

SRPL is not a dynamic SQL exception. SRPL work must still publish through Procedure contracts, catalog versions, admission, transaction scope, WAL durability where mutations are visible, and typed ResultStream output.

## Surface proof rule

Changes touching application execution, CLI entrypoints, RPC handlers, SRPL execution, benchmarks, or demos must retain evidence for:

| Claim | Required evidence |
|---|---|
| Procedure-only execution | Tests or review evidence that entrypoints require cataloged Procedure identity and `ContractHash`. |
| No ad hoc SQL bypass | Source/topology review or targeted tests showing no string-based SQL execution path was introduced. |
| Transaction scope | Runtime or integration tests showing Procedure execution is transaction-scoped where state changes are visible. |
| Durable mutation safety | WAL/recovery evidence for visible durable effects. |
| Security and audit | Admission and audit/trace evidence for external or privileged execution paths. |

A parser, CLI command, demo, or benchmark may be useful local evidence, but it is not production readiness and must not be described as a native application API.

## Rationale

This decision reduces ambiguity and prevents implementation drift across architecture, code, tests, and operations.

## Consequences

### Positive

- The implementation boundary is explicit.
- Reviewers can reject incompatible shortcuts.
- Tests can be mapped to the decision.
- Operational behavior is easier to explain after an incident.

### Negative

- Some implementation shortcuts are intentionally unavailable.
- Additional tests and documentation are required.
- Experimental work must be isolated before entering critical paths.

## Validation

This ADR is validated by:

- Procedure contract, admission, execution, WAL/recovery, and entrypoint tests for changed application paths;
- a matching specification when the decision affects a technical structure;
- a test plan when the decision affects runtime behavior;
- a runbook when the decision affects operations;
- trace or audit evidence when the decision affects security, durability, or recovery.

## Rejection criteria

Reject implementation work that contradicts this decision without a superseding ADR.

Reject changes that accept unversioned SQL strings as application requests or route application mutations around Procedure contracts, admission, transaction scope, WAL evidence, or audit evidence.
