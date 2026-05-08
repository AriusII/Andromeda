# andromeda-storage-heap

## Purpose

`andromeda-storage-heap` is a future C5 owner crate for heap storage layout and heap-row access policy.

This scaffold reserves a boundary for heap page interpretation, slot directory policy, row placement, heap scans, and heap recovery evidence. No behavior has moved from `andromeda-storage`.

## Scope

Future work in this crate may own:

- Heap page layout interpretation over explicit page bytes.
- Slot directory rules, row insertion metadata, scan cursors, and vacuum planning.
- Heap redo and recovery evidence that depends on durable WAL and page LSNs.
- Typed errors for malformed heap pages, insufficient space, invalid slots, and unsafe recovery state.

## Non-goals

- No application-facing ad hoc SQL.
- No bypass of typed Procedure contracts.
- No ownership of generic page codecs, buffer-pool residency, disk page I/O, manifests, backup, restore, or HA/DR orchestration.
- No transaction commit publication or physical WAL file ownership.
- No GPU output, benchmark output, RAM state, or temporary storage as heap truth.
- No behavior move in this scaffold.

## Prerequisites

Before behavior lands here:

- Heap changes must respect WAL-before-visible-commit and WAL-before-page-flush ordering.
- Durable heap bytes must be interpreted through explicit codecs, not Rust native struct layout.
- Recovery must reconstruct heap state from durable WAL and durable pages only.
- Mission-critical behavior must include deterministic crash/recovery validation.

## Procedure

1. Define the heap responsibility being split.
2. Keep `src/lib.rs` limited to module declarations and intentional reexports.
3. Separate heap layout from generic page byte ownership.
4. Make redo, vacuum, and scan behavior explainable from durable evidence.
5. Add heap layout, recovery, and corruption-rejection tests before moving behavior.

## Validation

This scaffold is documentation-only. Future behavior requires `cargo fmt`, `cargo check`, `cargo clippy`, heap layout tests, scan and vacuum tests, codec tests through the page owner, and crash/recovery scenarios.

## Troubleshooting

If heap state can be reconstructed only from RAM or benchmark output, reject the path and require durable WAL plus page evidence.

## References

- `src/lib.rs`
- Existing owner: `crates/andromeda-storage/`
