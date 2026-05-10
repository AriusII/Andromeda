# ADR-0007-QUIC RPC BOUNDARY NO GRPC — QUIC/RPC boundary and no gRPC

> **Status:** Accepted for V0 documentation baseline  
> **Scope:** Andromeda architecture and implementation governance  
> **Baseline:** Rust 1.95.0, Rust 2024 Edition, resolver 3, 89 crates

## Context

Andromeda targets an enterprise-grade relational transactional engine with a strict Procedure surface, typed contracts, WAL-first durability, recovery evidence, and bounded adaptive internals.

## Decision

Use QUIC as transport and custom typed RPC as semantics; do not expose gRPC as the application surface.

QUIC owns transport behavior only. Procedure identity, `ContractHash`, `CatalogVersion`, admission, authorization, transaction scope, WAL durability, and ResultStream semantics remain owned by their engine crates and contracts.

Custom Protobuf payloads are allowed as typed payload contracts under Andromeda frame and Procedure semantics. Generated gRPC services, tonic service surfaces, REST/JSON application APIs, or ad hoc request handlers must not become the native application surface.

## Boundary proof rule

RPC and transport changes must retain evidence for:

| Claim | Required evidence |
|---|---|
| No gRPC application surface | Dependency/topology review or targeted tests showing no gRPC service surface owns application execution. |
| Typed Procedure invocation | Protocol or execution tests binding request frames to Procedure identity and contract metadata. |
| ResultStream ordering | Tests proving metadata precedes payload and completion/error frames remain typed. |
| External-surface safety | Admission, authorization, bounds, and malformed-frame rejection evidence for changed paths. |

Transport liveness alone is not release evidence. A QUIC server or protocol smoke test does not prove production readiness unless the retained evidence covers admission, security, recovery impact, and operational behavior for the changed C4/C5 path.

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

- RPC/protocol specifications, typed frame tests, malformed-frame rejection tests, and admission/security evidence for external surfaces;
- a matching specification when the decision affects a technical structure;
- a test plan when the decision affects runtime behavior;
- a runbook when the decision affects operations;
- trace or audit evidence when the decision affects security, durability, or recovery.

## Rejection criteria

Reject implementation work that contradicts this decision without a superseding ADR.

Reject changes that introduce gRPC, REST/JSON, or transport-owned Procedure semantics as the native application API.
