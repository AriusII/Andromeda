# Transaction And Recovery

## Purpose

This spec defines durable transaction evidence, MVCC visibility, recovery
reports, recovery traces, and Procedure invocation trace boundaries. It does
not define WAL bytes or page bytes; those are owned by the storage and WAL
spec.

## Durable commit evidence

A transaction may become visible as committed only when durable WAL evidence
proves a valid `TxCommit` record in the accepted durable prefix.

| Evidence | Required rule |
| --- | --- |
| `transaction_id` | Nonzero and matches WAL, commit-log, state-machine, and projection fields. |
| `terminal_state` | Exactly `Committed`. |
| `previous_transaction_state` | Live publication moves only from `Committing` to `Committed`; recovery must not fabricate prior runtime state. |
| `commit_record_lsn` | Nonzero LSN of the transaction `TxCommit` record. |
| `durable_commit_lsn` | Nonzero and greater than or equal to `commit_record_lsn`. |
| `durable_prefix_lsn` | Highest accepted checksum-valid WAL prefix used by this evidence. |
| `required_wal_start_lsn` | Manifest recovery anchor when reconstructed during startup. |
| `recovery_frontier_lsn` | Highest LSN accepted by recovery for normal visibility. |
| `chain_validation` | LSN order, previous LSN, length, record checksum, and header checksum validated through the commit record. |

The transaction summary must contain a `TxBegin`, exactly one `TxCommit`, no
`TxRollback`, and no conflicting terminal record in the accepted replay range.
Redo-relevant mutations made visible by the commit must have LSNs lower than
the commit record and inside the same accepted durable prefix.

Audit, trace, Procedure Store, result metadata, and completion projections may
carry a durable commit LSN only after commit evidence validates.

## Durable rollback evidence

A transaction may become terminally rolled back only when durable WAL evidence
proves a valid `TxRollback` record in the accepted durable prefix.

| Evidence | Required rule |
| --- | --- |
| `transaction_id` | Nonzero and matches WAL, rollback-log, state-machine, and projection fields. |
| `terminal_state` | Exactly `RolledBack`. |
| `previous_transaction_state` | Live evidence moves only from `RollingBack` to `RolledBack`; recovery must not fabricate prior runtime state. |
| `rollback_record_lsn` | Nonzero LSN of the transaction `TxRollback` record. |
| `durable_rollback_lsn` | Nonzero and greater than or equal to `rollback_record_lsn`. |
| `durable_prefix_lsn` | Highest accepted checksum-valid WAL prefix used by this evidence. |
| `chain_validation` | LSN order, previous LSN, length, record checksum, and header checksum validated through the rollback record. |

The transaction summary must contain a `TxBegin`, exactly one `TxRollback`, no
`TxCommit`, and no conflicting terminal record in the accepted replay range.
Redo-relevant mutations for a rolled-back transaction must be skipped and must
not become MVCC-visible.

## MVCC visibility

Current MVCC visibility supports `ReadCommitted` and `RepeatableRead`.
`Serializable` may appear as commit-log metadata, but this path does not
implement predicate locks, SSI validation, a serialization graph, or an
anomaly-proof scheduler.

| Policy | Current behavior | Not guaranteed by this path |
| --- | --- | --- |
| `ReadCommitted` | A durably committed creator is visible when `begin_ts` is at or before the statement snapshot timestamp. Rolled-back creators and delete intents are invisible. | Repeatable reads, predicate stability, write skew prevention, lost update prevention, Serializable ordering. |
| `RepeatableRead` | Fixed snapshot visibility. A transaction active when the snapshot was created remains invisible after it commits, except to its own transaction. | Serializable ordering, write skew prevention, predicate or range conflict detection, lost update prevention unless another layer supplies it. |

Read-your-writes is supported. Dirty reads from other transactions are
prevented. Long-reader retention may reclaim only versions whose `end_ts` is
strictly lower than the minimum visible timestamp.

## Recovery modes

| Mode | Application traffic | Replay behavior | Required report behavior |
| --- | --- | --- | --- |
| `FastStart` | Allowed only after minimal gates pass. | Replay complete valid WAL needed for visibility. | Emit normal startup evidence and skipped deep-check warnings. |
| `SafeStart` | Allowed only after reinforced gates pass. | Replay complete valid WAL and verify stronger catalog, manifest, and storage invariants. | Emit evidence for every reinforced gate. |
| `ForensicStart` | Blocked. | Inspection-only unless explicitly documented as non-mutating. | Emit forensic report, corruption boundary, and traffic-block evidence. |
| `RestoreValidation` | Blocked until restore is accepted. | Replay selected snapshot and WAL archive into an isolated target. | Emit snapshot, manifest, WAL archive, and target evidence. |
| `RefuseStart` | Blocked. | None. | Emit refusal reason, first failed gate, and operator-safe next action. |

Outcomes are `RecoveredOnline`, `RecoveredReadOnly`, `RecoveredDegraded`,
`ForensicOnly`, `RestoreRequired`, or `Refused`. Write access is allowed only
when the selected outcome and policy permit it.

## Recovery report and trace

Recovery reports and traces must carry identity, requested and effective mode,
source manifest or snapshot, storage format fingerprints, WAL boundaries,
replay counts, transaction classifications, corruption boundaries, validation
results, audit links, and outcome.

Required WAL boundaries include first inspected LSN, last readable LSN, last
valid record LSN, last durable LSN, corruption boundary LSN when known, and
target restore LSN when applicable.

Raw WAL bodies, page bodies, secrets, and unbounded diagnostics must not appear
in reports or traces. Use typed ids, LSNs, offsets, lengths, fixed-size
digests, reason codes, and bounded sanitized excerpts.

## Corruption handling

| Condition | Required outcome |
| --- | --- |
| WAL tail truncation | Stop at the last complete valid record and report the failed boundary. |
| WAL middle corruption | Stop replay at the corruption boundary; require forensic mode, restore, or refusal. |
| Unknown WAL payload format | Reject mutation; allow read-only forensic inspection only when policy permits. |
| Manifest corruption | Use a previous valid manifest only when chain, retention, and policy evidence permit it; otherwise refuse or enter forensic mode. |
| Snapshot, page, or segment corruption | Reconstruct only from validated WAL or snapshot evidence; otherwise preserve forensic evidence. |
| Catalog invariant failure | Do not publish catalog state; open read-only, forensic-only, or refuse according to policy. |

Audit and traces are forensic correlation. They must never re-authorize work,
execute a Procedure, or reconstruct database truth without WAL-backed evidence.

## Procedure invocation trace

Procedure invocation traces must record contract binding, surface, protocol
object, admission, transaction binding, WAL append or flush, terminal commit or
rollback evidence, completion, and recovery comparison when applicable.

The required order is:

1. Protocol and contract admission accepted, with no transaction evidence.
2. Security audited and authorized, with no transaction evidence.
3. Resource or IO admitted, with no transaction evidence.
4. Transaction bound only after admission and authorization.
5. WAL append or flush evidence recorded for durability claims.
6. Commit visible only with durable commit LSN matching prior WAL evidence.
7. Rollback durable only with durable rollback LSN matching prior WAL evidence.
8. Completion emitted with committed flag and durable LSN when terminal.

Frame, contract, surface, security, and resource rejection before transaction
binding must return `NoTransaction` evidence and must not create a transaction.

## Validation gates

- Commit visibility tests must reject `Committed` without durable commit LSN.
- Rollback tests must reject `RolledBack` without durable rollback LSN.
- Replay tests must reject conflicting terminal records.
- MVCC tests must separate current RepeatableRead behavior from future
  Serializable enforcement.
- Recovery tests must prove read-only forensic behavior and raw-body exclusion.
