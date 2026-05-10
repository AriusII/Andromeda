# Specification: RecoveryReport v0

> **Status:** Normative V0 specification  
> **Audience:** Andromeda maintainers, engine developers, architects, reviewers  
> **Language:** American English  
> **Baseline:** Rust 1.95.0, Rust 2024 Edition, x64 and ARM64 first

## In this article


- Define the purpose and scope of `RecoveryReport v0`.
- State the required structures.
- State invariants, errors, security, recovery, tests, and rejection criteria.

## Purpose

Define structured recovery output.

## Scope

This specification applies to V0 documentation and implementation planning. It defines the minimum stable contract needed for code, tests, and review.

## Non-goals

- It does not define a final production implementation.
- It does not weaken Andromeda's procedure-only surface.
- It does not authorize hidden dynamic behavior.

## Data structures

| Structure | Required role |
|---|---|
| `RecoveryReport` | Must be represented as an explicit typed structure or canonical descriptor. |
| `StartMode` | Must be represented as an explicit typed structure or canonical descriptor. |
| `OpenMode` | Must be represented as an explicit typed structure or canonical descriptor. |
| `RecoveryWarning` | Must be represented as an explicit typed structure or canonical descriptor. |
| `RecoveryError` | Must be represented as an explicit typed structure or canonical descriptor. |
| `StorageValidationStatus` | Must be represented as an explicit typed structure or canonical descriptor. |

## Invariants

- RecoveryReport is emitted after recovery attempt.
- Warnings and errors are typed.
- OpenMode reflects validation result.
- Report references manifest and WAL range.


## Serialization

- Persisted and network-visible formats use canonical encoding.
- Headers use explicit fixed-width integer fields.
- Variable payloads declare length before payload.
- Critical persisted structures use version fields.
- Rust native struct layout must not be persisted or sent over the wire.

### RecoveryReportV0 schema

| Field | Required rule |
|---|---|
| StartupMode | Requested startup mode, for example fast, safe, or forensic. |
| OpenMode | Accepted open decision: Online, ReadOnly, ForensicOnly, or Reject. |
| ManifestRef | Manifest version, snapshot id, base checkpoint LSN, and required WAL start LSN. |
| PhysicalWalBytes | Physical WAL file bytes inspected. |
| ScannedBytes | Bytes scanned by the WAL reader. |
| DurablePrefixBytes | Bytes accepted as durable prefix. |
| DurablePrefixRecordCount | Number of records in the durable prefix. |
| LastValidWalLsn | Last valid durable WAL LSN; code may expose this as `durable_lsn`. |
| ScanStop | Optional typed scan stop reason. |
| BoundaryKind | Clean, recoverable tail, or forensic chain break. |
| ReplayRecords | Ordered replayable records with LSN, kind, and optional transaction id. |
| IgnoredTransactions | Incomplete or rolled-back transactions skipped during replay. |
| ForensicRequired | True when normal recovery must not continue. |

The report must exist before Online, ReadOnly, or ForensicOnly is accepted. A missing report rejects startup.

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

- report after clean recovery.
- report after truncated WAL.
- ForensicOnly report tests.
- typed warning tests.
- durable prefix and LastValidWalLsn tests.
- ignored transaction partition tests.
- scan stop to open-mode decision tests.

## Rejection criteria

- Reject `open without report`.
- Reject `string-only errors`.
- Reject `missing LastValidWalLsn`.

## Acceptance summary

This specification is acceptable when implementation, tests, and documentation can prove the listed invariants without hidden defaults.
