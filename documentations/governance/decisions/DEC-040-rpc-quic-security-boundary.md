# DEC-040: RPC, QUIC, and Security Boundary Governance for Lot 5.1

**Date:** 2026-05-08  
**Status:** Accepted  
**Category:** Architecture / Boundary Governance  
**Related:** Lot 5.0, Lot 5.1, DEC-017, DEC-018, DEC-021, DEC-033

## Purpose

This decision defines the Lot 5.1 governance boundary for RPC, QUIC runtime,
IAM, audit, and surface-plane separation before any crate extraction work begins.
It records dependency rules that keep the abstract RPC protocol independent from
the concrete QUIC/Quinn runtime and keep storage truth independent from network
runtime concerns.

This decision is governance-only. It does not introduce runtime behavior, change
wire formats, add dependencies, or start Lot 5.2 or Lot 5.3 extraction.

## Scope

This decision applies to Lot 5.0 and Lot 5.1 planning for:

- the abstract RPC protocol contract;
- the QUIC/Quinn runtime integration boundary;
- IAM identity, authorization, and audit handoff boundaries;
- Application, Administration, and Cluster or HA/DR surface separation;
- dependency direction rules for future extraction work.

## Non-goals

This decision does not:

- create or move Rust crates;
- add Quinn, Rustls, Protobuf, IAM, audit, or storage code;
- define executable listener, stream-manager, IAM, or audit implementations;
- change existing DEC records;
- authorize Lot 5.2 or Lot 5.3 extraction;
- introduce any new application-facing surface.

## Decision

Lot 5.1 establishes boundary governance only. Future extraction work must keep
the abstract RPC protocol, concrete QUIC runtime, IAM/audit functions, and
surface planes separated by responsibility and dependency direction.

The abstract RPC protocol is the contract layer. It may define typed procedure
invocation envelopes, result metadata, error classification, cancellation,
backpressure, and stream sequencing rules. It must not depend on Quinn, Rustls,
socket runtimes, listener lifecycles, certificate loading, or executor-specific
types.

The QUIC runtime is the concrete transport adapter. Quinn may appear only behind
the approved runtime boundary and must remain replaceable behind the abstract RPC
contract. Runtime code may translate between QUIC streams and typed Andromeda RPC
frames, but it must not define Procedure semantics, bypass cataloged contracts,
or become a storage, WAL, recovery, catalog publication, or IAM policy authority.

IAM and audit boundaries remain policy and evidence boundaries. Authentication,
certificate identity binding, authorization decisions, and audit emission must
happen before dispatch reaches transactional execution. Audit output is evidence,
not truth; durable database truth remains the latest valid cold snapshot plus
durable WAL from that snapshot.

The canonical authorization model for Lot 5 is `andromeda-core::principal`.
The `PrincipalRegistry` authorization path owns certificate status, principal
status, surface scope, policy version, role permissions, direct permissions, and
authorization evidence. `andromeda-observe` owns trace schemas, audit envelopes,
query models, and durable audit evidence. Runtime bridges may project a core
authorization result into observe events, but they must not re-evaluate a second,
weaker permission model after core authorization has denied a request.

Application, Administration, and Cluster or HA/DR surfaces remain separate. The
Application surface may invoke typed, cataloged Procedures only. It must not
expose Administration, backup, restore, failover, promotion, quorum, fencing,
WAL shipping, or other HA/DR capabilities.

The following constraints are binding:

- No gRPC surface, dependency, compatibility mode, or generated gRPC service is
  allowed.
- No runtime JSON default is allowed for RPC payloads or transport envelopes.
- No application-facing ad hoc SQL, generic command text, dynamic table names, or
  dynamic predicates are allowed.
- No caller may bypass typed Procedure contracts.
- No commit may become visible before durable WAL.
- The abstract RPC protocol must not depend on Quinn.
- WAL, storage, and recovery crates must not depend on RPC runtime code, Quinn,
  Rustls, socket runtimes, or listener implementations.
- The Application surface must not expose Administration or HA/DR capabilities.

## Dependency rules

Future Lot 5 extraction must follow these dependency rules:

| Area | May depend on | Must not depend on |
| --- | --- | --- |
| Abstract RPC protocol | core identifiers, typed contracts, protocol envelopes, explicit codecs, observable error/result models | Quinn, Rustls, socket runtimes, listener implementations, storage engines, WAL managers |
| QUIC/Quinn runtime | abstract RPC protocol, QUIC stream mapping, runtime feature gates, certificate extraction adapters | storage truth, WAL commit authority, Procedure definition authority, Administration operations through Application routing |
| IAM authorization | core principal registry, certificate and principal status, surface scope, policy versions, role/direct permissions | post-denial re-evaluation through observe-only models, ad hoc SQL, untyped command payloads |
| Audit evidence | audit event contracts, correlation identifiers, durable audit records, replayable evidence projections | storage-native struct layouts, GPU or benchmark output as truth, policy decisions that bypass core authorization |
| WAL, storage, and recovery | explicit storage codecs, durable WAL records, recovery manifests, cold snapshot metadata | RPC runtime crates, Quinn, Rustls, listener lifecycles, network session state |
| Application surface | typed, cataloged Procedure contracts and bounded RPC result streams | Administration commands, HA/DR commands, backup/restore, failover, quorum, fencing, WAL shipping |
| Administration and Cluster or HA/DR surfaces | explicit administrative or cluster contracts and their own authorization policies | Application routing shortcuts or generic command tunnels |

Dependency direction must preserve testability. Protocol and policy contract
tests must be able to run without enabling a concrete QUIC runtime.

## Migration procedure

When Lot 5.2 or Lot 5.3 extraction is later authorized, the coordinator must:

1. Open a separate work order that names the target extraction lot and ownership
   set.
2. Map existing protocol, QUIC runtime, IAM, audit, Application, Administration,
   and Cluster or HA/DR artifacts to the dependency rules in this decision.
3. Extract the abstract RPC protocol before wiring concrete Quinn runtime
   dependencies into the runtime adapter.
4. Keep Quinn and Rustls behind a runtime-owned boundary and a non-default runtime
   feature when executable integration is needed.
5. Prove that WAL, storage, recovery, catalog publication, and transactional
   commit paths do not import RPC runtime or Quinn symbols.
6. Prove that Application routing cannot dispatch Administration or HA/DR
   operations.
7. Add or update tests in the extraction work item. This DEC does not provide
   executable tests by itself.

## Validation

For this governance-only change, validation is documentary:

- Check that the decision preserves Andromeda invariants for no gRPC, no runtime
  JSON default, no ad hoc SQL, typed Procedure contracts, durable WAL before
  visible commit, and surface-plane separation.
- Check that the decision does not introduce code, dependencies, runtime behavior,
  wire-format changes, or extraction work.
- Check that the decision explicitly prevents abstract RPC protocol dependency on
  Quinn and prevents WAL, storage, and recovery dependency on RPC runtime or
  Quinn.

Future extraction work must add executable validation for dependency graph rules,
feature gates, plane routing, IAM authorization, audit correlation, and
crash/recovery behavior where storage truth or commit visibility is affected.

## Residual risks

- Existing crates may already contain mixed protocol/runtime responsibilities that
  require careful mapping before extraction.
- Dependency drift can reappear if future work adds Quinn, Rustls, or runtime
  symbols to shared protocol, WAL, storage, or recovery crates.
- Surface separation can be weakened by convenience routing unless Application,
  Administration, and Cluster or HA/DR dispatch tests are added during extraction.
- Audit evidence can be mistaken for database truth unless future documentation and
  tests continue to distinguish audit traces from durable WAL and cold snapshots.

## References

- DEC-017: QUIC Runtime Dependency
- DEC-018: mTLS Identity Extraction and Binding
- DEC-021: Protobuf Schema Contract for Frame and Result Stream Compatibility
- DEC-033: Durable Audit Ledger
- AGENTS.md: Andromeda Codex Operating Instructions
