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
| `DefinitionBatchState` | Explicit lifecycle state for intake, dry-run, apply, publication, rollback, rejection, and recovery. |
| `BatchOperationIndex` | Dense zero-based operation position used for dependency, rejection, audit, and WAL evidence. |
| `DependencyEdge` | Directed dependency from one catalog object or batch operation to another with an explicit dependency kind. |
| `PublicationReceipt` | Durable apply result containing the committed catalog version, WAL boundary, operation count, and evidence hashes. |
| `RollbackTrace` | Evidence emitted for a pre-commit rollback or a recovery skip of an incomplete batch. |

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
- A dry-run result is valid only for the exact source hash, dependency graph hash, previous catalog version, principal, policy version, and batch id it evaluated.
- Apply must not recompute a different dependency graph, operation order, compatibility decision, or publication target from the accepted dry-run.
- Rollback before durable commit must restore the previous visible catalog version and emit rollback evidence.
- Rollback after durable commit must not remove history in place; reversal requires a new DefinitionBatch.
- Every accepted operation has a dense `BatchOperationIndex` and exactly one compatibility decision.
- Every rejected operation identifies the failed operation index when an operation caused the rejection.
- Every security-sensitive decision records both `CertificateIdentity` and `UserPrincipal` when they are known; absence is explicit evidence, not an implicit default.

## DefinitionBatch lifecycle

DefinitionBatch V0 uses the following state machine. Implementations may use different internal names, but trace, audit, and tests must be able to project these states.

| State | Entry condition | Valid exit | Required evidence |
|---|---|---|---|
| `Received` | Canonical batch descriptor accepted for validation. | `DryRunRejected` or `DryRunAccepted` | `DefinitionBatchId`, source hash, submitted operation count, requester identity. |
| `DryRunRejected` | Dry-run found a validation, dependency, compatibility, security, or policy failure. | Terminal | Stable rejection code, failed operation index when applicable, decision trace, security audit trace when admission was consulted. |
| `DryRunAccepted` | Dry-run produced an ordered dependency graph and proposed `NextCatalogVersion`. | `ApplyRejected` or `Applying` | Previous/next catalog version, graph hash, operation count, compatibility matrix, policy version. |
| `ApplyRejected` | Apply preconditions failed before mutation WAL begin. | Terminal | Stable rejection code, accepted dry-run id, current catalog version, trace evidence. |
| `Applying` | Mutation WAL begin record is appended for the accepted dry-run. | `RolledBack`, `RecoverySkipped`, or `Published` | WAL begin LSN, operation count, graph hash, source hash. |
| `RolledBack` | Apply failed before durable publication commit. | Terminal | Rollback trace, previous catalog version, failure code, last attempted operation index. |
| `Published` | WAL commit record is durable and visibility switched to `NextCatalogVersion`. | Terminal | Publication receipt, durable LSN, catalog version receipt, audit record. |
| `RecoverySkipped` | Startup found incomplete or invalid mutation evidence. | Terminal | Recovery report entry, skipped LSN range, rejection code, previous catalog version preserved. |

## Dry-run contract

Dry-run is a deterministic admission and planning step. It must not mutate the visible catalog, durable catalog roots, or invocation binding state.

Dry-run input must include:

- `DefinitionBatchId`.
- Target database and namespace identifiers.
- Expected `PreviousCatalogVersion`.
- Canonical source or import payload hash.
- Ordered operation descriptors.
- Requesting `CertificateIdentity`, `UserPrincipal`, surface scope, requested permissions, and `PolicyVersion`.
- `StatsVersion` of the contract evidence being published, carried in `ProcedureContractBinding`; see `docs/adr/ADR-0014-STATS_VERSION_PUBLICATION.md`.
- Compatibility policy for each operation that can affect an existing object or contract.

Dry-run output must include:

- Accepted or rejected result.
- Proposed `NextCatalogVersion` for accepted batches.
- Dense operation order and `BatchOperationIndex` values.
- Dependency graph with a stable `DependencyGraphHash`.
- Compatibility decision for every operation.
- Security admission result and audit evidence for every security-sensitive operation.
- Stable rejection code and failed operation index for rejected batches when the failure is operation-specific.

Dry-run must reject:

- Empty operation lists.
- Zero or missing batch, database, namespace, object, contract, policy, or catalog version identities where the field is required.
- Source hash mismatch or non-canonical source encoding.
- Duplicate operation indexes, object ids, qualified names, lifecycle names, or publication targets.
- Dependency cycles, missing dependency targets, dependency kind mismatches, or forward same-batch references.
- Non-advancing catalog versions.
- Missing contract hash or non-canonical contract evidence.
- Security downgrade, missing required permission, surface mismatch, stale policy version, or fail-open admission.
- Unsupported operation classes in V0.

## Apply and rollback contract

Apply consumes exactly one accepted dry-run result. It must not accept a batch descriptor that only resembles the dry-run; it must bind the dry-run by `DefinitionBatchId`, previous/next catalog version, source hash, dependency graph hash, operation count, operation order, compatibility decisions, policy version, and requester evidence.

Apply preconditions:

- The current visible catalog version equals dry-run `PreviousCatalogVersion`.
- The accepted dry-run has not expired, been superseded, or been applied.
- The caller remains admitted for the same surface, principal, permissions, policy version, and operation set.
- All referenced objects still match the versions and hashes evaluated by dry-run.
- No incompatible concurrent publication has advanced the target namespace.

Apply WAL sequence:

| WAL record | Required content | Visibility rule |
|---|---|---|
| `DefinitionBatchBegin` | Batch id, previous/next catalog version, source hash, dependency graph hash, operation count. | Never makes catalog changes visible. |
| `DefinitionBatchApplyOperation` | Batch id, operation index, operation kind, object identity, compatibility decision, operation hash. | Replayed only inside a complete committed batch. |
| `DefinitionBatchCommit` | Batch id, previous/next catalog version, first/last operation index, operation count, source hash, dependency graph hash, receipt hash. | Catalog may become visible only after this record is in the durable WAL prefix. |
| `DefinitionBatchRollback` | Batch id, failure code, last attempted operation index, previous catalog version. | Documents pre-commit rollback; never advances visibility. |

Failure handling:

- Failure before `DefinitionBatchBegin` produces `ApplyRejected`.
- Failure after `DefinitionBatchBegin` and before durable `DefinitionBatchCommit` produces `RolledBack` during live apply or `RecoverySkipped` during startup.
- No operation from a rolled-back or skipped batch may be visible to binding, planning, metadata reads, audit projection as published state, or procedure invocation.
- After durable `DefinitionBatchCommit`, the batch is published history. Reversal requires a new DefinitionBatch with its own dry-run, WAL records, and audit evidence.

## Dependency graph contract

The dependency graph is part of the accepted dry-run and publication receipt. It must be stable enough for hashing, review, recovery, and tests.

| Graph element | Required rule |
|---|---|
| Node identity | Uses catalog object identity, object kind, object version, and operation index when the node is created or deprecated by the batch. |
| Edge identity | Uses source node, target node, dependency kind, and required target version. |
| Operation order | The accepted order is a topological order over the graph and is dense from `0..operation_count`. |
| Existing dependency | Target object must exist at the version evaluated by dry-run and remain unchanged until apply begins. |
| Same-batch dependency | Target operation must have a lower operation index than the dependent operation. |
| Cycle policy | Any cycle is rejected unless a future spec defines a typed mutually-recursive object class; V0 defines no such exception. |
| Hash policy | `DependencyGraphHash` covers nodes, edges, operation indexes, dependency kinds, target versions, and source hash binding. |
| Recovery policy | Recovery replays only operation records whose indexes and graph hash match the committed receipt. |

## Procedure lifecycle operations

DefinitionBatch is the only visible catalog mutation path for Procedure lifecycle changes.
The operation surface remains limited until each lifecycle class has explicit compatibility,
dependency, durability, and audit evidence.

| Operation | Required gate |
|---|---|
| Create | Active in V0. Requires new target, explicit identity allocation, contract hash publication, dependency registration, and publication evidence. |
| Deprecate | Active in V0. Requires existing target, active invocation fencing, explicit dependency closure policy, and historical evidence retention. |
| Alter | Reserved in V0 except when represented as a compatible create of the same identity at the next version with explicit compatibility acceptance. |
| Drop | Reserved in V0. Use deprecate until dependency closure, retained evidence, and restore semantics are implemented. |
| Rename or move | Reserved and rejected in V0. |

Alter | Existing target, explicit identity preservation, compatibility acceptance.
Alter compatibility must reject unsafe Procedure changes before publication, including Input changes,
required permission changes, result stream removal, and any shape mutation where contract, catalog,
stats, and policy evidence allow reuse cannot be proven.

Drop or deprecate requires dependency closure, active invocation policy, and historical evidence retention.
Drop and deprecate behavior must reject new invocation binding to the deprecated active name or
version. Deprecated | Fence keys for new invocations of the deprecated version. Reject new invocation
binding to the deprecated active name or version. Compatibility tests must classify alters, drops,
renames, moves before the operation surface expands.


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

### Publication receipt

`PublicationReceipt` is the durable operator- and test-visible result of a successful apply.

| Receipt field | Required rule |
|---|---|
| DefinitionBatchId | Same value carried by dry-run, apply, WAL, audit, and recovery evidence. |
| PreviousCatalogVersion | Must equal the version visible before publication. |
| PublishedCatalogVersion | Must equal dry-run `NextCatalogVersion`. |
| SourceHash | Must match the accepted dry-run and WAL commit record. |
| DependencyGraphHash | Must match the accepted dry-run and WAL commit record. |
| OperationCount | Must equal the number of operation records between begin and commit. |
| FirstOperationIndex / LastOperationIndex | Must prove dense operation coverage for the batch. |
| BeginLsn / CommitLsn / DurableLsn | Must prove commit record durability before visibility. |
| ReceiptHash | Stable hash over receipt fields for audit and recovery comparison. |
| SecurityAuditTraceId | Required when the batch affected permissions, policies, invocable procedures, or administrative catalog state. |
| RecoveryDecision | `NotRecovered`, `Replayed`, or `Skipped` with a recovery report reference after startup. |

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

DefinitionBatch publication is an administrative catalog surface unless a future spec narrows a specific operation to another surface. Application Procedure invocation must not publish catalog mutations directly.

Required security evidence:

| Evidence | Required rule |
|---|---|
| CertificateIdentity | Captured when transport identity exists; missing identity is an explicit denial reason. |
| UserPrincipal | Captured after principal binding; missing or disabled principal is denied before apply. |
| SurfaceScope | Must match the requested operation class; Application, Administration, and HA/DR scopes are not interchangeable. |
| RequiredPermissions | Evaluated before dry-run acceptance and rechecked before apply begins. |
| PolicyVersion | Bound to dry-run, apply preconditions, compatibility decisions, and audit evidence. |
| SecurityAuditTrace | Emitted for allow and deny decisions that affect dry-run, apply, rollback, or publication. |

Fail-open admission is forbidden. If the permission evaluator, policy store, principal binding, or audit projection required for an allow decision is unavailable, dry-run or apply must reject with `SECURITY_ADMISSION_DENIED`.

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

DefinitionBatch trace evidence must additionally carry `DefinitionBatchId`, `PreviousCatalogVersion`, `NextCatalogVersion` when known, `SourceHash`, `DependencyGraphHash`, `BatchOperationIndex` when applicable, `DefinitionBatchState`, `DefinitionBatchRejectionCode` when rejected, and the WAL boundary or publication receipt when durable mutation was attempted.

## Recovery behavior

DefinitionBatch always affects durable catalog state. Recovery must scan the durable WAL prefix and classify every DefinitionBatch boundary it encounters.

| Recovery input | Required decision |
|---|---|
| No begin record | No batch exists; visible catalog remains at manifest or checkpoint root. |
| Begin without complete operation records | Skip the batch, emit `RecoverySkipped`, preserve previous catalog version. |
| Begin and operations without commit | Skip the batch, emit `RecoverySkipped`, preserve previous catalog version. |
| Commit outside durable WAL prefix | Skip the batch, emit `RecoverySkipped`, preserve previous catalog version. |
| Commit with record count mismatch | Reject replay, enter forensic or read-only recovery mode according to recovery policy. |
| Commit with source or graph hash mismatch | Reject replay, enter forensic or read-only recovery mode according to recovery policy. |
| Complete committed batch | Replay or accept the published catalog version exactly once and emit recovery evidence. |

Recovery must not synthesize missing operation records, infer dependency graphs, or accept a commit record whose count, hashes, catalog versions, or batch id do not match the dry-run/apply evidence.

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
- apply precondition rejection tests for stale catalog version, stale policy version, source hash drift, graph hash drift, operation count drift, and caller admission drift.
- rollback tests proving pre-commit failure leaves previous catalog version visible.
- recovery tests for begin-only, begin-plus-partial-operations, complete-operations-without-commit, commit-outside-durable-prefix, count mismatch, hash mismatch, and successful replay.
- dependency graph hash stability tests and cycle, missing target, kind mismatch, and forward same-batch reference rejection tests.
- publication receipt tests proving dense operation indexes, durable LSN coverage, receipt hash stability, and audit trace linkage.
- rejection code stability tests for every code in this specification.

## Rejection criteria

DefinitionBatch implementations must:

- Reject `apply without dry-run`.
- Reject `apply with different DefinitionBatchId than dry-run`.
- Reject `destructive change without accepted compatibility and security policy`.
- Reject `missing dependency`.
- Reject `security downgrade`.
- Reject `publication without WAL evidence`.
- Reject `rollback without rollback or recovery evidence`.
- Reject `unstable DefinitionBatchRejectionCode`.

Stable rejection codes are part of the V0 contract. Implementations may expose richer diagnostics, but tests and audit must be able to assert these codes.

| Code | Reject when |
|---|---|
| `DEFINITION_BATCH_EMPTY` | The batch contains no operations. |
| `DEFINITION_BATCH_ID_MISMATCH` | Apply, WAL, audit, or recovery evidence uses a different batch id than the accepted dry-run. |
| `DEFINITION_BATCH_SOURCE_HASH_MISMATCH` | Source or import payload hash differs from the accepted dry-run. |
| `DEFINITION_BATCH_GRAPH_HASH_MISMATCH` | Dependency graph hash differs from the accepted dry-run or WAL commit. |
| `DEFINITION_BATCH_OPERATION_COUNT_MISMATCH` | Operation records are missing, extra, duplicated, or not dense. |
| `DEFINITION_BATCH_UNSUPPORTED_OPERATION` | Alter, drop, rename, move, or any unregistered operation class is submitted as a first-class V0 operation. |
| `CATALOG_VERSION_STALE` | Current catalog version no longer equals dry-run `PreviousCatalogVersion`. |
| `CATALOG_VERSION_NON_ADVANCING` | Proposed next version does not strictly advance the previous version. |
| `DEPENDENCY_MISSING` | A referenced dependency target does not exist. |
| `DEPENDENCY_KIND_MISMATCH` | A dependency target exists but has the wrong object kind or version. |
| `DEPENDENCY_CYCLE` | The accepted graph is cyclic. |
| `DEPENDENCY_FORWARD_REFERENCE` | A same-batch dependency points to an operation with the same or later operation index. |
| `CONTRACT_HASH_MISSING` | A procedure operation lacks required contract hash evidence. |
| `CONTRACT_COMPATIBILITY_REJECTED` | Compatibility policy rejects the shape, permission, protocol, result, or identity change. |
| `SECURITY_ADMISSION_DENIED` | Permission, principal, certificate, surface scope, policy version, or fail-closed admission rejects the batch. |
| `SECURITY_DOWNGRADE_REJECTED` | The batch weakens required permissions, policy, auditability, or surface separation without an explicit accepted policy. |
| `PUBLICATION_WAL_BEGIN_MISSING` | Apply or recovery cannot find the required begin record. |
| `PUBLICATION_WAL_COMMIT_MISSING` | Apply or recovery cannot find the required commit record. |
| `PUBLICATION_DURABILITY_NOT_REACHED` | Commit exists but is not in the durable WAL prefix. |
| `PUBLICATION_RECEIPT_MISMATCH` | Receipt fields do not match dry-run, WAL, or catalog visibility evidence. |
| `ROLLBACK_EVIDENCE_MISSING` | A failed pre-commit apply lacks rollback or recovery skip evidence. |
| `REJECTION_CODE_UNSTABLE` | A rejection cannot be mapped to a stable V0 code. |

## Acceptance summary

Owner: DefinitionBatch, catalog publication, WAL/recovery, and security admission implementers.

Evidence: accepted dry-run evidence, dependency graph hash, compatibility matrix, WAL begin/apply/commit records, rollback or recovery-skip traces, publication receipt, security audit trace, and stable rejection-code tests.

Reject: P01 and later implementation work that publishes catalog state outside DefinitionBatch, applies without an accepted dry-run, exposes a half-catalog, advances visibility before durable WAL commit, omits rollback/recovery evidence, weakens security admission, or cannot map failures to stable V0 rejection codes.
