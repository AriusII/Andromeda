# andromeda-savepoint

## Purpose

`andromeda-savepoint` is the C5 owner crate for savepoint stacks and partial rollback metadata.

This crate owns savepoint naming, nesting, write-set bounds, and partial rollback evidence. `andromeda-tx` reexports these types during the transaction extraction so existing callers keep their import paths.

## Scope

This crate owns:

- Savepoint identifiers, nesting rules, stack operations, and bounded metadata.
- Partial rollback planning within an active transaction.
- Evidence needed to coordinate savepoint rollback with MVCC, locking, and heap changes.
- Typed errors for invalid nesting, stale savepoints, write-set bound violations, and rollback conflicts.

## Non-goals

- No application-facing ad hoc SQL.
- No bypass of typed Procedure contracts.
- No ownership of full transaction terminal status, WAL bytes, MVCC visibility, locks, heap storage, recovery replay, backup, restore, or HA/DR.
- No GPU output, benchmark output, RAM state, or temporary storage as savepoint truth.

## Prerequisites

Before adding durable savepoint behavior:

- Savepoint rollback must not publish transaction visibility before durable commit evidence exists.
- Any persistent savepoint evidence must use explicit codecs and versioned byte contracts.
- Partial rollback must leave recovery, MVCC, and lock cleanup decisions explainable from durable and bounded evidence.
- Mission-critical behavior must include deterministic crash/recovery validation where durable state is affected.

## Procedure

1. Define the savepoint responsibility being split.
2. Keep `src/lib.rs` limited to module declarations and intentional reexports.
3. Bound savepoint metadata and write-set evidence.
4. Keep transaction terminal-state authority outside savepoint internals.
5. Add rollback, bound, and recovery-adjacent tests before expanding behavior.

## Validation

Run `cargo fmt`, `cargo check`, `cargo clippy`, focused savepoint tests, rollback integration tests, and crash/recovery coverage for durable savepoint evidence.

## Troubleshooting

If savepoint rollback makes later commit or recovery state ambiguous, reject the path until terminal transaction and WAL evidence are explicit.

## References

- `src/lib.rs`
- Compatibility facade: `crates/andromeda-tx/`
