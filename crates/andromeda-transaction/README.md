# andromeda-transaction

## Purpose

`andromeda-transaction` is the C5 owner crate for transaction lifecycle coordination.

This crate owns transaction identifiers, lifecycle state transitions, terminal commit and rollback policy, WAL adapter coordination, and durable visibility evidence.

## Scope

This crate owns:

- Transaction lifecycle state and transition validation.
- Commit and rollback terminal-state publication gates.
- Coordination with WAL, MVCC, locking, savepoint, recovery, and observability crates.
- Typed errors for invalid transitions, short durability, conflicting terminal evidence, and recovery-inconsistent state.

## Non-goals

- No application-facing ad hoc SQL.
- No bypass of typed Procedure contracts.
- No physical WAL frame or file format ownership.
- No page, heap, manifest, backup, restore, or HA/DR implementation.
- No GPU output, benchmark output, RAM state, or temporary storage as transaction truth.

## Prerequisites

Before behavior lands here:

- A commit record must be durably flushed to WAL before commit visibility is published.
- Rollback paths that claim crash-recoverable terminal state must have durable evidence.
- Persistent and network bytes must use explicit codecs, not Rust native struct layout.
- Mission-critical behavior must include deterministic crash/recovery validation.

## Procedure

1. Define the precise transaction responsibility being split.
2. Keep `src/lib.rs` limited to module declarations and intentional reexports.
3. Add typed errors before exposing public behavior.
4. Prove WAL-before-visible-commit ordering before any visibility publication is accepted.
5. Add crash/recovery coverage before moving durable transaction behavior.

## Validation

Run `cargo fmt`, `cargo check`, `cargo clippy`, focused transaction tests, and crash/recovery scenarios that reconstruct visible state only from durable WAL evidence.

## Troubleshooting

If a future transaction path publishes visibility without durable WAL coverage, treat it as a C5 invariant violation and fail closed.

## References

- `src/lib.rs`
- Transaction log owner: `crates/andromeda-transaction-log/`
