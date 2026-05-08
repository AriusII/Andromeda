# andromeda-restore

## Purpose

`andromeda-restore` is a future C5 owner crate for restore orchestration, point-in-time recovery inputs, and restore validation evidence.

This scaffold reserves a boundary for restore plans, artifact verification, WAL archive replay ranges, PITR targets, and restore reports. No behavior has moved from `andromeda-storage`.

## Scope

Future work in this crate may own:

- Restore plan descriptors, selected backup artifacts, WAL archive ranges, and PITR target validation.
- Restore execution evidence and recovery validation reports.
- Explicit restore metadata codecs with version fields, hashes, and compatibility gates.
- Typed errors for missing artifacts, incomplete WAL coverage, unsafe PITR targets, and failed validation.

## Non-goals

- No application-facing ad hoc SQL.
- No bypass of typed Procedure contracts.
- No backup creation, physical WAL byte ownership, transaction commit publication, page format ownership, or HA/DR quorum policy.
- No GPU output, benchmark output, RAM state, or temporary storage as restore truth.
- No behavior move in this scaffold.

## Prerequisites

Before behavior lands here:

- Restore must validate durable backup artifacts and WAL archive coverage before claiming success.
- Restored visible commits must be reconstructed only from durable commit evidence.
- Restore metadata bytes must use explicit codecs, not Rust native struct layout.
- Mission-critical behavior must include deterministic restore and crash/recovery validation.

## Procedure

1. Define the restore responsibility being split.
2. Keep `src/lib.rs` limited to module declarations and intentional reexports.
3. Specify restore metadata bytes, PITR target validation, and report evidence before implementation.
4. Fail closed on missing or ambiguous artifact coverage.
5. Add restore, PITR, corruption-rejection, and recovery-validation tests before moving behavior.

## Validation

This scaffold is documentation-only. Future behavior requires `cargo fmt`, `cargo check`, `cargo clippy`, restore orchestration tests, PITR range tests, artifact validation tests, and crash/recovery scenarios.

## Troubleshooting

If restore success depends on RAM state, temporary files, GPU output, or benchmark output, reject the report and require durable artifact evidence.

## References

- `src/lib.rs`
- Existing owner: `crates/andromeda-storage/`
