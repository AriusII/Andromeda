# Specification: DefinitionBatch v0

> **Status:** Normative V0 specification  
> **Audience:** Andromeda maintainers, engine developers, architects, reviewers  
> **Language:** American English  
> **Baseline:** Rust 1.95.0, Rust 2024 Edition, x64 and ARM64 first

## In this article


- Define the purpose and scope of `DefinitionBatch v0`.
- State the required structures.
- State invariants, errors, security, recovery, tests, and rejection criteria.

## Purpose

Define controlled catalog evolution.

## Scope

This specification applies to V0 documentation and implementation planning. It defines the minimum stable contract needed for code, tests, and review.

## Non-goals

- It does not define a final production implementation.
- It does not weaken Andromeda's procedure-only surface.
- It does not authorize hidden dynamic behavior.

## Data structures

| Structure | Required role |
|---|---|
| `DefinitionBatch` | Must be represented as an explicit typed structure or canonical descriptor. |
| `Operation` | Must be represented as an explicit typed structure or canonical descriptor. |
| `DependencyGraph` | Must be represented as an explicit typed structure or canonical descriptor. |
| `DryRunResult` | Must be represented as an explicit typed structure or canonical descriptor. |
| `ApplyTrace` | Must be represented as an explicit typed structure or canonical descriptor. |

## Invariants

- DryRun must run before apply.
- Apply is transactional.
- No half-catalog is published.
- Breaking changes require policy.


## Serialization

- Persisted and network-visible formats use canonical encoding.
- Headers use explicit fixed-width integer fields.
- Variable payloads declare length before payload.
- Critical persisted structures use version fields.
- Rust native struct layout must not be persisted or sent over the wire.

## State transitions

State transitions must be explicit. Invalid transitions return typed errors and emit trace evidence when they affect execution, storage, security, or recovery.

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

- dry-run valid/invalid tests.
- crash mid-apply tests.
- breaking change rejection tests.
- dependency order tests.

## Rejection criteria

- Reject `apply without dry-run`.
- Reject `destructive change without policy`.
- Reject `missing dependency`.
- Reject `security downgrade`.

## Acceptance summary

This specification is acceptable when implementation, tests, and documentation can prove the listed invariants without hidden defaults.
