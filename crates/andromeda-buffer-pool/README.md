# andromeda-buffer-pool

## Purpose

`andromeda-buffer-pool` is a future C5 owner crate for page residency, pinning, dirtiness, eviction, and flush coordination.

This scaffold reserves a boundary for buffer frames, page guards, dirty tracking, eviction policy, and WAL durability fences before flush. No behavior has moved from `andromeda-storage`.

## Scope

Future work in this crate may own:

- Buffer frame identifiers, pinning, guards, and frame lifecycle.
- Dirty-page tracking, flush candidates, eviction policy, and flush scheduling.
- WAL durability observer integration that blocks unsafe page flushes.
- Typed errors for pinned-frame eviction, dirty flush failure, insufficient WAL durability, and invalid frame state.

## Non-goals

- No application-facing ad hoc SQL.
- No bypass of typed Procedure contracts.
- No ownership of durable page byte formats, heap semantics, physical disk I/O, manifest publication, recovery replay, backup, restore, or HA/DR.
- No transaction commit publication or physical WAL byte ownership.
- No GPU output, benchmark output, RAM state, or temporary storage as buffer-pool truth.
- No behavior move in this scaffold.

## Prerequisites

Before behavior lands here:

- Dirty page flush must prove durable WAL coverage through the page LSN.
- In-memory residency must not be treated as durable truth.
- Persistent and network bytes must use explicit codecs where this crate participates in durable formats.
- Mission-critical flush behavior must include deterministic crash/recovery validation.

## Procedure

1. Define the buffer-pool responsibility being split.
2. Keep `src/lib.rs` limited to module declarations and intentional reexports.
3. Keep residency and eviction policy separate from durable page format ownership.
4. Enforce WAL-before-page-flush checks before any flush success is reported.
5. Add pinning, eviction, dirty flush, and crash/recovery tests before moving behavior.

## Validation

This scaffold is documentation-only. Future behavior requires `cargo fmt`, `cargo check`, `cargo clippy`, buffer-pool lifecycle tests, WAL durability fence tests, eviction tests, and crash/recovery scenarios.

## Troubleshooting

If a flush succeeds without proving WAL durability through the page LSN, fail closed and keep the page dirty.

## References

- `src/lib.rs`
- Existing owner: `crates/andromeda-storage/`
