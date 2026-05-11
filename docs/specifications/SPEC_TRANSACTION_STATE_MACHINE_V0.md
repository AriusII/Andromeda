# Specification: TransactionStateMachine v0

> **Status:** Normative V0 specification  
> **Audience:** Andromeda maintainers, engine developers, architects, reviewers  
> **Language:** American English  
> **Baseline:** Rust 1.95.0, Rust 2024 Edition, x64 and ARM64 first

## In this article


- Define the purpose and scope of `TransactionStateMachine v0`.
- State the required structures.
- State invariants, errors, security, recovery, tests, and rejection criteria.

## Purpose

Define transaction lifecycle states and transitions.

## Scope

This specification applies to V0 documentation and implementation planning. It defines the minimum stable contract needed for code, tests, and review.

## Non-goals

- It does not define a final production implementation.
- It does not weaken Andromeda's procedure-only surface.
- It does not authorize hidden dynamic behavior.

## Data structures

| Structure | Required role |
|---|---|
| `TransactionState` | Must be represented as an explicit typed structure or canonical descriptor. |
| `TransactionTransition` | Must be represented as an explicit typed structure or canonical descriptor. |
| `PoisonReason` | Must be represented as an explicit typed structure or canonical descriptor. |
| `DurableCommitEvidence` | Must be represented as an explicit typed structure or canonical descriptor. |
| `RollbackEvidence` | Must be represented as an explicit typed structure or canonical descriptor. |
| `VisibleCommitFence` | WAL durability fence that must precede visible commit. |
| `TransactionRejectionCode` | Stable typed rejection code for invalid transitions. |
| `RecoveredTransactionEvidence` | Transaction terminal classification reconstructed only from the durable WAL prefix. |

## Invariants

- Committed is visible only after durable WAL.
- `VisibleCommitFence` references the commit LSN and `flush_through` evidence.
- `flush_through_lsn` must be greater than or equal to the terminal record LSN it protects.
- A client ACK, completion trace, MVCC visibility update, catalog publication, map publication, page-visible state, or audit-visible commit is not valid until the commit record is inside the durable WAL prefix.
- Poisoned cannot continue normal execution.
- Disposed is terminal.
- Rollback is typed and observable.
- Invalid transitions emit `TransactionRejectionCode`.
- Recovery reconstructs terminal transaction state from cold snapshot plus durable WAL only. RAM summaries, buffer-pool state, traces, GPU output, and ResultStream payloads are not transaction truth.


## Canonical states and events

| State | Meaning | Normal execution allowed |
|---|---|---|
| `Created` | Transaction handle exists but no durable begin record has been accepted. | No mutation visibility. |
| `Active` | Transaction has begun and may acquire locks and append mutation WAL records. | Yes. |
| `Committing` | Commit was requested and the transaction is in the shrinking phase. | No new locks or mutations. |
| `Committed` | `TxCommit` is covered by durable WAL and may become externally visible. | Terminal visibility only. |
| `Failed` | Execution failed and must roll back before disposal. | No normal mutation execution. |
| `RollingBack` | Rollback was requested and the transaction is in the shrinking phase. | No new locks or mutations. |
| `RolledBack` | Rollback record is covered by durable WAL, or recovery classified the transaction as non-visible. | Terminal rollback only. |
| `Poisoned` | Invariant violation or corruption was observed. | No. |
| `Disposed` | Final cleanup completed. | No. |

| Event | Required durable or logical precondition |
|---|---|
| `Begin` | A transaction id is allocated and no terminal evidence exists for that id. |
| `CommitRequested` | Transaction is `Active`; no rollback, poison, or failed evidence exists. |
| `DurableWalFlushed` | `DurableCommitEvidence` exists and `flush_through_lsn >= commit_record_lsn`. |
| `Fail` | Execution error is typed and associated with the transaction id. |
| `Poison` | A corruption, invariant, security, or recovery condition requires fail-closed handling. |
| `RollbackRequested` | Transaction is `Active`, `Failed`, or `Poisoned`; no visible commit has been published. |
| `RollbackComplete` | `RollbackEvidence` exists and `flush_through_lsn >= rollback_record_lsn`. |
| `Dispose` | Transaction is `Committed` or `RolledBack`; cleanup cannot change the terminal decision. |

## Durable evidence schemas

`VisibleCommitFence` is the only evidence that permits visible commit. Its canonical V0 fields are:

| Field | Required rule |
|---|---|
| `TransactionId` | Non-zero transaction id matching the commit record. |
| `CommitRecordLsn` | Non-zero LSN of the `TxCommit` record. |
| `FlushThroughLsn` | Non-zero durable WAL prefix LSN; must be `>= CommitRecordLsn`. |
| `WalSegmentId` | Segment or file identity that contains `CommitRecordLsn`. |
| `DurablePrefixBytes` | Durable byte offset or byte count proving the record is inside the accepted prefix. |
| `RecordKind` | Must be `TxCommit`; any other record kind rejects visible commit. |
| `TraceId` | Trace evidence for audit and crash-test correlation. |

`DurableCommitEvidence` contains `VisibleCommitFence` plus the terminal transaction state to publish. It must not be synthesized from execution traces, RAM state, or client completion state.

`RollbackEvidence` has the same durability shape as `VisibleCommitFence`, except `RecordKind` must be a rollback terminal record and the resulting terminal state is `RolledBack`.

`RecoveredTransactionEvidence` is emitted by recovery for every transaction id observed in the durable WAL prefix:

| Field | Required rule |
|---|---|
| `TransactionId` | Non-zero transaction id. |
| `FirstLsn` | First durable WAL LSN observed for the transaction. |
| `LastLsn` | Last durable WAL LSN observed for the transaction. |
| `RecordCount` | Count of durable records for the transaction. |
| `RecoveredState` | `Committed`, `RolledBack`, `Incomplete`, or `Invalid`. |
| `TerminalRecordLsn` | Required for `Committed` and `RolledBack`; absent for `Incomplete`. |
| `ReplayDecision` | `ReplayCommitted`, `SkipRolledBack`, `SkipIncomplete`, or `ForensicReject`. |

## Normative transition matrix

| From | Event | Required evidence | To | Rejection code |
|---|---|---|---|---|
| `Created` | `Begin` | Valid transaction id; no prior terminal evidence. | `Active` | `InvalidBegin` |
| `Active` | `CommitRequested` | No rollback/poison/failure evidence; mutation set is closed. | `Committing` | `CommitNotAllowed` |
| `Committing` | `DurableWalFlushed` | `VisibleCommitFence` with `FlushThroughLsn >= CommitRecordLsn`. | `Committed` | `CommitWithoutDurableWal` |
| `Active` | `RollbackRequested` | Typed rollback reason. | `RollingBack` | `RollbackNotAllowed` |
| `Failed` | `RollbackRequested` | Typed failure reason. | `RollingBack` | `RollbackNotAllowed` |
| `Poisoned` | `RollbackRequested` | Typed poison reason and recovery/audit trace. | `RollingBack` | `PoisonContinuationRejected` |
| `RollingBack` | `RollbackComplete` | `RollbackEvidence` with `FlushThroughLsn >= RollbackRecordLsn`. | `RolledBack` | `RollbackWithoutDurableWal` |
| `Active` | `Fail` | Typed execution failure. | `Failed` | `InvalidFailureTransition` |
| `Active` | `Poison` | Typed poison reason. | `Poisoned` | `InvalidPoisonTransition` |
| `Committed` | `Dispose` | Terminal commit evidence already retained. | `Disposed` | `InvalidDispose` |
| `RolledBack` | `Dispose` | Terminal rollback evidence already retained. | `Disposed` | `InvalidDispose` |

All omitted transitions are invalid and must return a typed `TransactionRejectionCode`. A rejected transition must leave the previous state unchanged.

## WAL and recovery mapping

| Durable WAL evidence | Recovery classification | Replay rule |
|---|---|---|
| `TxBegin` only | `Incomplete` | Skip; no visible state may be derived. |
| `TxBegin` plus mutation records without terminal record | `Incomplete` | Skip mutations and record ignored transaction evidence. |
| Mutation records without valid `TxBegin` | `Invalid` | Reject normal recovery and require forensic handling. |
| `TxCommit` inside durable prefix with valid previous chain | `Committed` | Replay committed mutations in LSN order. |
| Rollback terminal record inside durable prefix | `RolledBack` | Skip mutations and record rollback evidence. |
| Terminal record exists only after the durable prefix | `Incomplete` | Ignore the non-durable suffix; no visibility. |
| LSN gap, duplicate or reordered LSN, or previous-LSN mismatch | `Invalid` | Do not replay; require ForensicOnly or Reject open decision. |

Recovery must not upgrade an `Incomplete` transaction to `Committed` because a client ACK, trace completion, page image, or in-memory status exists. The durable WAL prefix is the authority.

## Serialization

- Persisted and network-visible formats use canonical encoding.
- Headers use explicit fixed-width integer fields.
- Variable payloads declare length before payload.
- Critical persisted structures use version fields.
- Rust native struct layout must not be persisted or sent over the wire.

## State transitions

State transitions must be explicit. Invalid transitions return typed errors and emit trace evidence when they affect execution, storage, security, or recovery.

The implementation may expose helper APIs with different internal names, but it must preserve the V0 transition matrix and durable evidence fields. Any helper that publishes visible commit or terminal rollback must receive both the terminal record LSN and the durable WAL prefix LSN, and must reject `durable_lsn < record_lsn`.

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

Security-sensitive operations require admission through identity, principal, permission, policy, and audit checks before durable mutation or transaction creation. GPU and accelerator execution is excluded from the entire transaction kernel, WAL, MVCC visibility, recovery, and security-critical paths (INV-008). The positive policy specifying where GPU MAY be used (advisory analytics outside the commit path) is normative in `docs/specifications/SPEC_GPU_EXECUTION_POLICY_V0.md`.

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

Recovery replays only transaction records inside the durable WAL prefix reported by `RecoveryReportV0.LastValidWalLsn`.

Recovery must:

- validate monotonic LSN order and previous-LSN chain before transaction classification;
- partition transactions into replayable committed transactions and ignored incomplete or rolled-back transactions;
- emit ignored transaction evidence with transaction id, first LSN, last LSN, record count, and reason;
- reject normal recovery when terminal transaction evidence is outside the durable prefix or the WAL chain is inconsistent;
- keep application connections closed when forensic handling is required.

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

- state transition tests.
- poison continuation rejection tests.
- commit visibility tests.
- visible commit fence tests.
- rollback evidence tests.
- stable transaction rejection code tests.
- durable LSN behind commit record LSN rejection tests.
- durable LSN behind rollback record LSN rejection tests.
- replay committed transaction from durable prefix tests.
- skip incomplete transaction after crash-before-commit tests.
- skip rolled-back transaction tests.
- forensic rejection for transaction chain break tests.

## Rejection criteria

- Reject `commit without durable evidence`.
- Reject `commit without VisibleCommitFence`.
- Reject `visible commit when FlushThroughLsn < CommitRecordLsn`.
- Reject `rollback completion when FlushThroughLsn < RollbackRecordLsn`.
- Reject `transition from Disposed`.
- Reject `normal execution after Poisoned`.
- Reject `string-only transaction rejection`.
- Reject `recovery visibility from RAM, trace, page image, client ACK, or ResultStream evidence`.
- Reject `normal recovery after transaction WAL chain break`.

## Acceptance summary

Owner: Person 10 recovery/crash runner/forensic startup owns the durable recovery-facing transaction rules in this spec, including `VisibleCommitFence`, durable terminal evidence, crash reconstruction, and replay/rejection mapping.

Evidence: P01 evidence must include transition matrix tests, visible commit fence tests, durable LSN behind terminal record rejection tests, rollback evidence tests, poison continuation rejection tests, crash-before-commit replay tests, skipped incomplete transaction evidence, and forensic rejection evidence for transaction WAL chain breaks.

Reject: P01 must reject transaction readiness when visible commit can occur without durable WAL, when `FlushThroughLsn < CommitRecordLsn`, when rollback completion lacks durable evidence, when recovery derives visibility from RAM, trace, page image, client ACK, or ResultStream evidence, or when invalid transitions return string-only rejection.
