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
| `DefinitionBatchId` | Stable batch identity carried by dry-run, apply, WAL, and audit evidence. |
| `CompatibilityDecision` | Additive, breaking, security-impact, deprecated, or rejected decision. |
| `PublicationWalRecord` | WAL record proving all-or-nothing catalog publication. |
| `DefinitionBatchRejectionCode` | Stable typed rejection code for failed dry-run or apply. |

## Invariants

- DryRun must run before apply.
- Apply is transactional.
- No half-catalog is published.
- Breaking changes require policy.
- No operation from a partially failed batch may become visible.
- Visible publication requires durable WAL coverage.
- Dry-run and apply use the same `DefinitionBatchId`.
- CompatibilityDecision is explicit for every operation.
- Publication is all-or-nothing at CatalogVersion granularity.
- Rejection codes are stable enough for tests, audit, and operator evidence.

## Procedure lifecycle operations

DefinitionBatch is the only visible catalog mutation path for Procedure lifecycle changes.
The operation surface remains limited until each lifecycle class has explicit compatibility,
dependency, durability, and audit evidence.

| Operation | Required gate |
|---|---|
| Create | New target, explicit identity allocation, contract hash publication, and dependency registration. |
| Alter | Existing target, explicit identity preservation, compatibility acceptance. |
| Drop or deprecate | Explicit dependency closure, active invocation policy, historical evidence retention. |
| Deprecated | Fence keys for new invocations of the deprecated version. |

Alter compatibility must reject unsafe Procedure changes before publication, including Input changes,
required permission changes, result stream removal, and any shape mutation where contract, catalog,
stats, and policy evidence allow reuse cannot be proven.

Drop and deprecate behavior must reject new invocation binding to the deprecated active name or
version. Compatibility tests must classify alters, drops, renames, moves before the operation surface
expands.


## Serialization

- Persisted and network-visible formats use canonical encoding.
- Headers use explicit fixed-width integer fields.
- Variable payloads declare length before payload.
- Critical persisted structures use version fields.
- Rust native struct layout must not be persisted or sent over the wire.

### Durable publication evidence

DefinitionBatch publication is all-or-nothing at `CatalogVersion` granularity.

| Evidence field | Required rule |
|---|---|
| DefinitionBatchId | Same value in dry-run, apply, WAL records, audit, and recovery evidence. |
| PreviousCatalogVersion | Must equal the visible catalog version used for dry-run. |
| NextCatalogVersion | Must be exactly the version proposed by the accepted dry-run. |
| SourceHash | Hash of the accepted DefinitionBatch source or canonical import payload. |
| DependencyGraphHash | Hash of the accepted dependency graph. |
| OperationCount | Must match the number of applied operations. |
| PublicationWalRecord | Commit evidence that covers every operation in the batch. |
| DurableLsn | Must be at or beyond the publication commit record before visibility. |

Recovery replays a batch only when begin/apply/commit evidence is complete, record counts match, hashes match, and the publication LSN is in the durable WAL prefix. Incomplete batches are skipped and reported; they must not publish a half-catalog.

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

| Operation | V0 status |
|---|---|
| Create | Active. Requires new identity, contract hash, dependency graph, and publication evidence. |
| Deprecate | Active. Requires existing identity, active invocation fencing, and retained historical evidence. |
| Alter | Reserved unless represented as a compatible create of the same identity at the next version. |
| Drop | Reserved; use deprecate until dependency closure and retained evidence rules are implemented. |
| Rename or move | Reserved and rejected in V0. |

## Tests

- dry-run valid/invalid tests.
- crash mid-apply tests.
- breaking change rejection tests.
- dependency order tests.
- DefinitionBatchId continuity tests.
- compatibility decision matrix tests.
- publication WAL all-or-nothing tests.

## Rejection criteria

- Reject `apply without dry-run`.
- Reject `apply with different DefinitionBatchId than dry-run`.
- Reject `destructive change without policy`.
- Reject `missing dependency`.
- Reject `security downgrade`.
- Reject `publication without WAL evidence`.
- Reject `unstable DefinitionBatchRejectionCode`.

## Acceptance summary

This specification is acceptable when implementation, tests, and documentation can prove the listed invariants without hidden defaults.
