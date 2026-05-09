# andromeda-transaction-log

## Purpose

`andromeda-transaction-log` is the C5 owner crate for logical transaction terminal evidence.

This crate owns transaction log record shapes, terminal commit and rollback evidence, replay-facing classification, and transaction-local LSN evidence. Commit orchestration and status publication live in `andromeda-transaction`; physical WAL bytes live in `andromeda-wal`.

## Scope

This crate owns:

- Logical transaction log record shapes and terminal evidence rules.
- Commit and rollback evidence classification over a durable WAL prefix.
- Transaction replay summaries consumed by recovery and visibility publication.
- Typed errors for missing, conflicting, truncated, or non-durable terminal evidence.

## Non-goals

- No application-facing ad hoc SQL.
- No bypass of typed Procedure contracts.
- No ownership of physical WAL file bytes unless a later explicit split assigns it.
- No MVCC visibility, lock scheduling, page flushing, manifest switching, backup, restore, or HA/DR orchestration.
- No GPU output, benchmark output, RAM state, or temporary storage as transaction-log truth.

## Prerequisites

Before adding persistent transaction-log bytes:

- Terminal transaction records must be covered by the durable WAL prefix before they influence visibility.
- Log bytes must use explicit versioned codecs and little-endian canonical serialization.
- Replay must reject broken chains, conflicting terminal states, and non-durable records.
- Mission-critical behavior must include deterministic crash/recovery validation.

## Procedure

1. Define the logical transaction-log responsibility separately from physical WAL storage.
2. Keep `src/lib.rs` limited to module declarations and intentional reexports.
3. Add explicit codecs before accepting persistent or network bytes.
4. Preserve WAL-before-visible-commit ordering for all terminal classifications.
5. Add crash/recovery and corruption-rejection tests before expanding behavior.

## Validation

Run `cargo fmt`, `cargo check`, `cargo clippy`, transaction-log replay tests, codec roundtrip and corruption-rejection tests, and crash/recovery scenarios.

## Troubleshooting

If replay derives a terminal transaction state from bytes outside the durable WAL prefix, fail closed and classify the evidence as invalid.

## References

- `src/lib.rs`
- Commit manager: `crates/andromeda-transaction/`
- Physical WAL owners: `crates/andromeda-wal/`, `crates/andromeda-storage/`
