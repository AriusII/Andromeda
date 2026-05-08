# DEC-041: Security Contract Boundary for Lot 5.4A

**Date:** 2026-05-08  
**Status:** Accepted  
**Category:** Architecture / Security Contract Boundary  
**Related:** Lot 5.4, WR-5.4A-4, DEC-018, DEC-021, DEC-033, DEC-040

## Purpose

This decision defines the bounded security contract extraction for Lot 5.4A.
It records the architectural boundary for `andromeda-security-contract` without
modifying transport adapters, mutable IAM state, durable storage, WAL, or
recovery code.

The decision keeps security contract types deterministic, reviewable, and
runtime-free. It also closes a known documentation gap from earlier decisions:
surface or permission compatibility must be expressed through explicit semantic
mappings, not through ordinal comparison such as `as u8` casts.

## Scope

This decision applies to future Lot 5.4A security contract extraction and review
work for:

- runtime-free security contract types;
- explicit surface, permission, and operation mappings;
- Application, Administration, and Cluster or HA/DR surface separation;
- documentation and review gates that prevent ordinal enum comparisons;
- deferral boundaries for full IAM and `PrincipalRegistry` ownership;
- the initial runtime-free `andromeda-security-contract` crate.

This work item is classified as an important architecture security boundary
change. The initial crate provides contract vocabulary and tests only; it does
not perform runtime authorization.

## Non-goals

This decision does not:

- move code between crates;
- migrate full IAM state from `andromeda-core`, `andromeda-observe`, or any
  runtime crate;
- replace the current `PrincipalRegistry` implementation;
- define full IAM persistence, policy management, certificate rotation, or
  principal lifecycle workflows;
- change RPC wire formats, Protobuf contracts, WAL records, storage formats, or
  recovery behavior;
- introduce Administration, backup, restore, failover, promotion, quorum,
  fencing, WAL shipping, or other HA/DR capability onto the Application surface.

## Decision

Lot 5.4A accepts the following security contract boundary.

| Rule | Decision | Validation intent |
| --- | --- | --- |
| Runtime-free ownership | `andromeda-security-contract` must remain a runtime-free contract boundary. It may define stable security contract types, identifiers, and semantic mapping helpers, but it must not depend on Quinn, Rustls, Tokio, listener runtimes, storage engines, WAL, recovery, executor dispatch, mutable IAM stores, or concrete certificate extraction. | Default builds and contract tests must run without network runtime features. |
| Explicit mappings | Surface, permission, operation, and certificate-scope compatibility must be expressed through explicit `match`-based mappings or named tables. Each variant must have a named semantic mapping. | Adding a new variant must fail review or tests until its mapping is updated intentionally. |
| No ordinal comparison | Security decisions must not compare enum variants through ordinal casts such as `surface as u8 == certificate.surface as u8`. Ordinal values may not define authorization, surface compatibility, operation routing, or audit acceptance. | Static review and targeted tests must reject `as u8`-based security compatibility checks. |
| Application surface restriction | The Application surface may carry only typed, cataloged Procedure invocation and contract-read traffic. It must not carry Administration or HA/DR operations directly or through generic tunnels. | Application dispatch tests must reject Administration, backup, restore, cluster, quorum, fencing, and WAL-shipping routes. |
| IAM deferral | Full IAM and durable `PrincipalRegistry` ownership are deferred. Lot 5.4A may document contract boundaries and mapping requirements, but it must not claim final policy-store, revocation-store, role-management, or principal-lifecycle behavior. | Future IAM work must open its own decision or implementation work order. |

The `andromeda-security-contract` boundary is allowed to define contract-safe
security vocabulary, including surface scopes, permission families, operation
classes, compatibility mapping functions, stable labels, and explicit
codec-facing discriminants. It is not allowed to perform runtime authorization
against mutable principal state or make transport-specific certificate decisions.

When two contract concepts must be compared, the comparison must use a named
semantic function. Examples include mapping a listener plane to a required
surface scope, mapping an operation to a required permission, or checking whether
a surface permits a permission family. These functions must be total over the
known enum variants and must reject unknown, unsupported, or unmapped values when
the input comes from disk or the network.

This decision supersedes any example or local implementation pattern that treats
enum declaration order as security evidence. Enum declaration order is an
implementation detail and is not a security contract.

## Procedure

Use this procedure for future work that implements or reviews the Lot 5.4A
security contract boundary.

1. Confirm that the work item is contract-only before modifying
   `andromeda-security-contract`.
2. List every security enum, operation class, permission family, and surface
   concept that crosses a crate, disk, network, audit, or test boundary.
3. Define explicit mappings for each crossing. Use named `match` arms or tables
   with one row per variant.
4. Reject any security compatibility check that depends on enum ordinal order,
   numeric cast equality, or range comparison.
5. Keep Application, Administration, and Cluster or HA/DR routing separate.
   Application routing must terminate before any Administration or HA/DR command
   can be dispatched.
6. Keep the contract boundary independent from Quinn, Rustls, Tokio, listener
   lifecycles, executor dispatch, storage truth, WAL, recovery, and mutable IAM
   state.
7. Treat full IAM and durable `PrincipalRegistry` ownership as deferred work.
   Open a separate decision or implementation order before adding policy-store,
   role-store, revocation-store, or lifecycle-management behavior.
8. Add targeted tests or static checks with the implementation work item. This
   documentation record does not provide executable validation by itself.

## Validation

Validation for Lot 5.4A is targeted:

- Check that `andromeda-security-contract` is runtime-free and has no normal
  dependencies in the initial extraction.
- Check that no transport runtime, protocol wire format, WAL, storage, or
  recovery behavior is modified.
- Check that the decision explicitly marks `andromeda-security-contract` as
  runtime-free.
- Check that explicit mappings are required for surface, permission, operation,
  and certificate-scope compatibility.
- Check that ordinal comparison through `as u8` is rejected for security
  decisions.
- Check that the Application surface cannot transport Administration or HA/DR
  capability.
- Check that full IAM and durable `PrincipalRegistry` ownership remain deferred.

The minimum expected gates are:

- a dependency-topology test proving the security contract boundary has no Quinn,
  Rustls, Tokio, executor, WAL, storage, recovery, or mutable IAM-store
  dependency;
- a source or lint check that rejects `as u8` ordinal comparisons in security
  compatibility code;
- mapping coverage tests that fail when a new surface, permission, or operation
  variant is added without an explicit mapping;
- Application-surface admission tests that reject Administration and HA/DR
  operations before dispatch.

## References

- DEC-018: mTLS Identity Extraction and Binding
- DEC-021: Protobuf Schema Contract for Frame and Result Stream Compatibility
- DEC-033: Durable Audit Ledger
- DEC-040: RPC, QUIC, and Security Boundary Governance for Lot 5.1
- `AGENTS.md`: Andromeda Codex Operating Instructions
- `crates/README.md`: Rust Source Layout boundary rules
