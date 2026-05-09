# andromeda-mvcc

## Purpose

`andromeda-mvcc` is the C5 owner crate for snapshot visibility and version lifecycle policy.

This crate owns MVCC snapshots, version visibility, active-reader tracking, and version garbage-collection eligibility.

## Scope

This crate owns:

- Snapshot identifiers, visibility decisions, and reader lifetime evidence.
- Version creation, retirement, and garbage-collection eligibility rules.
- Interactions between durable transaction status and visible row versions.
- Typed errors for invalid snapshots, missing durable status, stale readers, and unsafe GC decisions.

## Non-goals

- No application-facing ad hoc SQL.
- No bypass of typed Procedure contracts.
- No ownership of WAL bytes, page formats, heap storage, lock scheduling, or savepoint stacks.
- No catalog publication, backup, restore, or HA/DR implementation.
- No GPU output, benchmark output, RAM state, or temporary storage as MVCC truth.

## Prerequisites

Before behavior lands here:

- Visibility must depend on durable transaction evidence, not in-memory status alone.
- A committed version must not become visible before the commit WAL record is durable.
- Persistent and network bytes must use explicit codecs, not Rust native struct layout.
- MVCC behavior that affects recovery, GC, or visibility must include crash/recovery validation.

## Procedure

1. Define the visibility responsibility being split.
2. Keep `src/lib.rs` limited to module declarations and intentional reexports.
3. Make durable transaction status an explicit input to visibility decisions.
4. Reject GC decisions that cannot be explained from durable state and active-reader evidence.
5. Add snapshot, anomaly, and crash/recovery tests before moving behavior.

## Validation

Run `cargo fmt`, `cargo check`, `cargo clippy`, MVCC visibility tests, anomaly-oriented tests, property checks where practical, and crash/recovery scenarios.

## Troubleshooting

If a version is visible only because of RAM state or benchmark-derived timing, reject the decision and require durable transaction evidence.

## References

- `src/lib.rs`
- Transaction status owner: `crates/andromeda-transaction/`
- Storage integration owner: `crates/andromeda-storage/`
