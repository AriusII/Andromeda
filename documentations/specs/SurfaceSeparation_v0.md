# SurfaceSeparation v0 Specification

## Purpose

Define the accepted contract for separating Andromeda public operation surfaces
before Procedure dispatch, administrative dispatch, HA/DR dispatch, recovery
dispatch, or forensic startup.

`SurfaceSeparation v0` is a boundary specification. It is not a claim that every
Administration, HA/DR, backup, restore, or forensic runtime endpoint is
implemented. It defines what must be rejected when a request attempts to enter
the wrong surface.

The central rule is simple: the Application surface may invoke typed, cataloged
Procedures and read allowed contract metadata only. It must not route
Administration, HA/DR, backup, restore, forensic startup, security management,
quorum, fencing, promotion, or WAL-shipping operations.

## Scope

This specification applies to:

- QUIC and RPC route admission before Procedure dispatch;
- certificate surface-scope checks;
- security-contract surface and permission vocabulary;
- Procedure manifest permission checks;
- HA/DR stream namespace partitioning;
- recovery and forensic startup documentation;
- audit and trace evidence that records rejected surface crossings.

It covers:

- Application, Administration, Cluster or HA/DR, BackupAgent, Forensic, and
  Monitoring work classes;
- accepted permission families for each surface;
- fail-closed behavior before transaction creation;
- the minimum validation evidence required before accepting surface changes.

## Current Implementation Status

The current repository has these boundary artifacts:

- `documentations/specs/SecurityAdmission_v0.md` defines pre-transaction
  admission and states that Application must not carry privileged operations.
- `crates/andromeda-security-contract` defines runtime-free surface,
  permission, and `SurfaceClass` vocabulary. `ForensicStart` is a recovery
  permission, not an application permission.
- `andromeda-core::principal::SurfaceScope` denies Administration and HA/DR
  permissions on the Application scope before role or direct permission grants.
- `andromeda-quic::ProcedureGateway::bind_application_procedure_route` admits
  only Application-plane Procedure routes and rejects non-Application request
  scopes, non-Application planes, non-Application manifest permissions, and
  HA/DR-reserved stream ids before dispatch.

This specification adds acceptance text for the combined Admin, HA/DR, and
forensic boundary. It does not add a durable IAM store, Admin RPC audit read
endpoint, HA/DR runtime, backup runtime, restore runtime, or forensic repair
runtime.

## Non-goals

This specification does not:

- introduce application-facing ad hoc SQL, generic command text, dynamic table
  names, dynamic predicates, or untyped command tunnels;
- bypass typed Procedure contracts;
- claim full durable IAM or policy-management runtime completion;
- claim an implemented Admin RPC audit read endpoint;
- define Quinn, Rustls, socket, or listener runtime behavior;
- define HA/DR quorum, promotion, fencing, WAL-shipping, backup, restore, or
  forensic startup runtime behavior;
- authorize forensic startup to repair, rewrite, compact, or publish inspected
  database state;
- make audit records, RAM state, temp files, GPU output, or benchmark output
  database truth;
- create a transaction before surface admission succeeds.

## Prerequisites

Reviewers must use these references before accepting changes to this spec:

- `AGENTS.md` for Andromeda non-negotiable invariants.
- `crates/AGENTS.md` for crate-scope Rust rules.
- `docs/adr/ADR-0012-quic-rpc-no-grpc.md` for QUIC/RPC documentation
  acceptance.
- `documentations/governance/decisions/DEC-040-rpc-quic-security-boundary.md`
  for RPC, QUIC, IAM, audit, and surface-plane separation.
- `documentations/governance/decisions/DEC-041-security-contract-boundary.md`
  for runtime-free security vocabulary.
- `documentations/specs/SecurityAdmission_v0.md` for admission order and
  security evidence.
- `documentations/specs/ProcedureContract_v0.md` for typed Procedure contract
  requirements.
- `documentations/specs/RecoveryReport_v0.md` for forensic startup constraints.

## Procedure

### Surface model

Each request must bind to exactly one surface before it can reach a dispatcher.

| Surface or work class | Allowed role | Application-route status |
| --- | --- | --- |
| Application | Invoke typed, cataloged Procedures and read allowed contract metadata. | Allowed only when frame, manifest, contract, stream id, certificate scope, and permission evidence all match Application. |
| Administration | Manage security, catalog publication controls, administrative recovery controls, certificate operations, and bounded operator actions. | Forbidden. Must use Administration admission and authorization. |
| Cluster or HA/DR | Promotion, fencing, quorum, replica membership, cluster manifest updates, and WAL-shipping coordination. | Forbidden. Must use Cluster or HA/DR admission and HA/DR stream namespaces. |
| BackupAgent or recovery | Backup, restore, PITR validation, and recovery-agent actions. | Forbidden. Must use BackupAgent or an explicitly authorized Administration path. |
| Forensic | Forensic startup and inspection-only evidence gathering. | Forbidden. Application traffic remains blocked while forensic mode is effective. |
| Monitoring | Diagnostics and read-only telemetry. | Forbidden for Procedure dispatch unless separately admitted as read-only metadata or diagnostic behavior. |

### Application route gates

An Application Procedure route is valid only when all of the following are true:

1. The listener or connection plane is Application.
2. The certificate identity has Application surface scope.
3. The request `surface_scope` is `application`.
4. The request is an `RpcExecuteRequest` for a typed, cataloged Procedure.
5. The Procedure manifest includes the canonical Application execute
   permission, `andromeda.execute_procedure`.
6. The request carries matching `ContractHash`, `CatalogVersion`, and
   `StatsVersion` evidence.
7. The stream id is in the V0 Application namespace and not in the HA/DR
   reserved namespace.
8. No transaction id is supplied by the client.
9. IAM authorization, when reached, evaluates the Application scope and the
   required Procedure execute permission.

Failure at gates 1 through 8 is a route, protocol, contract, or surface
rejection before IAM authorization evidence is produced. Failure at gate 9 is an
authorization rejection with audit-ready authorization evidence.

### Privileged operation rules

The following operation families must never be represented as Application
Procedure routes:

| Operation family | Examples | Required boundary |
| --- | --- | --- |
| Administration | security management, certificate rotation, role management, catalog publication controls, administrative shutdown | Administration |
| HA/DR | cluster promotion, node fencing, quorum, membership publication, cluster manifest updates, WAL shipping | Cluster or HA/DR |
| Backup and restore | backup, restore, PITR validation, restore acceptance | BackupAgent or explicitly authorized Administration |
| Forensic | forensic startup, corruption inspection, forensic hold, read-only evidence collection | Forensic or explicitly authorized Administration inspection path |
| Security management | revocation, certificate lifecycle, policy management, break-glass authorization | Administration |

No Procedure manifest may use privileged permissions to make a privileged
operation appear application-routable. If a cataloged Procedure needs to expose
application business behavior, it must use application permissions and must not
perform administrative, recovery, HA/DR, or forensic control work.

### Forensic behavior

Forensic startup is inspection-only. It must block Application-surface traffic,
transaction creation, catalog publication, HA/DR promotion, administrative
repair actions, WAL append, checkpoint publication, manifest switching, index
publication, map publication, and statistics publication unless a separate
Administration-surface policy explicitly authorizes a bounded operation outside
the inspected database truth.

Forensic evidence may be written only to an allowed external report sink or an
isolated audit path that does not mutate the inspected database truth.

### HA/DR stream namespace

V0 reserves stream ids `128..=255` for HA/DR control. Application Procedure
routes must reject those stream ids before Procedure dispatch. Future or
reserved stream ids outside the V0 Application namespace must also fail closed
until a reviewed surface contract admits them.

## Validation

Documentation acceptance checks:

- The spec states that Application routes carry typed, cataloged Procedures
  only.
- The spec excludes Administration, HA/DR, backup, restore, forensic startup,
  security management, quorum, fencing, promotion, and WAL shipping from the
  Application surface.
- The spec distinguishes route rejections before IAM from IAM authorization
  denials.
- The spec keeps forensic evidence distinct from database truth.
- The spec does not claim full durable IAM, Admin RPC audit reads, HA/DR
  runtime, backup runtime, restore runtime, or forensic repair runtime.
- The spec does not introduce gRPC, runtime JSON defaults, ad hoc SQL, or
  untyped payload tunnels.

Rust validation for this boundary:

```powershell
cargo test -p andromeda-rpc-protocol --test surface_separation_contract --locked
cargo test -p andromeda-quic --test procedure_gateway_route --locked surface_separation -- --nocapture
```

The QUIC command requires the modular test root to include
`procedure_gateway_route/surface_separation.rs`.

## Troubleshooting

| Symptom | Likely cause | Corrective action |
| --- | --- | --- |
| Application route accepts a privileged permission family. | Manifest permission admission is too broad. | Require `andromeda.execute_procedure` with the Application family and reject privileged families before dispatch. |
| Administration command appears as a Procedure invocation. | Privileged operation was tunneled through Application routing. | Move the operation to Administration admission with explicit authorization and audit evidence. |
| HA/DR operation uses an Application stream id. | Stream namespace partitioning was bypassed. | Route HA/DR through the reserved HA/DR namespace and reject Application dispatch. |
| Forensic startup allows application traffic. | Forensic mode was treated as recovery success. | Block Application traffic and emit forensic evidence without publishing repaired truth. |
| Rejection carries IAM evidence even though frame or surface admission failed. | IAM was evaluated after an invalid route shape. | Return a typed route, protocol, contract, or surface rejection before IAM authorization. |
| Audit evidence is described as database truth. | Audit or forensic records were overclaimed. | Reword as forensic or authorization evidence and cite cold snapshot plus durable WAL truth. |

## References

- `AGENTS.md`
- `crates/AGENTS.md`
- `docs/adr/ADR-0012-quic-rpc-no-grpc.md`
- `documentations/governance/decisions/DEC-040-rpc-quic-security-boundary.md`
- `documentations/governance/decisions/DEC-041-security-contract-boundary.md`
- `documentations/specs/SecurityAdmission_v0.md`
- `documentations/specs/ProcedureContract_v0.md`
- `documentations/specs/RecoveryReport_v0.md`
- `crates/andromeda-security-contract/src/admission.rs`
- `crates/andromeda-security-contract/src/surface.rs`
- `crates/andromeda-security-contract/src/permission.rs`
- `crates/andromeda-core/src/principal/surface_scope.rs`
- `crates/andromeda-quic/src/procedure_gateway/route.rs`
- `crates/andromeda-quic/src/hadr_streams.rs`
