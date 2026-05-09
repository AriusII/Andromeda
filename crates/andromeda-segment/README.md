# andromeda-segment

## Purpose

`andromeda-segment` is the C5 boundary crate for durable storage segments and segment indexes.

This crate now owns segment identity, lifecycle state, mutation class, and implementation-neutral durability boundary validation. Storage keeps page-backed descriptors and segment-index codecs during migration.

## Scope

This crate owns:

- `SegmentId`, `SegmentState`, `SegmentMutation`, and `SegmentDurabilityBoundary`.

Future work in this crate may own:

- Segment identifiers, descriptors, indexes, generation metadata, and sealing rules.
- Immutable segment publication evidence and compaction or merge inputs.
- Explicit segment metadata codecs with version fields, checksums, and compatibility gates.
- Typed errors for invalid segment state, broken indexes, unsupported versions, and unsafe publication.

## Non-goals

- No application-facing ad hoc SQL.
- No bypass of typed Procedure contracts.
- No transaction commit publication, physical WAL file ownership, heap row semantics, buffer-pool eviction, backup execution, restore execution, or HA/DR quorum policy.
- No GPU output, benchmark output, RAM state, or temporary storage as segment truth.
- No segment byte-codec promotion without explicit layout, checksum, and crash/recovery evidence.

## Prerequisites

Before behavior lands here:

- Segment publication must be tied to durable WAL, manifest, and recovery evidence.
- Segment metadata bytes must use explicit codecs, not Rust native struct layout.
- Segment compaction output must be advisory until durable publication evidence exists.
- Mission-critical behavior must include deterministic crash/recovery validation.

## Procedure

1. Define the segment responsibility being split.
2. Keep `src/lib.rs` limited to module declarations and intentional reexports.
3. Specify segment metadata bytes and publication evidence before implementation.
4. Preserve manifest and WAL coverage checks for published segments.
5. Add codec, segment-index, corruption-rejection, and crash/recovery tests before moving behavior.

## Validation

Run `cargo check -p andromeda-segment`. Future descriptor and codec behavior requires `cargo fmt`, `cargo clippy`, segment metadata tests, segment-index tests, corruption-rejection tests, and crash/recovery scenarios.

## Troubleshooting

If a segment is treated as durable without manifest and WAL-backed publication evidence, reject the segment from recovery.

## References

- `src/lib.rs`
- Existing owner: `crates/andromeda-storage/`
