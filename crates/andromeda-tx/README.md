# andromeda-tx

## Purpose

`andromeda-tx` is a compatibility facade for the extracted transaction family.
It remains in the workspace so topology-approved downstream crates can keep the
historical `andromeda_tx::*` boundary while owner crates continue to stabilize.

The facade owns no transaction behavior. Implementations live in:

- `andromeda-transaction` for lifecycle state, transaction managers, strict 2PL
  coordination, commit protocol, transition traces, and WAL adapter orchestration.
- `andromeda-transaction-log` for transaction-log record shapes, transaction LSNs,
  replay records, and WAL payload contracts.
- `andromeda-mvcc` for snapshots, transaction status visibility, row-version
  visibility, active snapshot tracking, MVCC GC, and reclamation eligibility.
- `andromeda-locking` for lock modes, resources, manager behavior, wait fairness,
  deadlock evidence, and lock history traces.
- `andromeda-savepoint` for savepoint stacks and bounded write-set evidence.

## Scope

This crate keeps only source-compatible root and nested reexports such as:

- `andromeda_tx::TransactionManager`
- `andromeda_tx::CommitLogManager`
- `andromeda_tx::Lsn`
- `andromeda_tx::Snapshot`
- `andromeda_tx::LockManager`
- `andromeda_tx::SavepointStack`
- `andromeda_tx::mvcc::*`
- `andromeda_tx::commit_log::*`
- `andromeda_tx::wal_adapter::*`
- `andromeda_tx::gc::*`

Do not add new transaction behavior here. Add behavior, tests, and docs to the
appropriate owner crate, then expose only a deliberate compatibility reexport
from this facade when an existing downstream path needs it.

## Non-goals

This crate does not own:

- Commit or rollback lifecycle transitions.
- WAL append, flush, replay, or payload encoding behavior.
- MVCC visibility, status tables, GC, or reclamation logic.
- Lock manager behavior, deadlock decisions, or strict 2PL validation.
- Savepoint stack or write-set behavior.
- `andromeda-exec` migration to direct transaction owner dependencies.

## Validation

Facade compatibility:

```powershell
cargo test -p andromeda-tx --test api_compat_reexports --locked
```

Owner-family validation:

```powershell
cargo test -p andromeda-transaction --tests --locked
cargo test -p andromeda-transaction-log --tests --locked
cargo test -p andromeda-mvcc --tests --locked
cargo test -p andromeda-locking --tests --locked
cargo test -p andromeda-savepoint --tests --locked
```

Keep `andromeda-exec` on `andromeda-tx` until the topology gate explicitly
permits direct runtime edges to transaction owner crates.
