# SecurityAdmissionCanonicalOrder v0 Specification

## Purpose

Define the accepted documentation contract for
`SecurityAdmissionCanonicalOrder v0`, the canonical order for pre-dispatch
security admission in Andromeda.

The canonical order is:

1. typed identity;
2. surface classification;
3. contract lookup;
4. permission evaluation;
5. admission decision;
6. durable and audit trace.

Security admission must fail closed before Procedure dispatch, privileged
operation dispatch, transaction creation, catalog publication, WAL-visible
intent, or HA/DR action.

## Scope

This specification applies to documentation and future implementation work that
mentions authentication, authorization, surface admission, Procedure invocation
admission, administrative admission, HA/DR admission, backup admission,
monitoring admission, forensic admission, or durable security-audit evidence.

It covers:

- the canonical admission order;
- evidence required at each step;
- Application Surface rejection of Administration, HA/DR, backup, restore,
  forensic, and security-management operations;
- typed Procedure contract lookup before permission evaluation;
- fail-closed admission decisions with no transaction effect;
- durable audit or trace evidence for security decisions.

## Current Implementation Status

`SecurityAdmission v0` already defines the broader pre-transaction admission
contract, stable vocabulary, surface mappings, evidence categories, and current
implementation boundaries. This specification narrows the ordering rule for
security admission. It does not claim that every runtime path already emits a
complete end-to-end `SecurityAdmission` packet or a durable audit record for
every pre-IAM rejection.

Protocol and frame validation remain required preconditions. A malformed frame,
unsupported protocol version, invalid payload kind, or untyped tunnel is
rejected before security admission can begin. Those failures still have
`NoTransaction` effect. Once a bounded typed request exists, security admission
must follow the canonical order in this specification.

## Non-goals

This specification does not:

- replace `SecurityAdmission v0`;
- define a durable IAM store, policy store, role store, certificate lifecycle
  system, or revocation workflow;
- define mTLS packet parsing, QUIC runtime behavior, or a concrete certificate
  authority implementation;
- expose Administration, HA/DR, backup, restore, security management, or
  forensic operations through the Application Surface;
- allow ad hoc SQL, generic command text, dynamic table names, dynamic
  predicates, shape-shifting returns, runtime JSON defaults, gRPC, or untyped
  payload tunnels;
- allow permission evaluation before typed identity, surface classification, or
  contract lookup;
- make audit evidence database truth;
- place audit appends on the transaction commit path.

## Prerequisites

Reviewers must use these references before accepting changes to this spec:

- `AGENTS.md` for Procedure-only application behavior, durable WAL before
  visible commit, and surface separation invariants.
- `documentations/specs/SecurityAdmission_v0.md` for pre-transaction admission
  evidence and stable security vocabulary.
- `documentations/specs/ProcedureContract_v0.md` for contract identity,
  required permissions, `ContractHash`, `CatalogVersion`, `StatsVersion`, and
  `PolicyVersion`.
- `documentations/specs/AuditLedger_v0.md` for durable security decision
  evidence and forensic-only replay.
- `documentations/specs/FrameHeader_RPC_v0.md` for frame validation
  preconditions.
- `documentations/04_QUIC_RPC_SECURITY_HADR_OPERATIONS.md` for Application,
  Administration, and HA/DR surface separation.
- DEC-018 for mTLS identity extraction and session binding.
- DEC-033 for durable audit ledger evidence.
- DEC-040 for RPC, QUIC, IAM, audit, and surface-plane separation.
- DEC-041 for runtime-free security contract vocabulary.

## Procedure

### Canonical precondition

The security admission order begins only after the RPC, QUIC, or local caller
boundary has produced a bounded typed request envelope. Pre-admission protocol
rejection must not create a transaction and must not invoke permission logic.

The typed request envelope must include or reference:

- request id and session id;
- listener or caller boundary;
- requested operation kind;
- Procedure identity or privileged operation identity when present;
- payload kind and bounded payload shape evidence;
- correlation id or trace id when available.

### Step 1: Typed identity

Admission must first bind the request to typed identity evidence.

| Evidence | Required rule |
| --- | --- |
| Certificate identity | Resolve the mTLS or equivalent `CertificateIdentity` when the surface requires it. |
| Session binding | Prove the request belongs to the authenticated session and presented certificate scope. |
| Principal identity | Resolve the `UserPrincipal`, service principal, node principal, backup agent, or monitoring principal. |
| Principal status | Fail closed for disabled, expired, revoked, unknown, or mismatched principals. |
| Break-glass status | Allow only through a bounded policy with stronger audit evidence and no permanent bypass. |

Identity failure returns a typed denial with `NoTransaction` effect. Permission
rules must not run against an anonymous or partially typed principal unless the
surface explicitly defines a bounded unauthenticated operation such as a minimal
hello or health exchange.

### Step 2: Surface classification

Admission must classify the request into exactly one surface after identity is
typed.

Accepted v0 surfaces and operation-family classifications are:

| Surface | Allowed purpose | Application routing rule |
| --- | --- | --- |
| Application | Business Procedure invocation and allowed contract metadata reads. | May execute typed cataloged Procedures only. |
| Administration | DefinitionBatch import, catalog administration, Procedure Store, backup and restore control, debug, certificates, IAM, policies, and operational maintenance. | Must not be tunneled through Application. |
| Monitoring | Bounded health, metrics, and status reads. | Must remain read-only and policy-scoped. |
| BackupAgent | Backup, backup validation, and retention-agent work. | Must not be tunneled through Application. |
| Cluster or HA/DR | WAL shipping, quorum, fencing, manifests, health, membership, promotion, and replica coordination. | Must not be tunneled through Application. |
| Forensic operation family | Forensic startup, hold, read-only inspection, corruption-boundary investigation, and incident evidence preservation where supported by an Administration or explicitly documented recovery control path. | Must not be tunneled through Application. |

The Application Surface must reject:

- DefinitionBatch import, catalog mutation, object creation, map creation, and
  Procedure creation;
- Administration operations;
- backup, restore, PITR, and retention operations;
- HA/DR operations, including WAL shipping, quorum, fencing, membership,
  promotion, demotion, and replica control;
- forensic startup, forensic hold management, raw page inspection, raw WAL
  inspection, corruption-boundary investigation, and recovery inspection;
- security management, certificate management, IAM policy management,
  principal management, and break-glass administration;
- deep Procedure Store inspection and debug execution.

Surface mismatch returns a typed denial with `NoTransaction` effect. It must not
fall through to another surface, proxy the operation, or reinterpret the
payload.

### Step 3: Contract lookup

Admission must resolve the typed contract or operation descriptor before
permission evaluation.

For Procedure invocation, contract lookup must bind:

| Evidence | Required rule |
| --- | --- |
| Procedure identity | Nonzero Procedure identity or qualified Procedure reference accepted by the catalog. |
| `ContractHash` | Must match the current or explicitly accepted compatible Procedure contract. |
| `CatalogVersion` | Must identify the catalog version used for lookup. |
| `StatsVersion` | Must be included when required by the Procedure contract binding. |
| `PolicyVersion` | Must be included or derivable from the contract and policy evidence. |
| Required permissions | Must be read from the contract binding, not from client-provided text. |
| Result shape | Must be the typed ResultStream shape declared by the contract. |

For Administration, Monitoring, BackupAgent, Cluster or HA/DR, and forensic
operation families, contract lookup must resolve an equivalent typed operation
descriptor owned by that surface or control path. The descriptor must define
operation identity, required permission families, resource class, audit family,
and allowed effect.

Contract lookup failure returns a typed denial with `NoTransaction` effect. The
failure must not leak secret catalog, policy, certificate, or object details.

### Step 4: Permission evaluation

Admission must evaluate permission after typed identity, surface, and contract
or operation descriptor are known.

Permission evaluation must use explicit semantic mappings. It must not compare
surface, permission, operation, role, or certificate-scope compatibility through
enum ordinals, numeric casts, string prefixes, or range checks.

Evaluation must include:

- policy version;
- required permission families from the Procedure contract or operation
  descriptor;
- principal direct permissions;
- role-derived and group-derived permissions;
- certificate scope;
- surface scope;
- explicit deny rules;
- break-glass policy if applicable;
- resource, quota, and rate limits that are part of security policy;
- audit family and retention requirement for the decision.

Explicit deny wins over allow unless a formally specified break-glass policy
authorizes a bounded exception. Break-glass admission must still bind typed
identity, surface, contract or operation descriptor, permission evidence, and
stronger audit evidence.

Permission denial returns a typed rejection with `NoTransaction` effect.

### Step 5: Admission decision

Admission must produce exactly one terminal decision:

| Decision | Required effect |
| --- | --- |
| `Accept` | Return a bounded admission token or equivalent evidence packet for the next layer. |
| `DenyIdentity` | Reject before surface dispatch with `NoTransaction`. |
| `DenySurface` | Reject before contract or operation dispatch with `NoTransaction`. |
| `DenyContract` | Reject before Procedure or privileged operation dispatch with `NoTransaction`. |
| `DenyPermission` | Reject before Procedure or privileged operation dispatch with `NoTransaction`. |
| `DenyPolicy` | Reject before Procedure or privileged operation dispatch with `NoTransaction`. |
| `DenyResource` | Reject before Procedure or privileged operation dispatch with `NoTransaction`. |
| `DenyAuditUnavailable` | Reject when the owning path requires durable audit evidence before decision visibility and cannot produce it. |

An accepted admission token must include:

- principal identity and status evidence;
- surface classification;
- Procedure contract binding or typed operation descriptor;
- required permission evidence;
- policy version;
- resource or quota decision when applicable;
- audit correlation id;
- `NoPrivilegeEscalation` and surface-scope evidence.

Transaction creation, Procedure dispatch, privileged operation dispatch, HA/DR
action, catalog publication, or recovery-visible intent may occur only after an
accepted admission decision and after required durable or audit trace handling
has succeeded.

### Step 6: Durable and audit trace

Security decisions must produce bounded trace evidence. IAM authorization
acceptance and denial paths that the owning implementation classifies as
durable security decisions must also write durable audit evidence before the
decision becomes visible to dispatch.

Audit evidence must include:

| Evidence | Required rule |
| --- | --- |
| Correlation | Trace id, request id, session id, and invocation or operation id when available. |
| Identity | Principal id, certificate fingerprint or stable certificate reference when applicable, and principal status. |
| Surface | Listener surface, requested surface, and final classified surface. |
| Contract or descriptor | Procedure contract binding or typed operation descriptor identity. |
| Permission | Required permission family, explicit deny result, allow source, policy version, and break-glass marker when applicable. |
| Decision | Accepted or denied outcome, reason code, and `NoTransaction` marker for rejections. |
| Durability | Audit event id, durable audit family, record LSN or durable sink evidence when the path requires durable audit. |

Audit evidence is forensic and authorization evidence. It is not database truth,
does not replace WAL-backed recovery, and must not be placed on the transaction
commit path. If durable audit is required for a security decision and the audit
sink is unavailable, the decision must fail closed rather than silently allow
dispatch.

## Validation

Documentation acceptance checks:

- The spec states the canonical order as typed identity, surface
  classification, contract lookup, permission evaluation, admission decision,
  and durable or audit trace.
- The spec keeps protocol and frame validation as preconditions without
  reordering security admission.
- The spec rejects Administration, HA/DR, backup, restore, security-management,
  and forensic operations on the Application Surface.
- The spec requires Procedure contract or typed operation descriptor lookup
  before permission evaluation.
- The spec requires explicit permission mappings and rejects ordinal or
  string-prefix compatibility.
- The spec requires `NoTransaction` effect for identity, surface, contract,
  permission, policy, resource, and audit-unavailable denials.
- The spec keeps audit evidence distinct from database truth and off the
  transaction commit path.
- The spec does not introduce gRPC, runtime JSON defaults, ad hoc SQL, generic
  command text, or untyped payload tunnels.

Future implementation work should add or keep targeted validation for:

```powershell
cargo test -p andromeda-quic --test procedure_gateway_route
cargo test -p andromeda-quic --test zero_rtt_admission_policy
cargo test -p andromeda-exec --test remote_invoke_network_e2e
cargo test -p andromeda-observe --test audit_family_contract
```

Security and RPC changes that implement this order must also include admission
matrix tests for Application Surface rejection of Administration, HA/DR, backup,
restore, forensic, and security-management operations.

## Troubleshooting

| Symptom | Likely cause | Corrective action |
| --- | --- | --- |
| Permission evaluation runs before identity is typed. | Authorization was attempted against partial identity. | Move permission evaluation after typed identity, surface classification, and contract lookup. |
| Application route accepts an admin operation. | Surface classification allowed cross-surface routing. | Reject at surface classification and require Administration Surface admission. |
| HA/DR promotion is reachable through Application. | Cluster surface separation was bypassed. | Reject the operation on Application and require Cluster or HA/DR surface identity and policy. |
| Forensic startup can be requested by an application client. | Forensic operations were treated as business Procedures. | Reject on Application and route only through the owning administrative or recovery control path. |
| Client-provided permission text controls admission. | Contract lookup did not own required permission evidence. | Read required permissions from the Procedure contract or typed operation descriptor. |
| Denied admission creates a transaction. | Admission failure was not fail-closed before transaction creation. | Return a typed rejection with `NoTransaction` and block dispatch. |
| Accepted security decision has no durable audit evidence where required. | Audit handling ran after visible dispatch or was skipped. | Fail closed or move durable audit handling before decision visibility. |
| Audit replay is described as reconstructing database state. | Audit evidence was confused with storage truth. | Reword audit as forensic and authorization evidence only. |

## References

- `AGENTS.md`
- `documentations/specs/SecurityAdmission_v0.md`
- `documentations/specs/ProcedureContract_v0.md`
- `documentations/specs/AuditLedger_v0.md`
- `documentations/specs/FrameHeader_RPC_v0.md`
- `documentations/04_QUIC_RPC_SECURITY_HADR_OPERATIONS.md`
- `documentations/governance/decisions/DEC-018-mtls-identity-extraction.md`
- `documentations/governance/decisions/DEC-033-durable-audit-ledger.md`
- `documentations/governance/decisions/DEC-040-rpc-quic-security-boundary.md`
- `documentations/governance/decisions/DEC-041-security-contract-boundary.md`
- `.agents/skills/iam-security-policy/SKILL.md`
