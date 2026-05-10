# Specification: SecurityAdmission v0

> **Status:** Normative V0 specification  
> **Audience:** Andromeda maintainers, engine developers, architects, reviewers  
> **Language:** American English  
> **Baseline:** Rust 1.95.0, Rust 2024 Edition, x64 and ARM64 first

## In this article

- Define the purpose and scope of `SecurityAdmission v0`.
- State the required structures and admission order.
- State fail-closed decisions, audit hooks, errors, tests, and rejection criteria.

## Purpose

Define security admission before procedure execution, transaction creation, durable mutation, or privileged administration action.

Security admission binds transport identity, logical principal, requested surface, requested permission, policy version, and audit evidence into one explicit decision. A caller that has an authenticated mTLS certificate is not admitted until every required admission step succeeds.

## Scope

This specification applies to V0 documentation and implementation planning. It defines the minimum stable contract needed for code, tests, and review.

The V0 admission boundary covers:

- QUIC connections using custom Andromeda RPC semantics and typed Protobuf messages.
- Application surface procedure invocation.
- Administration surface operations.
- HA/DR cluster surface operations.
- Internal operations only when they cross a security-sensitive boundary or impersonate a user-facing request.

Admission must complete before transaction creation, executor dispatch, WAL append, catalog mutation, storage mutation, audit export, backup/restore mutation, or HA/DR state transition.

## Non-goals

- It does not define a final production implementation.
- It does not weaken Andromeda's procedure-only application surface.
- It does not authorize hidden dynamic behavior.
- It does not introduce gRPC service semantics or JSON-native protocol payloads.
- It does not allow certificate identity to imply a logical user, role, or permission.

## Data structures

| Structure | Required role |
|---|---|
| `AdmissionRequest` | Typed request containing surface, operation, permission set, certificate identity, optional invocation binding, and resource admission context. |
| `CertificateIdentity` | Cryptographic mTLS peer identity evidence. It is never a user, role, or permission grant. |
| `UserPrincipal` | Logical authenticated principal resolved from an allowed certificate binding or equivalent trusted identity mapping. |
| `PrincipalStatus` | Enabled, disabled, suspended, revoked, or expired principal state. Only enabled principals may be admitted. |
| `Role` | Named grouping of permissions. Roles are expanded through the active policy snapshot and cannot bypass explicit denies. |
| `Permission` | Stable typed permission identifier with surface, action, resource class, and optional resource id. Unknown permissions deny. |
| `Policy` | Canonical allow/deny policy snapshot with explicit precedence, digest, and version. |
| `PolicyVersion` | Explicit policy snapshot identity used for admission, plan/cache binding, and audit evidence. |
| `SurfaceScope` | Application, Administration, HA/DR, or Internal surface boundary. |
| `BreakGlassPolicy` | Typed emergency policy that is disabled by default and valid only under the rules in this specification. |
| `AdmissionDecision` | Typed allow or deny result with decision id, rejection code when denied, policy version evidence, and audit trace reference. |
| `AdmissionRejectionCode` | Stable typed rejection code for denied, malformed, stale, unavailable, or unauditable admission. |
| `SecurityAuditTrace` | Structured trace evidence for every admission decision and every admission attempt that reaches the decision layer. |

### CertificateIdentity v0

`CertificateIdentity` must be canonical and must include enough evidence to validate the authenticated transport peer without granting permissions:

| Field | Required rule |
|---|---|
| `certificate_fingerprint` | Stable digest of the peer certificate or verified certificate chain root used for binding. |
| `issuer_fingerprint` | Stable digest or id of the issuing authority accepted by the trust store. |
| `subject` | Canonical subject descriptor after parser normalization. |
| `subject_alt_names` | Canonical ordered set of validated SAN entries or equivalent workload identity names. |
| `not_before` and `not_after` | Certificate validity window used for the admission decision. |
| `trust_domain` | Explicit trust domain, tenant, or cluster authority that scoped the certificate. |
| `mtls_session_id` | Request/session binding evidence when available from QUIC transport. |
| `revocation_status` | Fresh accepted, stale, revoked, or unavailable revocation evidence. Unavailable or revoked fails closed unless policy explicitly permits stale revocation evidence for a bounded internal-only recovery path. |

The certificate identity proves only that a cryptographic peer was authenticated. It must not contain `PrincipalId`, role membership, or permission bits as admission facts. A certificate extension may be used as mapping input only after policy validates the mapping rule.

### UserPrincipal v0

`UserPrincipal` must be canonical and must be resolved after certificate validation:

| Field | Required rule |
|---|---|
| `principal_id` | Stable logical principal id. It must be non-empty, canonical, and auditable. |
| `principal_kind` | User, service, cluster node, automation, or emergency operator. |
| `principal_status` | Must be enabled for normal admission. Disabled, suspended, revoked, or expired principals deny. |
| `bound_certificate_fingerprints` | Policy-controlled bindings or references used to prove the mTLS identity may act as this principal. |
| `tenant_or_security_domain` | Scope that constrains policy lookup and resource access. |
| `role_refs` | Roles to expand using the active policy snapshot. Empty roles do not imply deny if direct permissions exist, but no hidden default role is permitted. |
| `valid_from` and `valid_until` | Principal validity window when applicable. Expired principals deny. |

Principal resolution must fail closed when no binding exists, when more than one logical principal matches without a deterministic policy rule, or when the identity store is unavailable.

### Permission v0

`Permission` must be a stable typed value, not a free-form string. It must contain:

| Field | Required rule |
|---|---|
| `permission_id` | Stable canonical id used in policy, audit, tests, and rejection evidence. |
| `surface_scope` | Application, Administration, HA/DR, or Internal. |
| `action` | Stable action enum or registry entry. |
| `resource_class` | Procedure, catalog, database, backup, restore, cluster, audit, policy, or system resource class. |
| `resource_id` | Optional canonical resource id. Absence means class-wide only when policy explicitly allows class-wide grants. |
| `requires_policy_version` | True for all permissioned security decisions in V0. |

Unknown permissions, unregistered actions, malformed resource ids, and cross-surface permission reuse deny before execution. The Application surface may invoke procedures only through procedure contracts and required permissions; it may not request Administration or HA/DR permissions.

### PolicyVersion v0

`PolicyVersion` identifies the exact policy snapshot used for the decision.

| Field | Required rule |
|---|---|
| `policy_version` | Monotonic or otherwise totally ordered snapshot identifier within the policy domain. |
| `policy_digest` | Stable digest of the canonical policy snapshot or signed policy bundle. |
| `policy_domain` | Tenant, cluster, or system domain for the snapshot. |
| `loaded_at` | Time or logical clock evidence for snapshot acquisition. |
| `staleness_bound` | Maximum accepted age or logical freshness bound. |

Every allow must carry `PolicyVersion` and `policy_digest`. Every denial after policy lookup must carry the observed policy version and digest. Pre-policy denials must explicitly mark `PolicyVersion` as unavailable and state why policy lookup was not reached.

Stale, missing, unsigned, malformed, conflicting, or unavailable policy snapshots deny. A plan cache key, procedure invocation binding, or admission context that carries a stale or mismatched `PolicyVersion` must be rejected before transaction creation.

### SurfaceScope v0

`SurfaceScope` is part of the authorization decision, audit trace, and protocol route binding.

| Surface | Allowed V0 scope | Required denial rule |
|---|---|---|
| Application | Procedure invocation only, using published procedure contracts and required permissions. | Deny Administration, HA/DR, policy mutation, audit export, backup/restore mutation, and direct storage or filesystem operations. |
| Administration | Administrative operations such as policy, catalog administration, audit export, and operator actions. | Deny application procedure execution unless routed through the application admission path. |
| HA/DR | Cluster membership, quorum, backup, restore, failover, fencing, and recovery operations. | Deny Application and general Administration permissions unless explicitly modeled as HA/DR permissions. |
| Internal | Engine-local service boundary with explicit caller evidence. | Deny external client traffic and deny privilege escalation from a user surface. |

Surface mismatch always denies. The denial must happen before transaction creation and must emit `SecurityAuditTrace`.

### BreakGlassPolicy v0

Break-glass is denied by default. It is not a general override and never grants Application surface procedure execution.

A `BreakGlassPolicy` may allow only a bounded Administration or HA/DR emergency operation when all of these fields and validations are present:

| Field or rule | Requirement |
|---|---|
| `break_glass_policy_id` | Stable non-empty id present in the active policy snapshot. |
| `policy_version` and `policy_digest` | Must match the active policy snapshot used for admission. |
| `requesting_principal_id` | Must identify an enabled emergency-capable principal. |
| `authorizing_principal_id` | Must identify a distinct enabled authorizer unless policy explicitly defines a single-operator disaster mode for HA/DR recovery. |
| `reason` | Non-empty operator reason, bounded, redacted, and persisted in audit evidence. |
| `ticket_or_incident_id` | Non-empty external or internal incident reference. |
| `allowed_surface_scope` | Administration or HA/DR only. Application is invalid. |
| `allowed_permissions` | Explicit emergency permission allowlist. Wildcards and unknown permissions are invalid. |
| `not_before` and `expires_at` | Bounded validity window. Missing, expired, future-invalid, or unbounded windows deny. |
| `max_duration` | Must be bounded by policy. |
| `audit_required` | Must be true and must require durable audit append for allows. |

Break-glass cannot override malformed certificate identity, missing principal binding, disabled or revoked principal, unavailable audit sink for allow decisions, surface mismatch to Application, unknown permission, unsupported protocol version, malformed frame, invalid Protobuf envelope, or transaction-before-admission.

If break-glass is used, the decision must include both normal denial evidence and explicit break-glass allow evidence. If either audit trace or durable audit append fails for a break-glass allow, the operation denies.

## Invariants

- Admission happens before transaction creation, executor dispatch, WAL append, durable mutation, and privileged operation execution.
- Deny wins over allow. Explicit deny in policy wins over role expansion, direct allow, inherited allow, and break-glass except where this specification explicitly permits break-glass.
- Break-glass is deny-by-default and cannot apply to Application surface procedure execution.
- `SurfaceScope` is enforced for every request and permission.
- Every admission decision emits `SecurityAuditTrace`.
- Every allow and every policy-backed denial records `PolicyVersion` and `policy_digest`.
- `CertificateIdentity` proves cryptographic identity only; it is not a permission grant.
- Principal resolution is separate from certificate validation.
- Permission evaluation is typed, stable, and policy-versioned.
- Admission failures are fail-closed and use stable `AdmissionRejectionCode` values.
- Application, Administration, HA/DR, and Internal surfaces remain separated.
- QUIC is transport; Andromeda custom RPC and typed Protobuf are the only V0 native remote admission protocol semantics.

## Admission order

Implementations must use this order or prove an equivalent stricter order:

1. Validate QUIC session and RPC frame context before payload allocation beyond admitted bounds.
2. Validate protocol version, route, `SurfaceScope`, and typed Protobuf envelope.
3. Build `AdmissionRequest` with request id, session id, trace id, surface, operation, requested permissions, contract binding when applicable, and resource budget context.
4. Validate `CertificateIdentity`, trust domain, validity window, and revocation evidence.
5. Resolve `UserPrincipal` from certificate binding through a deterministic identity mapping rule.
6. Reject disabled, suspended, revoked, expired, ambiguous, or missing principals.
7. Load the active `PolicyVersion` snapshot for the principal, surface, operation, and security domain.
8. Expand roles and direct grants inside the loaded policy snapshot.
9. Validate all requested permissions and surface constraints.
10. Apply explicit denies before allows.
11. Apply `BreakGlassPolicy` only if the normal decision denies and every break-glass rule in this specification passes.
12. Emit `SecurityAuditTrace` and required durable audit evidence.
13. Return `AdmissionDecision`.
14. Only after an allow may the caller create a transaction, dispatch execution, append domain WAL, or perform the privileged operation.

Any unavailable dependency in steps 1 through 12 denies unless the request has not reached admission because the frame or protocol route was already rejected by the RPC layer.

## Serialization

- Persisted and network-visible formats use canonical encoding.
- Admission-visible protocol payloads use typed Protobuf messages over custom Andromeda RPC.
- Headers use explicit fixed-width integer fields.
- Variable payloads declare length before payload.
- Critical persisted structures use version fields.
- Rust native struct layout must not be persisted or sent over the wire.
- JSON-native admission requests, string-only permissions, and gRPC service contracts are not V0 protocol formats.

## State transitions

State transitions must be explicit. Invalid transitions return typed errors and emit trace evidence when they affect execution, storage, security, or recovery.

| Transition | Valid rule |
|---|---|
| `Unauthenticated -> CertificateValidated` | Only after mTLS identity validation succeeds. |
| `CertificateValidated -> PrincipalResolved` | Only after deterministic binding to one enabled logical principal. |
| `PrincipalResolved -> PolicyLoaded` | Only after an active, fresh, verified policy snapshot is loaded. |
| `PolicyLoaded -> PermissionEvaluated` | Only after all requested permissions are known and surface-scoped. |
| `PermissionEvaluated -> Allowed` | Only after explicit denies are absent and all required allows are present. |
| `PermissionEvaluated -> BreakGlassEvaluated` | Only after normal policy evaluation denies and request is Administration or HA/DR. |
| `BreakGlassEvaluated -> Allowed` | Only when every `BreakGlassPolicy v0` requirement passes and durable audit append succeeds. |
| `AnyAdmissionState -> Denied` | On malformed, unavailable, stale, unauthorized, unauditable, or mismatched input. |

No state transition may create a transaction, append a domain WAL record, mutate catalog/storage, or start executor work before `Allowed`.

## Fail-closed decision matrix

| Condition | Decision | Rejection code | Error family | Required evidence |
|---|---|---|---|---|
| Missing certificate identity | Deny before transaction creation | `MissingCertificateIdentity` | PermissionError | Trace id, request/session when available, surface, no principal assumed, policy unavailable reason. |
| Malformed certificate identity | Deny before transaction creation | `MalformedCertificateIdentity` | PermissionError | Certificate parse failure class, trust domain when available, no principal assumed. |
| Expired or not-yet-valid certificate | Deny before transaction creation | `CertificateValidityWindowInvalid` | PermissionError | Certificate fingerprint, validity window, observed decision time. |
| Revoked certificate | Deny before transaction creation | `CertificateRevoked` | PermissionError | Certificate fingerprint, revocation evidence id, no principal assumed. |
| Revocation status unavailable | Deny before transaction creation | `CertificateRevocationUnavailable` | PermissionError | Certificate fingerprint, revocation source, fail-closed reason. |
| Certificate not bound to principal | Deny before transaction creation | `PrincipalBindingMissing` | PermissionError | Certificate fingerprint, mapping rule id when available, no principal assumed. |
| Ambiguous principal binding | Deny before transaction creation | `PrincipalBindingAmbiguous` | PermissionError | Certificate fingerprint, mapping rule id, redacted candidate count. |
| Disabled, suspended, revoked, or expired principal | Deny before transaction creation | `PrincipalNotEnabled` | PermissionError | Principal id, status, policy version when available, denial reason. |
| Identity or principal store unavailable | Deny before transaction creation | `IdentityStoreUnavailable` | SystemError | Store id, timeout/backpressure class, no hidden fallback. |
| Policy store unavailable | Deny before transaction creation | `PolicyStoreUnavailable` | SystemError | Principal id when available, surface, requested permission ids, fail-closed reason. |
| Missing policy version | Deny before transaction creation | `PolicyVersionMissing` | PermissionError | Principal id, surface, requested permission ids, policy unavailable reason. |
| Stale policy version | Deny before transaction creation | `PolicyVersionStale` | PermissionError | Expected and observed policy version, digest when available. |
| Policy digest mismatch | Deny before transaction creation | `PolicyDigestMismatch` | PermissionError | Observed version, expected digest, observed digest. |
| Permission evaluator unavailable | Deny before transaction creation | `PermissionEvaluatorUnavailable` | SystemError | Principal id, surface, requested permissions, policy version, audit attempt. |
| Unknown permission | Deny before transaction creation | `UnknownPermission` | PermissionError | Permission id, surface, policy version, operation. |
| Permission denied by explicit policy | Deny before transaction creation | `PermissionExplicitlyDenied` | PermissionError | Principal id, permission id, deny rule id, policy version and digest. |
| Required permission absent | Deny before transaction creation | `PermissionMissing` | PermissionError | Principal id, permission id, policy version and digest. |
| SurfaceScope mismatch | Deny before transaction creation | `SurfaceScopeMismatch` | PermissionError | Requested surface, permission surface, operation, principal id when available. |
| Application request for Administration or HA/DR permission | Deny before transaction creation | `PrivilegedPermissionOnApplicationSurface` | PermissionError | Surface, permission id, operation, principal id, policy version. |
| Procedure contract missing required permission binding | Deny before transaction creation | `ContractPermissionBindingMissing` | ContractError | Procedure id, contract hash when available, missing permission binding. |
| Procedure contract policy version mismatch | Deny before transaction creation | `ContractPolicyVersionMismatch` | ContractError | Procedure id, contract hash, contract policy version, active policy version. |
| Resource budget unavailable before admission | Deny before transaction creation | `AdmissionBudgetUnavailable` | ResourceError | Budget class, requested operation, fail-closed reason. |
| Resource budget exceeded before admission | Deny before transaction creation | `AdmissionBudgetExceeded` | ResourceError | Budget class, requested and allowed values, policy version when available. |
| Audit trace emission unavailable for denial | Deny and record best-effort local failure evidence | `AuditTraceUnavailableForDeny` | SystemError | Local failure evidence; operation remains denied. |
| Durable audit append unavailable for allow | Deny before execution | `AuditAppendUnavailableForAllow` | SystemError | Principal id, surface, policy version, sink id, fail-closed reason. |
| Break-glass absent | Deny normal rejection | Normal rejection code | PermissionError | Normal denial evidence; no break-glass policy id. |
| Break-glass used on Application surface | Deny before transaction creation | `BreakGlassSurfaceForbidden` | PermissionError | Surface, operation, principal id, policy version. |
| Break-glass missing reason, incident id, expiry, or authorizer | Deny before execution | `BreakGlassMalformed` | PermissionError | Missing field list, requesting principal, policy version. |
| Break-glass expired or outside validity window | Deny before execution | `BreakGlassExpired` | PermissionError | Break-glass policy id, observed time, validity window. |
| Break-glass permission not allowlisted | Deny before execution | `BreakGlassPermissionDenied` | PermissionError | Break-glass policy id, permission id, policy version. |
| Valid break-glass policy with durable audit evidence | Allow only on authorized Administration or HA/DR surface | None | None | Normal denial evidence, break-glass policy id, requester, authorizer, reason, incident id, expiry, policy version, durable audit record id. |
| Transaction creation attempted before allow | Reject operation and poison caller path when applicable | `TransactionBeforeAdmission` | SystemError | Caller component, operation, trace id, no transaction side effects. |

## Error model

| Error family | Use |
|---|---|
| ContractError | Invalid admission request shape, malformed contract permission binding, incompatible hash, missing contract field, or policy version mismatch in invocation binding. |
| PermissionError | Principal lacks required permission, surface scope mismatches, identity binding fails, policy denies, or break-glass is invalid. |
| ResourceError | Admission budget, quota, backpressure, or timeout failure before transaction creation. |
| TransactionError | Transaction creation attempted before admission or transaction boundary violated after admission. |
| StorageError | WAL, page, segment, manifest, audit sink, or corruption failure that affects admission evidence. |
| SystemError | Internal condition requiring poison, rollback, forensic, restore, or fail-closed path. |

Every denial must use `AdmissionRejectionCode`; string-only denial is invalid.

## Security model

Security-sensitive operations require admission through identity, principal, permission, policy, and audit checks before durable mutation or transaction creation.

The security model is fail-closed:

- Missing evidence denies.
- Unavailable stores deny.
- Unknown permissions deny.
- Ambiguous identity mapping denies.
- Stale policy denies.
- Surface mismatch denies.
- Audit failure denies any allow.
- Denial audit failure cannot convert denial into allow.

The Application surface is procedure-only. Procedures cannot access external network, external filesystem, Administration operations, HA/DR operations, policy mutation, direct storage mutation, or audit export unless a future version explicitly changes the contract and tests.

## Audit hooks

`SecurityAuditTrace` must be emitted for every admission decision and for every admission attempt that reaches the admission service. The trace must be structured, bounded, and redacted.

| Field | Required rule |
|---|---|
| `trace_id` | Required for every trace. |
| `request_id` and `session_id` | Required when tied to RPC. |
| `invocation_id` | Required for procedure invocation once known. |
| `surface_scope` | Required for every decision. |
| `operation` | Required stable operation id. |
| `certificate_fingerprint` | Required when certificate evidence exists. |
| `principal_id` | Required after principal resolution; explicitly unavailable before resolution. |
| `permission_ids` | Required for permissioned operations. |
| `procedure_id` and `contract_hash` | Required for application procedure invocation once decoded. |
| `catalog_version` | Required when a procedure or catalog-bound operation is admitted. |
| `policy_version` and `policy_digest` | Required for every allow and every post-policy denial. |
| `result` | Allow or deny. |
| `admission_rejection_code` | Required for every denial. |
| `error_kind` | Required for every denial. |
| `reason_code` | Required stable reason code. |
| `break_glass_policy_id` | Required when break-glass is evaluated. |
| `durable_audit_record_id` | Required for every allow when durable audit is configured as required, and always required for break-glass allow. |

Pre-policy failures must set principal and policy fields to explicitly unavailable values, not empty strings. Post-policy decisions must carry the active `PolicyVersion`.

Durable audit append is required before allowing break-glass, policy mutation, audit export, backup/restore mutation, HA/DR state transition, and any operation whose policy marks audit as required before execution. If durable audit append fails for those allows, admission denies.

## Observability

At minimum, implementations must emit trace evidence with:

```text
TraceId
RequestId when applicable
SessionId when applicable
InvocationId when applicable
PrincipalId when resolved
SurfaceScope
PermissionId when applicable
ProcedureId when applicable
ContractHash when applicable
CatalogVersion when applicable
PolicyVersion when applicable
PolicyDigest when applicable
Result
AdmissionRejectionCode when denied
ErrorKind when applicable
```

Protocol rejections that happen before admission, such as bad frame version or invalid Protobuf envelope, are owned by the RPC/protocol trace path. If such a request reaches admission context, `SecurityAuditTrace` must include the protocol rejection reference and deny.

## Recovery behavior

Admission decisions are not replayed as authority after restart. Recovery may replay audit evidence for forensics and consistency checks, but it must re-load active policy before admitting new operations.

If durable state was mutated without prior allowed admission evidence, recovery must report a forensic violation and follow the owning storage, transaction, or audit recovery policy. Missing or corrupt audit evidence for a completed security-sensitive operation is a recovery and forensic defect; it must not be silently ignored.

## Compatibility

Changes are classified as:

| Change | Default status |
|---|---|
| Add optional field with explicit default | Additive only when denial semantics are unchanged. |
| Add required field | Breaking. |
| Change type or cardinality | Breaking. |
| Change `AdmissionRejectionCode` meaning | Breaking. |
| Add new rejection code | Additive only when older implementations fail closed. |
| Change security requirement | Security-impact and breaking unless explicitly versioned. |
| Change break-glass semantics | Security-impact and breaking unless explicitly versioned. |
| Change recovery behavior | Breaking unless explicitly versioned. |

## Tests

Implementations must provide tests for:

- Missing, malformed, expired, revoked, and revocation-unavailable certificate identity.
- Certificate-to-principal binding missing and ambiguous.
- Disabled, suspended, revoked, and expired principals.
- Identity store, principal store, policy store, and permission evaluator unavailable.
- Unknown permission, explicit deny, missing required permission, and cross-surface permission reuse.
- Application surface rejection of Administration and HA/DR permissions.
- Procedure contract required permission binding and policy version mismatch.
- Stale, missing, conflicting, and digest-mismatched `PolicyVersion`.
- Resource budget unavailable and budget exceeded before transaction creation.
- Audit trace evidence on every denial row in the fail-closed matrix.
- Durable audit append required before break-glass allow and other required-audit allows.
- Break-glass missing reason, incident id, authorizer, expiry, allowlisted permission, and policy version.
- Break-glass forbidden on Application surface.
- Valid break-glass allowed only on Administration or HA/DR with durable audit evidence.
- Stable `AdmissionRejectionCode` tests for every matrix row.
- Transaction-before-admission rejection.
- Protobuf/custom RPC admission path rejects gRPC service or JSON-native admission payloads.

## Rejection criteria

Reject any implementation, test fixture, or spec extension that permits:

- Reject `transaction before admission`.
- Reject `executor dispatch before admission`.
- Reject `WAL append before admission` for a security-sensitive operation.
- Reject `certificate as permission bypass`.
- Reject `certificate as logical user`.
- Reject `principal resolution without certificate binding evidence`.
- Reject `unaudited allow`.
- Reject `break-glass on application surface`.
- Reject `break-glass without expiry`.
- Reject `break-glass without reason`.
- Reject `break-glass without incident id`.
- Reject `break-glass without policy version`.
- Reject `break-glass without durable audit evidence`.
- Reject `missing policy version on allow`.
- Reject `unknown permission as allow`.
- Reject `surface mismatch as allow`.
- Reject `string-only permission`.
- Reject `string-only admission denial`.
- Reject `dynamic JSON admission request`.
- Reject `JSON-native admission request`.
- Reject `gRPC service admission surface`.
- Reject `hidden default role`.
- Reject `policy evaluator unavailable as allow`.
- Reject `audit append unavailable as allow`.

## Acceptance summary

| Required item | Acceptance proof |
|---|---|
| Owner | Security/IAM admission owner is responsible for `CertificateIdentity`, `UserPrincipal`, `Permission`, `PolicyVersion`, `SurfaceScope`, `BreakGlassPolicy`, `AdmissionDecision`, `AdmissionRejectionCode`, and `SecurityAuditTrace` semantics in this specification. |
| Evidence | Acceptance requires tests and documentation proving every fail-closed matrix row emits a stable `AdmissionRejectionCode`, required `SecurityAuditTrace` fields, `PolicyVersion` evidence when policy is consulted, durable audit evidence for required-audit allows, and no transaction creation before `AdmissionDecision::Allow`. |
| Reject | Reject SECURITY_ADMISSION completion if any implementation allows certificate-as-permission, unaudited allow, Application-surface break-glass, missing policy version on allow, string-only denial, unknown permission allow, surface mismatch allow, unavailable evaluator allow, or transaction/executor/WAL work before admission allow. |
