# andromeda-storage-page

## Purpose

`andromeda-storage-page` is the C5 owner crate for durable page format boundaries.

This crate owns page identifiers, page headers, trailers, payload bounds, checksums, page LSN policy, page images, and deterministic page-store contracts extracted from `andromeda-storage`.

## Scope

Future work in this crate may own:

- Page format constants, version fields, headers, trailers, and checksum coverage.
- Page LSN validation and WAL-before-page-flush requirements.
- Explicit page encoders and decoders with little-endian canonical serialization.
- Typed errors for corrupt bytes, unsupported versions, invalid bounds, and unsafe page durability.

## Non-goals

- No application-facing ad hoc SQL.
- No bypass of typed Procedure contracts.
- No heap, B+Tree, buffer pool, manifest, recovery, backup, restore, or HA/DR orchestration.
- No transaction commit publication or physical WAL file ownership.
- No GPU output, benchmark output, RAM state, or temporary storage as page truth.
- No disk page I/O move in this extraction step.

## Prerequisites

Before behavior lands here:

- Page bytes must be encoded and decoded through explicit codecs, never Rust native struct layout.
- Page flush must prove durable WAL coverage through the page LSN.
- Format changes must include versioning, offset documentation, golden vectors, and corruption rejection.
- Mission-critical behavior must include deterministic crash/recovery validation.

## Procedure

1. Define the page-format responsibility being split.
2. Keep `src/lib.rs` limited to module declarations and intentional reexports.
3. Specify byte offsets, endian rules, checksums, and compatibility behavior before implementation.
4. Preserve WAL-before-page-flush ordering.
5. Add codec, property, corruption-rejection, and crash/recovery tests before moving behavior.

## Validation

This extraction includes type and in-memory contract ownership. Future durable codec or disk behavior requires `cargo fmt`, `cargo check`, `cargo clippy`, page codec roundtrip tests, golden tests, property tests, fuzz or corruption-rejection coverage, and crash/recovery scenarios.

## Troubleshooting

If a decoder accepts ambiguous bytes or native layout assumptions, reject the format and require an explicit codec specification.

## References

- `src/lib.rs`
- Existing owner: `crates/andromeda-storage/`
