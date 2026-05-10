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
| `OpenModeDecision` | Startup decision derived from durable manifest, WAL scan, and report evidence. |
| `ManifestRef` | Durable manifest and cold snapshot anchor used for recovery. |
| `WalScanBoundary` | Typed classification of the durable WAL scan boundary. |
| `RecoveryReplayRecord` | Replayable record selected from the durable WAL prefix. |
| `IgnoredTransaction` | Transaction skipped during replay with typed reason. |

## Invariants

- RecoveryReport is emitted after recovery attempt.
- Warnings and errors are typed.
- OpenMode reflects validation result.
- Report references manifest and WAL range.
- The report must be built from cold snapshot, manifest, and durable WAL scan evidence. RAM, traces, buffer-pool pages, GPU output, and client ACK state are not report truth.
- The report must exist before `Online`, `ReadOnly`, or `ForensicOnly` is accepted. A missing report rejects startup.
- `LastValidWalLsn` is the normative field name for the highest durable WAL LSN; code may expose the same value as `durable_lsn` only through an explicit mapping.
- `ForensicOnly` blocks application connections and permits only operator inspection, report export, backup/restore tooling, and explicitly authorized forensic commands.


## Serialization

- Persisted and network-visible formats use canonical encoding.
- Headers use explicit fixed-width integer fields.
- Variable payloads declare length before payload.
- Critical persisted structures use version fields.
- Rust native struct layout must not be persisted or sent over the wire.

### RecoveryReportV0 schema

| Field | Required rule |
|---|---|
| `ReportVersion` | Must be `0` for this schema. |
| `ReportId` | Stable identifier for retained evidence and audit correlation. |
| `TraceId` | Trace id for the startup or recovery attempt. |
| `StartupMode` | Requested startup mode: `FastStart`, `SafeStart`, or `ForensicStart`. |
| `OpenModeDecision` | Full decision object defined below. |
| `ManifestRef` | Manifest version, snapshot id, base checkpoint LSN, and required WAL start LSN. |
| `WalRef` | WAL identity, segment/file id, physical byte length, and scan command or fixture id. |
| `PhysicalWalBytes` | Physical WAL file bytes inspected. |
| `ScannedBytes` | Bytes scanned by the WAL reader. |
| `DurablePrefixBytes` | Bytes accepted as durable prefix. |
| `DurablePrefixRecordCount` | Number of records in the durable prefix. |
| `LastValidWalLsn` | Highest valid durable WAL LSN; internal code name `durable_lsn` must map to this field. |
| `ScanStop` | Optional typed scan stop reason with byte offset. |
| `BoundaryKind` | `Clean`, `RecoverableTail`, or `ForensicChainBreak`. |
| `StorageValidationStatus` | `Valid`, `Degraded`, `ForensicRequired`, or `Rejected`. |
| `ReplayRecords` | Ordered replayable records with LSN, kind, transaction id, and replay target. |
| `IgnoredTransactions` | Incomplete or rolled-back transactions skipped during replay. |
| `Warnings` | Ordered typed `RecoveryWarning` entries. |
| `Errors` | Ordered typed `RecoveryError` entries. |
| `ForensicRequired` | True when normal recovery must not continue. |
| `ReportPersisted` | True only after the report has been retained before open-mode acceptance. |

The report must exist before Online, ReadOnly, or ForensicOnly is accepted. A missing report rejects startup.

### ManifestRef

| Field | Required rule |
|---|---|
| `ManifestVersion` | Version of the selected durable manifest. |
| `SnapshotId` | Non-zero cold snapshot identifier mounted for recovery. |
| `BaseCheckpointLsn` | Checkpoint LSN from which WAL replay is evaluated. |
| `RequiredWalStartLsn` | Non-zero LSN at which WAL coverage must begin. |
| `ManifestHash` | Hash or canonical identity of the selected manifest. |
| `PreviousManifestHash` | Present when previous-root fallback is part of the decision. |

### WalScanBoundary

| BoundaryKind | Scan stop examples | Open-mode impact |
|---|---|---|
| `Clean` | No scan stop. | `FastStart` may become `Online`; `SafeStart` may become `ReadOnly` or `Online` by policy. |
| `RecoverableTail` | Truncated header, truncated record, truncated payload, checksum mismatch in suffix. | `FastStart` rejects; `SafeStart` may become `ReadOnly` after durable prefix replay. |
| `ForensicChainBreak` | LSN gap, duplicate or reordered LSN, previous-LSN mismatch, manifest anchor not covered. | Normal open rejects; `ForensicStart` may become `ForensicOnly` only after report retention. |

### OpenModeDecision

| Field | Required rule |
|---|---|
| `RequestedStartupMode` | `FastStart`, `SafeStart`, or `ForensicStart`. |
| `AcceptedOpenMode` | `Online`, `ReadOnly`, `ForensicOnly`, or `Reject`. |
| `DecisionReason` | Typed reason, never a string-only explanation. |
| `ReplayAllowed` | True only for `Online` or `ReadOnly` decisions with valid durable prefix coverage. |
| `ApplicationConnectionsAllowed` | True only for `Online`; false for `ReadOnly`, `ForensicOnly`, and `Reject` unless a separate read-only application policy is explicitly defined. |
| `ReportRequired` | Always true for every accepted open mode. |
| `ReportPersistedBeforeOpen` | Must be true before `Online`, `ReadOnly`, or `ForensicOnly` is accepted. |

| Durable evidence and requested mode | Required decision |
|---|---|
| Valid manifest, cold snapshot, clean WAL, `FastStart`, persisted report | `Online` |
| Valid manifest, cold snapshot, recoverable tail, `FastStart` | `Reject` with `FastStartRequiresCleanScan` |
| Valid manifest, cold snapshot, recoverable tail, `SafeStart`, persisted report | `ReadOnly` unless policy explicitly permits `Online` after durable prefix replay. |
| Forensic chain break under `FastStart` or `SafeStart` | `Reject` with `ForensicHandlingRequired` |
| Forensic chain break under `ForensicStart`, persisted report | `ForensicOnly` |
| Missing report for any accepted mode | `Reject` with `MissingRecoveryReport` |
| Manifest not validated, missing cold snapshot, or WAL does not cover `RequiredWalStartLsn` | `Reject` with typed storage validation error. |

### ReplayRecords and IgnoredTransactions

`ReplayRecords` contains only records inside the durable WAL prefix and in increasing LSN order. Each row records `Lsn`, `Kind`, `TransactionId` when applicable, `ReplayTarget`, and `Decision`.

`IgnoredTransactions` contains every non-replayed transaction observed in the durable prefix:

| Field | Required rule |
|---|---|
| `TransactionId` | Non-zero transaction id. |
| `Reason` | `Incomplete` or `RolledBack`. |
| `FirstLsn` | First durable LSN for the transaction. |
| `LastLsn` | Last durable LSN for the transaction. |
| `RecordCount` | Count of durable records observed for the transaction. |

## State transitions

State transitions must be explicit. Invalid transitions return typed errors and emit trace evidence when they affect execution, storage, security, or recovery.

Recovery report lifecycle is:

```text
RecoveryAttemptStarted -> DurableInputsScanned -> ReportBuilt -> ReportPersisted -> OpenModeDecided
RecoveryAttemptStarted -> DurableInputsScanned -> ReportBuildRejected -> OpenModeRejected
```

`OpenModeDecided` is invalid unless `ReportPersisted` happened first. `ForensicOnly` is an accepted open mode for inspection, not an application-serving mode.

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
ReportId
StartupMode
AcceptedOpenMode
LastValidWalLsn
BoundaryKind
```

## Recovery behavior

Recovery report production must:

- validate the manifest and cold snapshot anchor before accepting replay;
- scan the WAL into a durable prefix and optional typed scan stop;
- classify the boundary as `Clean`, `RecoverableTail`, or `ForensicChainBreak`;
- reject normal replay if `LastValidWalLsn < RequiredWalStartLsn`;
- derive replay and ignored-transaction partitions only from durable prefix records;
- persist the report before any accepted open mode is returned;
- emit `ForensicOnly` or `Reject` instead of silently repairing uncertain state.

Forensic startup must:

- keep procedure execution and application connections closed;
- preserve the report and durable bytes inspected;
- expose operator-visible audit projection with `TraceId`, `ReportId`, `ManifestRef`, `LastValidWalLsn`, `BoundaryKind`, and decision reason;
- require restore, PITR, or manual operator action before normal service resumes.

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
- missing report rejects open-mode acceptance tests.
- `LastValidWalLsn` / `durable_lsn` field mapping tests.
- `FastStart` rejects recoverable tail tests.
- forensic chain break blocks `Online` and `ReadOnly` tests.
- `ForensicStart` requires persisted report tests.
- manifest anchor not covered by durable WAL rejects startup tests.
- application connections closed under `ForensicOnly` tests.

## Rejection criteria

- Reject `open without report`.
- Reject `string-only errors`.
- Reject `missing LastValidWalLsn`.
- Reject `accepted Online or ReadOnly after forensic chain break`.
- Reject `ForensicOnly without persisted report`.
- Reject `report derived from RAM-only or trace-only evidence`.
- Reject `LastValidWalLsn below RequiredWalStartLsn`.
- Reject `replay records outside the durable WAL prefix`.
- Reject `silent repair without OpenModeDecision`.

## Acceptance summary

Owner: Person 10 recovery/crash runner/forensic startup owns `RecoveryReportV0`, `OpenModeDecision`, and forensic startup wording for P01.

Evidence: P01 evidence must include typed report schema coverage, clean/truncated/chain-break WAL scan tests, `LastValidWalLsn` mapping assertions, report-before-open assertions, ignored transaction partition assertions, and retained startup decision artifacts that name `Online`, `ReadOnly`, `ForensicOnly`, or `Reject`.

Reject: P01 must reject recovery readiness when startup can open without a persisted report, when `ForensicOnly` permits application execution, when `Online` or `ReadOnly` follows a forensic chain break, when replay uses bytes outside the durable WAL prefix, or when report fields/errors are string-only.
