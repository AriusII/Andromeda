# MvccIsolation v0 Specification

## Purpose

Define the current MVCC isolation contract implemented by `andromeda-tx`.

This specification documents the behavior that is supported today. It does not
promote the `Serializable` commit-log label into a serializable execution
guarantee, and it does not infer visibility from RAM-only state, timestamps, or
undurable transaction outcomes.

## Scope

This specification applies to the current transaction crate MVCC surface:

- `MvccIsolationPolicy` in `crates/andromeda-tx/src/mvcc_snapshot.rs`;
- `Snapshot` validation and active transaction membership;
- `MvccRowHeader::visible_in_snapshot` in
  `crates/andromeda-tx/src/mvcc_version.rs`;
- `TransactionStatusTable` durable terminal status gates in
  `crates/andromeda-tx/src/mvcc_status.rs`;
- `ActiveSnapshotRegistry` retention-frontier behavior in
  `crates/andromeda-tx/src/active_snapshot_registry.rs`;
- `IsolationLevel` commit metadata in
  `crates/andromeda-tx/src/commit_log/entry.rs`.

It covers row-version visibility, rollback invisibility, long-reader retention,
and anomaly labels for the current V0 behavior.

## Non-goals

This specification does not:

- introduce application-facing ad hoc SQL;
- bypass typed Procedure contracts;
- make a transaction visible before durable WAL evidence;
- define a persisted MVCC row byte format;
- add a new runtime anomaly API;
- claim Serializable isolation for the current MVCC visibility engine;
- define predicate locks, range locks, SSI, deterministic scheduling, or
  write-write conflict prevention;
- change recovery, WAL, lock-manager, or catalog behavior.

## Prerequisites

Reviewers must use these references before accepting changes to this spec:

- `AGENTS.md` for Andromeda doctrine and durable-commit invariants.
- `crates/AGENTS.md` for Rust crate boundaries and test locality.
- `documentations/03_TRANSACTION_WAL_MVCC_STORAGE_RECOVERY.md` for conceptual
  MVCC and anomaly terminology.
- `documentations/specs/WalRecord_v0.md` for WAL-before-visible-commit
  doctrine.
- `crates/andromeda-tx/src/mvcc_snapshot.rs` for supported MVCC visibility
  policies.
- `crates/andromeda-tx/src/mvcc_version.rs` for row-version visibility rules.
- `crates/andromeda-tx/src/mvcc_status.rs` for durable terminal status rules.
- `crates/andromeda-tx/src/active_snapshot_registry.rs` for long-reader
  retention-frontier rules.
- `crates/andromeda-tx/src/commit_log/entry.rs` for commit-log isolation
  metadata labels.
- `crates/andromeda-tx/tests/v0_transaction_lifecycle/mvcc_visibility.rs` and
  `crates/andromeda-tx/tests/mvcc_isolation_anomaly_contract.rs` for executable
  contract evidence.

## Procedure

### Ownership

`andromeda-tx` owns the transaction-local MVCC visibility decision for row
versions. Visibility is a function of:

1. the row version's creator and optional deleter transaction ids;
2. the row version's logical `begin_ts` and optional `end_ts`;
3. the reader `Snapshot`;
4. the durable terminal evidence mirrored into `TransactionStatusTable`.

No caller may treat a timestamp by itself as commit evidence. A transaction is
visible to other snapshots only when `TransactionStatusTable` records it as
`Committed` through durable WAL evidence, or when the snapshot belongs to the
same in-flight transaction and is reading its own writes.

### Supported MVCC visibility policies

The current `MvccIsolationPolicy` enum supports exactly two row-visibility
policies.

| Policy | Current behavior | Prevented by current MVCC visibility | Not guaranteed by current MVCC visibility |
| --- | --- | --- | --- |
| `ReadCommitted` | Any durably committed creator is visible when `begin_ts <= snapshot.timestamp`. `active_tx_ids` is ignored for visibility and retained for diagnostics. | Dirty reads from other transactions. Rolled-back creators and rolled-back delete intents are invisible. | Repeatable reads, predicate stability, write skew prevention, lost update prevention, Serializable ordering. |
| `RepeatableRead` | Snapshot-isolation visibility. A writer that was active when the snapshot was created remains invisible after it commits, except to its own transaction. | Dirty reads from other transactions. Re-reading the same fixed snapshot does not observe concurrent writers listed in `active_tx_ids`. Versions and deletes after the snapshot timestamp are not observed. | Serializable ordering, write skew prevention, predicate/range conflict detection, lost update prevention unless supplied by another layer. |

`RepeatableRead` in this crate should be read as snapshot isolation over the
current row-version API. It must not be documented as Serializable.

### Commit-log isolation metadata

The current `IsolationLevel` enum supports two commit metadata labels:

| Label | Current implementation meaning |
| --- | --- |
| `Snapshot` | The commit record stores a snapshot-classification label. MVCC visibility is still evaluated through `MvccIsolationPolicy`, row timestamps, and durable status evidence. |
| `Serializable` | The commit record can store the label, but `andromeda-tx` does not currently turn that label into Serializable execution. No predicate locks, SSI validation, deterministic serialization graph, or anomaly-proof scheduler is implemented by this MVCC visibility path. |

The `Serializable` label is durable metadata, not proof that the transaction
executed under Serializable isolation. A future implementation may bind this
label to strict 2PL, SSI, deterministic scheduling, or another reviewed
mechanism, but this V0 spec must not overclaim it.

### Visibility rules

A row version is visible to a snapshot only if all required conditions hold:

1. The snapshot and row header validate successfully.
2. The creator is visible:
   - `Committed` creators are visible under `ReadCommitted`;
   - `Committed` creators are visible under `RepeatableRead` only when they
     were not active at snapshot creation, except for the snapshot owner;
   - `InFlight` creators are visible only to their own transaction;
   - `RolledBack` creators are never visible.
3. `begin_ts <= snapshot.timestamp`, unless the creator is the snapshot owner.
4. If the row version is closed, the delete is observed only when the deleter is
   visible and `end_ts <= snapshot.timestamp`.
5. A rolled-back delete intent is not observed, so the older version remains
   visible when its creator is visible.

An unrecorded creator is treated as `InFlight` by
`TransactionStatusTable::status_for_snapshot`. That fail-closed default
preserves the durable-commit doctrine.

### Snapshot validation

`Snapshot::with_context_validated` verifies the snapshot against a live
`TransactionStatusTable`:

- the snapshot owner must be registered as `InFlight`;
- every transaction listed in `active_tx_ids` must be either unknown or
  explicitly `InFlight`;
- terminal transactions must not appear in `active_tx_ids`.

`Snapshot::with_context` validates shape only. It sorts and deduplicates
`active_tx_ids`, but it does not prove the active set against the status table.

### Long readers and retention frontier

`ActiveSnapshotRegistry` tracks `SnapshotHandle { begin_ts, tx_id }` values and
publishes the oldest active `begin_ts` as `minimum_visible_timestamp()`.

The current retention rule is strict:

```text
version eligible for timestamp-based reclamation only when end_ts < minimum_visible_timestamp()
```

When no snapshots are active, `minimum_visible_timestamp()` returns `u64::MAX`.
Long readers therefore pin old versions until their snapshot handles are
released. A version with `end_ts` equal to the frontier is not reclaimable.

### Anomaly classification

The current crate does not expose an `Anomaly` enum or a runtime anomaly-label
API. This specification uses anomaly names as documentation and test labels.

| Anomaly label | Current `ReadCommitted` status | Current `RepeatableRead` status | Notes |
| --- | --- | --- | --- |
| Dirty read | Prevented for other transactions. | Prevented for other transactions. | In-flight writes are visible only to the owning transaction. |
| Non-repeatable read | Allowed by design when a caller observes a fresh durable status between statements. | Prevented for a fixed snapshot when the concurrent writer was listed in `active_tx_ids`. | `ReadCommitted` callers should refresh statement snapshots intentionally. |
| Read-your-writes | Supported. | Supported. | The snapshot owner can see its own in-flight writes. |
| Rolled-back write visibility | Prevented. | Prevented. | Rolled-back creators are never visible; rolled-back delete intents are ignored. |
| Version after snapshot | Prevented for a fixed snapshot by `begin_ts`. | Prevented for a fixed snapshot by `begin_ts`. | This is row-version timestamp behavior, not predicate serializability. |
| Delete after snapshot | Prevented for a fixed snapshot by `end_ts`. | Prevented for a fixed snapshot by `end_ts`. | The prior version remains visible. |
| Write skew | Not prevented by MVCC visibility alone. | Not prevented by MVCC visibility alone. | Requires additional write-conflict, predicate, lock, or Procedure validation. |
| Lost update | Not classified as prevented by MVCC visibility alone. | Not classified as prevented by MVCC visibility alone. | Must be handled by write-write conflict control or locking. |
| Predicate phantom | Not guaranteed. | Not globally guaranteed. | A fixed row-version snapshot hides versions after the timestamp, but the crate does not implement predicate or range conflict detection. |
| Serializable anomaly freedom | Not supported. | Not supported. | `IsolationLevel::Serializable` is metadata until enforcement exists. |

## Validation

Documentation acceptance checks:

- The spec states only the two implemented `MvccIsolationPolicy` variants:
  `ReadCommitted` and `RepeatableRead`.
- The spec distinguishes `MvccIsolationPolicy` from commit-log
  `IsolationLevel` labels.
- The spec explicitly says `IsolationLevel::Serializable` is not an
  implemented Serializable guarantee.
- The spec preserves WAL-before-visible-commit.
- The spec preserves rollback invisibility.
- The spec preserves the strict long-reader frontier rule:
  `end_ts < minimum_visible_timestamp()`.
- The spec does not introduce SQL, gRPC, runtime JSON, GPU decision paths, or
  Procedure contract bypasses.

Executable validation:

```powershell
cargo fmt --all --check
cargo test -p andromeda-tx --test mvcc_isolation_anomaly_contract
cargo test -p andromeda-tx --test v0_transaction_lifecycle
```

Broader validation before changing runtime MVCC behavior:

```powershell
cargo check --workspace --all-targets --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo nextest run --workspace --all-features
```

Runtime changes to commit, WAL, rollback, recovery, MVCC visibility, catalog
publication, or GC reclamation require targeted crash/recovery or property
tests appropriate to the changed behavior.

## Troubleshooting

| Symptom | Likely cause | Corrective action |
| --- | --- | --- |
| Documentation claims Serializable is implemented. | Commit metadata was confused with concurrency enforcement. | Reword `Serializable` as a stored label until enforcement code and tests exist. |
| A row is visible without durable commit evidence. | Visibility inferred from timestamp or missing status. | Treat missing status as `InFlight` and require durable terminal evidence. |
| A rolled-back writer is visible. | Terminal status handling regressed. | Verify `TransactionStatus::RolledBack` returns invisible for creators and delete intents. |
| A repeatable-read snapshot observes a writer listed in `active_tx_ids`. | Snapshot membership gating regressed. | Verify `RepeatableRead` checks active membership except for read-your-writes. |
| A long reader loses a required old version. | Retention frontier used `<=` instead of strict `<`. | Reclaim only when `end_ts < minimum_visible_timestamp()`. |
| `active_tx_ids` contains a terminal transaction. | Snapshot was built without status validation or status validation regressed. | Use `Snapshot::with_context_validated` when live manager evidence is available. |
| An anomaly table reads like a proof of Serializable behavior. | Documentation is describing desired future behavior as current support. | Split current MVCC behavior from future Serializable enforcement work. |

## References

- `AGENTS.md`
- `crates/AGENTS.md`
- `documentations/03_TRANSACTION_WAL_MVCC_STORAGE_RECOVERY.md`
- `documentations/ROADMAP_IMPLEMENTATION_2026.md`
- `documentations/specs/WalRecord_v0.md`
- `crates/andromeda-tx/README.md`
- `crates/andromeda-tx/src/mvcc_snapshot.rs`
- `crates/andromeda-tx/src/mvcc_version.rs`
- `crates/andromeda-tx/src/mvcc_status.rs`
- `crates/andromeda-tx/src/active_snapshot_registry.rs`
- `crates/andromeda-tx/src/commit_log/entry.rs`
- `crates/andromeda-tx/tests/v0_transaction_lifecycle/mvcc_visibility.rs`
- `crates/andromeda-tx/tests/v0_transaction_lifecycle/snapshot_validation.rs`
- `crates/andromeda-tx/tests/mvcc_eligibility_contract/long_reader.rs`
- `crates/andromeda-tx/tests/mvcc_eligibility_contract/retention_boundary.rs`
- `crates/andromeda-tx/tests/mvcc_isolation_anomaly_contract.rs`
