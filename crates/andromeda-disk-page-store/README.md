# andromeda-disk-page-store

## Purpose

`andromeda-disk-page-store` is the C5 boundary crate for durable page I/O against disk-backed storage.

This crate now owns the implementation-neutral page flush durability boundary. The file-backed page store and disk manager remain in `andromeda-storage` during migration.

## Scope

This crate owns:

- WAL-before-page-flush validation for page LSN and durable WAL LSN evidence.

Future work in this crate may own:

- Disk page identifiers, file layout policy, read and write operations, and sync boundaries.
- Durability evidence for page writes and error classification for short reads, short writes, and sync failures.
- Coordination with page codecs, buffer-pool flush policy, manifests, and recovery.
- Typed errors for I/O, format, durability, and allocation failures.

## Non-goals

- No application-facing ad hoc SQL.
- No bypass of typed Procedure contracts.
- No ownership of transaction status, WAL bytes, page format semantics, heap layout, buffer-pool eviction, backup, restore, or HA/DR orchestration.
- No GPU output, benchmark output, RAM state, or temporary storage as disk-page-store truth.
- No durable page byte-format semantics; page bytes must come from explicit format codecs.

## Prerequisites

Before behavior lands here:

- Disk writes that make page state durable must be coordinated with WAL-before-page-flush policy.
- Page bytes must be produced and consumed through explicit codecs owned by the relevant format crate.
- Recovery must be able to classify partial, missing, or corrupt durable page evidence.
- Mission-critical behavior must include deterministic crash/recovery validation.

## Procedure

1. Define the disk page-store responsibility being split.
2. Keep `src/lib.rs` limited to module declarations and intentional reexports.
3. Keep byte-format decisions outside raw I/O where possible.
4. Report sync and short I/O failures with typed errors.
5. Add I/O contract, corruption, and crash/recovery tests before moving behavior.

## Validation

Run `cargo check -p andromeda-disk-page-store`. Future I/O behavior requires `cargo fmt`, `cargo clippy`, disk page-store contract tests, fault-injection tests, and crash/recovery scenarios.

## Troubleshooting

If durable page state cannot be distinguished from a short write or unsynced write, reject the evidence and require recovery classification.

## References

- `src/lib.rs`
- Existing owner: `crates/andromeda-storage/`
