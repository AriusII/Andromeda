# Specification: SecurityAdmission v0

> **Status:** Normative V0 specification  
> **Audience:** Andromeda maintainers, engine developers, architects, reviewers  
> **Language:** American English  
> **Baseline:** Rust 1.95.0, Rust 2024 Edition, x64 and ARM64 first

## In this article


- Define the purpose and scope of `SecurityAdmission v0`.
- State the required structures.
- State invariants, errors, security, recovery, tests, and rejection criteria.

## Purpose

Define admission order before transaction creation.

## Scope

This specification applies to V0 documentation and implementation planning. It defines the minimum stable contract needed for code, tests, and review.

## Non-goals

- It does not define a final production implementation.
- It does not weaken Andromeda's procedure-only surface.
- It does not authorize hidden dynamic behavior.

## Data structures

| Structure | Required role |
|---|---|
| `CertificateIdentity` | Must be represented as an explicit typed structure or canonical descriptor. |
| `UserPrincipal` | Must be represented as an explicit typed structure or canonical descriptor. |
| `Role` | Must be represented as an explicit typed structure or canonical descriptor. |
| `Permission` | Must be represented as an explicit typed structure or canonical descriptor. |
| `Policy` | Must be represented as an explicit typed structure or canonical descriptor. |
| `AdmissionDecision` | Must be represented as an explicit typed structure or canonical descriptor. |
| `SecurityAuditTrace` | Must be represented as an explicit typed structure or canonical descriptor. |
| `SurfaceScope` | Application, Administration, HA/DR, or internal surface boundary. |
| `PolicyVersion` | Explicit policy version used for admission and audit evidence. |
| `BreakGlassPolicy` | Explicit emergency policy requiring reason, principal, expiry, and audit evidence. |
| `AdmissionRejectionCode` | Stable typed rejection code for denied or malformed admission. |

## Invariants

- Admission happens before transaction creation.
- Deny wins unless break-glass policy applies.
- Break-glass is deny-by-default unless a valid `BreakGlassPolicy` is present.
- SurfaceScope is enforced.
- Every decision emits audit evidence.
- CertificateIdentity proves cryptographic identity only; it is not a permission grant.
- Admission failures are fail-closed and typed.


## Serialization

- Persisted and network-visible formats use canonical encoding.
- Headers use explicit fixed-width integer fields.
- Variable payloads declare length before payload.
- Critical persisted structures use version fields.
- Rust native struct layout must not be persisted or sent over the wire.

## State transitions

State transitions must be explicit. Invalid transitions return typed errors and emit trace evidence when they affect execution, storage, security, or recovery.

### Fail-closed decision matrix

| Condition | Decision | Required evidence |
|---|---|---|
| Missing certificate identity | Deny before transaction creation | Protocol/security trace; no principal assumed. |
| Certificate not bound to principal | Deny before transaction creation | SecurityAuditTrace with identity failure. |
| Disabled principal | Deny before transaction creation | Principal id, PolicyVersion when available, denial reason. |
| Permission evaluator unavailable | Deny before transaction creation | Fail-closed reason and audit attempt. |
| Unknown permission | Deny before transaction creation | Permission identifier and policy evidence when available. |
| SurfaceScope mismatch | Deny before transaction creation | Surface, requested permission, principal, and reason. |
| Stale PolicyVersion | Deny before transaction creation | Expected and observed PolicyVersion. |
| Valid break-glass policy | Allow only on authorized administration surface | Reason, expiry, principal, policy digest, audit evidence. |
| Break-glass missing reason or expiry | Deny before transaction creation | Break-glass rejection reason. |

## Error model

| Error family | Use |
|---|---|
| ContractError | Invalid shape, incompatible hash, missing contract field. |
| PermissionError | Principal lacks required permission or surface scope. |
| ResourceError | Budget, quota, backpressure, or timeout failure. |
| TransactionError | Isolation, rollback, commit, or serialization failure. |
| StorageError | WAL, page, segment, manifest, or corruption failure. |
| SystemError | Internal condition requiring poison, rollback, forensic, or restore path. |

## Security model

Security-sensitive operations require admission through identity, principal, permission, policy, and audit checks before durable mutation or transaction creation.

## Observability

At minimum, implementations must emit trace evidence with:

```text
TraceId
InvocationId when applicable
CatalogVersion when applicable
PolicyVersion when applicable
Result
ErrorKind when applicable
```

## Recovery behavior

If this specification affects durable state, it must define how recovery replays, validates, rebuilds, or rejects the affected state.

## Compatibility

Changes are classified as:

| Change | Default status |
|---|---|
| Add optional field with explicit default | Additive |
| Add required field | Breaking |
| Change type or cardinality | Breaking |
| Change security requirement | Security-impact |
| Change recovery behavior | Breaking unless explicitly versioned |

## Tests

- disabled principal tests.
- surface mismatch tests.
- permission denied tests.
- break-glass audit tests.
- fail-closed policy store unavailable tests.
- stable AdmissionRejectionCode tests.
- matrix row coverage tests.

## Rejection criteria

- Reject `transaction before admission`.
- Reject `certificate as permission bypass`.
- Reject `unaudited allow`.
- Reject `break-glass without expiry`.
- Reject `break-glass without reason`.
- Reject `string-only admission denial`.

## Acceptance summary

This specification is acceptable when implementation, tests, and documentation can prove the listed invariants without hidden defaults.
