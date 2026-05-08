# andromeda-tx

## Purpose

`andromeda-tx` owns transaction state, commit and rollback durability gates, MVCC visibility, lock coordination, savepoints, garbage-collection eligibility, transaction traces, and recovery-facing transaction evidence.

The crate enforces the durable-commit doctrine: a transaction becomes visible to other snapshots only after its commit record has been durably flushed to the write-ahead log. Rollback completion also requires durable evidence when the path claims crash-recoverable terminal state.

## Scope

This crate owns:

- Transaction identifiers, lifecycle states, transition traces, and transaction manager behavior.
- Commit-log entries and commit/rollback terminal evidence.
- WAL adapter contracts that append terminal records and verify durable flush coverage.
- MVCC snapshots, row visibility, active snapshot tracking, and GC eligibility.
- Lock manager, strict 2PL validation, deadlock detection, wait fairness, and cleanup evidence.
- Savepoint stacks and bounded write-set metadata.
- Typed transaction and WAL-adapter errors such as `TxWalAdapterError` plus `AndromedaResult` failures with stable error kinds.

## Non-goals

This crate does not own:

- WAL frame bytes or physical `FileWal` format.
- Page, heap, B+Tree, manifest, backup, restore, or storage recovery implementation.
- Procedure dispatch, SRPL execution, catalog publication, or application surface authorization.
- Ad hoc SQL, dynamic predicates, shape-shifting returns, or implicit null semantics.
- GPU, benchmark, analytics, or learned-model output as transaction truth.

## Prerequisites

Before changing this crate, confirm that the change respects these requirements:

- No terminal transaction state is published without durable WAL LSN evidence.
- A successful commit path appends the commit record, flushes through the commit LSN, and only then updates visibility.
- Short WAL flushes and missing terminal evidence fail closed and leave no visible commit or rollback.
- Replay accepts only terminal records covered by the durable WAL prefix.
- MVCC visibility, lock release, savepoint rollback, and GC decisions remain consistent with durable transaction status.
- Error paths use typed errors or `AndromedaResult`, not ambiguous string-only outcomes.

## Procedure

1. Identify whether the change affects lifecycle state, commit log, WAL adapter, MVCC, locks, savepoints, GC, or replay.
2. Preserve the five-step commit order: validate transaction state, append terminal WAL record, flush through the terminal LSN, publish terminal status, and emit evidence.
3. Keep rollback semantics explicit. Paths that do not write durable rollback evidence must not claim crash-recoverable terminal rollback.
4. Validate replay coverage before rebuilding visible status. Reject commit or rollback records whose terminal LSN is not covered by durable WAL.
5. Keep lock and MVCC tests close to the behavior being changed. Visibility changes require snapshot and recovery-oriented coverage, not only unit tests for status mutation.
6. Use typed errors for short flushes, conflicting terminal records, invalid transitions, deadlock routing, and lock protocol violations.

## Validation

Recommended transaction gates:

```powershell
cargo test -p andromeda-tx --test commit_log_gates_final -- --nocapture
cargo test -p andromeda-tx --test commit_log_durability -- --nocapture
cargo test -p andromeda-tx --test tx_wal_replay_recovery -- --nocapture
cargo test -p andromeda-tx --test v0_transaction_lifecycle -- --nocapture
cargo test -p andromeda-tx --test v0_transition_lifecycle -- --nocapture
cargo test -p andromeda-tx --test strict_2pl_contract -- --nocapture
cargo test -p andromeda-tx --test lock_manager_contract -- --nocapture
```

For concurrency-sensitive lock, deadlock, or MVCC changes, add property tests or model checks where the state space is small enough to make ordering defects reproducible. For WAL adapter, replay, or durable status mapping changes, add fuzz coverage for malformed replay inputs where practical. For commit or replay changes, include crash/recovery scenarios that prove visible status reconstructs only from durable WAL evidence.

## Troubleshooting

| Symptom | Check |
| --- | --- |
| A commit is not visible after `record_commit` | Verify `flush_through(commit_lsn)` returned a durable LSN greater than or equal to the commit LSN. |
| A replayed commit is rejected | Check terminal LSN, durable WAL coverage, previous terminal records, and status-table conflicts. |
| A rollback path completes but is not crash-recoverable | Confirm the path wrote durable rollback evidence instead of only clearing in-memory state. |
| GC removes versions too early | Recheck active snapshot registry bounds, terminal status, and durable commit LSN ordering. |
| Lock cleanup leaves stale ownership | Validate strict 2PL release paths, deadlock victim routing, and transaction coordinator cleanup evidence. |

## References

- `src/lib.rs`
- `src/commit_log.rs`
- `src/commit_protocol.rs`
- `src/wal_adapter.rs`
- `src/wal_adapter/`
- `src/mvcc.rs`
- `src/lock_manager.rs`
- `src/savepoint.rs`
- `src/trace.rs`
- `tests/commit_log_gates_final.rs`
- `tests/commit_log_durability.rs`
- `tests/tx_wal_replay_recovery.rs`
- `tests/v0_transaction_lifecycle.rs`
- `tests/strict_2pl_contract.rs`
- `tests/lock_manager_contract.rs`
