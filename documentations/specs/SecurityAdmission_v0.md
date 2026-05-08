# SecurityAdmission v0 Specification

## Purpose

Define the accepted documentation contract for `SecurityAdmission v0`, the
pre-transaction admission boundary for RPC and administrative requests that can
reach Andromeda execution or privileged operations.

`SecurityAdmission v0` is a contract boundary. It is not a claim that durable
IAM, policy-management runtime, certificate lifecycle management, revocation
workflow, or Admin RPC audit read behavior is complete.

## Scope

This specification applies to admission documentation for Application,
Administration, Monitoring, BackupAgent, and Cluster or HA/DR surfaces where a
request must be authenticated, bound to a surface, authorized, resource-checked,
and auditable before dispatch.

It covers:

- protocol and frame evidence;
- surface-plane evidence;
- typed Procedure contract evidence;
- certificate and principal evidence;
- policy and permission evidence;
- resource budget evidence;
- audit correlation evidence;
- fail-closed behavior before transaction creation.

## Current Implementation Status

Lot 5 implements a runtime-free admission vocabulary in
`andromeda-security-contract`: stable step, evidence, outcome, reason, and
surface-boundary codes plus explicit surface-to-permission-family mapping. This
crate is not a full admission packet and does not yet encode every target
evidence row in this specification.

Current QUIC route admission produces typed pre-transaction route errors for
protocol, frame, Protobuf, catalog, and surface failures. Those failures do not
carry IAM authorization evidence because no principal policy decision was
reached. IAM authorization decisions can carry `PrincipalAuthorizationEvidence`
for durable audit consumers.

Future implementation work must add an end-to-end `SecurityAdmission` evidence
packet if every pre-IAM rejection must be represented as a durable admission
decision.

## Non-goals

This specification does not:

- create a durable IAM store, policy store, role store, revocation store, or
  principal lifecycle workflow;
- replace the current `PrincipalRegistry` implementation;
- move IAM state between crates;
- define certificate parsing or concrete mTLS runtime behavior;
- introduce gRPC, runtime JSON defaults, ad hoc SQL, generic command text, or
  untyped payload tunnels;
- expose Administration or HA/DR operations through the Application surface;
- claim an implemented Admin RPC audit read endpoint;
- make audit ledger records database truth;
- start a transaction before admission succeeds.

## Prerequisites

Reviewers must use these references before accepting changes to this spec:

- DEC-018 for mTLS identity extraction and session binding.
- DEC-021 for Protobuf message-only schemas and error classification.
- DEC-033 for durable audit ledger evidence and no global audit disable.
- DEC-040 for RPC, QUIC, IAM, audit, and surface-plane separation.
- DEC-041 for runtime-free security contract vocabulary and explicit mappings.
- ADR-0011 for crate boundary rules.
- `documentations/specs/FrameHeader_RPC_v0.md` for frame validation
  preconditions.
- `documentations/specs/AuditLedger_v0.md` for audit evidence boundaries.

## Procedure

### Admission order

Admission must fail closed before transaction creation. The accepted v0 order is:

1. Validate the RPC frame header, protocol version, payload kind, and stream role.
2. Reject gRPC, runtime JSON defaults, generic command tunnels, and ad hoc SQL
   application surfaces.
3. Bind the request to exactly one listener surface: Application,
   Administration, Monitoring, BackupAgent, or Cluster or HA/DR.
4. Validate the requested operation is allowed on that surface.
5. Resolve certificate identity evidence when mTLS identity is available.
6. Check certificate status, surface scope, and session binding.
7. Resolve principal evidence and principal status.
8. Resolve policy version and available role, group, and direct permission
   evidence.
9. Evaluate explicit deny rules.
10. Evaluate required allow rules for the operation and surface.
11. Validate Procedure contract evidence when the request invokes a Procedure.
12. Validate resource budget evidence.
13. Bind audit correlation evidence for accepted decisions and IAM authorization
    denials. Route, protocol, catalog, and surface rejections must remain typed,
    sanitized, pre-transaction failures and may not carry IAM evidence.
14. Return an accepted admission token or a typed rejection with
    `NoTransaction` effect.

Explicit deny wins over allow unless a formally specified break-glass policy
authorizes a bounded exception and emits stronger audit evidence.

### Required evidence

The full target `SecurityAdmission` runtime is complete only when the request
can be represented by bounded, typed evidence. The current Lot 5 Rust contract
is accepted as the stable code vocabulary and boundary map for this target; it
intentionally does not expose all fields in this table.

| Evidence | Required fields | Acceptance rule |
| --- | --- | --- |
| Protocol evidence | Protocol version, frame type, payload kind, request id, session id, stream role | Must pass before payload dispatch. |
| Surface evidence | Listener surface, requested surface scope, operation family | Must not cross Application, Administration, Monitoring, or Cluster or HA/DR planes. |
| Contract evidence | Procedure identity, `ContractHash`, `CatalogVersion`, `StatsVersion` when required, `PolicyVersion` when required | Required for Procedure invocation. |
| Principal evidence | Certificate identity when available, principal id, principal status, presented fingerprint | Must fail closed on missing required identity, disabled principal, revoked certificate, or surface mismatch. |
| Permission evidence | Required permission family, direct and role-derived permission evidence, explicit deny outcome | Must be evaluated through explicit mappings, not enum ordinal comparison. |
| Resource evidence | Payload length, rows, duration, temp bytes, memory or stream budget as applicable | Must reject unsupported or over-budget requests before transaction creation. |
| Audit evidence | Trace id, request/session correlation, decision outcome, rejection reason when denied | Required for IAM decisions and durable-decision paths. Pre-IAM route, protocol, catalog, and surface rejections expose typed sanitized error and correlation evidence rather than IAM authorization evidence. |

### Surface rules

The Application surface may invoke typed, cataloged Procedures and read allowed
contract metadata. It must not carry Administration, backup, restore, failover,
promotion, quorum, fencing, WAL shipping, forensic startup, or security
management operations.

Administration, Monitoring, BackupAgent, and Cluster or HA/DR surfaces require
their own authorization policies and must not be tunneled through Application
routing. In the V0 vocabulary, the `Backup` admission boundary maps to the
`BackupAgent` security surface, while Cluster or HA/DR maps to the `Cluster`
security surface.

### Security contract boundary

`andromeda-security-contract` may define surface names, permission families,
operation classes, stable labels, and explicit semantic mappings. It must not be
described as runtime authorization, durable IAM storage, mutable principal
registry ownership, policy management, revocation, certificate extraction,
audit-ledger ownership, QUIC/TLS runtime behavior, WAL, storage, or recovery.

Security compatibility must use explicit mappings. `SecurityAdmissionBoundaryV0`
currently exposes `Application`, `Administration`, `Cluster`, `Backup`, and
`Monitoring`; `Backup` is the admission name for the `BackupAgent` surface. Do
not compare surface, permission, operation, or certificate-scope compatibility
through enum ordinals, numeric casts, or range checks.

### Admission result

When full runtime admission is implemented, accepted admission returns a
bounded token or equivalent evidence packet that can be consumed by the next
layer. The token must prove:

- the surface was allowed;
- the principal and policy evidence matched the request;
- the Procedure contract evidence matched the request when a Procedure is
  invoked;
- resource admission passed;
- audit correlation was established.

Current Lot 5 route admission does not produce this packet for every rejection.
Protocol, frame, Protobuf, catalog, and surface rejections are typed
`ProcedureRouteAdmissionError` failures with no transaction. IAM decisions may
include principal authorization evidence.

Rejected admission returns a typed failure. The failure must indicate that no
transaction was created. Rejection details must not leak secrets.

## Validation

Documentation acceptance checks:

- The spec cross-references DEC-040 and DEC-041.
- The spec marks `SecurityAdmission v0` as a pre-transaction contract boundary.
- The spec does not claim full durable IAM, policy-management runtime,
  revocation store, certificate lifecycle workflow, or Admin RPC audit read
  implementation.
- The spec keeps Application, Administration, Monitoring, BackupAgent, and
  Cluster or HA/DR surfaces separate.
- The spec requires explicit security mappings and rejects ordinal comparisons.
- The spec keeps audit evidence distinct from database truth.
- The spec states that the current Lot 5 Rust contract is a runtime-free
  vocabulary subset, not a complete admission evidence packet.

Future implementation work should add targeted validation for:

```powershell
cargo test -p andromeda-core --test resource_policy_gates
cargo test -p andromeda-quic --test procedure_gateway_route
cargo test -p andromeda-exec --test remote_invoke_network_e2e
```

These commands are not required for WR5-DOC documentation-only acceptance.

## Troubleshooting

| Symptom | Likely cause | Corrective action |
| --- | --- | --- |
| Transaction was created for a denied request. | Admission ran too late or failure was not fail-closed. | Move rejection before transaction creation and return `NoTransaction`. |
| Application route carries an admin operation. | Surface separation was bypassed. | Reject at surface admission and require Administration-surface authorization. |
| Disabled principal reaches dispatch. | Principal status evidence was not enforced. | Fail closed before Procedure or privileged operation dispatch. |
| Security contract crate is described as IAM runtime. | DEC-041 boundary drift. | Reword as runtime-free vocabulary and explicit mappings. |
| Rejection leaks a secret-bearing value. | Error details copied raw credential evidence. | Replace with typed reason codes and sanitized correlation identifiers. |
| Audit evidence is described as database truth. | DEC-033 or DEC-040 boundary drift. | Reword as forensic evidence and preserve cold snapshot plus durable WAL truth. |

## References

- `docs/adr/ADR-0012-quic-rpc-no-grpc.md`
- `documentations/governance/decisions/DEC-018-mtls-identity-extraction.md`
- `documentations/governance/decisions/DEC-021-protobuf-schema-contract.md`
- `documentations/governance/decisions/DEC-033-durable-audit-ledger.md`
- `documentations/governance/decisions/DEC-040-rpc-quic-security-boundary.md`
- `documentations/governance/decisions/DEC-041-security-contract-boundary.md`
- `documentations/specs/FrameHeader_RPC_v0.md`
- `documentations/specs/AuditLedger_v0.md`
- `crates/andromeda-security-contract/src/lib.rs`
- `crates/andromeda-core/tests/resource_policy_gates.rs`
- `crates/andromeda-quic/tests/procedure_gateway_route/route_admission.rs`
