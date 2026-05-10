# andromeda-restore

## Purpose

`andromeda-restore` is the C5 owner crate for restore orchestration, point-in-time recovery inputs, and restore validation evidence.

This crate owns restore plans, artifact preflight, WAL archive replay ranges, PITR target validation, ForensicStart stage policy, and restore evidence contracts. It is no longer a documentation-only scaffold.

Current status: partially implemented restore planning and preflight boundary. This crate does not claim production readiness, restore success, or operational completeness.

## Scope

This crate owns or is expected to own:

- Restore plan descriptors, selected backup artifacts, WAL archive ranges, and PITR target validation.
- Restore execution evidence and recovery validation reports.
- Explicit restore metadata codecs with version fields, hashes, and compatibility gates.
- Typed errors for missing artifacts, incomplete WAL coverage, unsafe PITR targets, and failed validation.

## Non-goals

- No application-facing ad hoc SQL.
- No bypass of typed Procedure contracts.
- No backup creation, physical WAL byte ownership, transaction commit publication, page format ownership, or HA/DR quorum policy.
- No GPU output, benchmark output, RAM state, or temporary storage as restore truth.
- No claim that restore preflight alone proves restore success.

## Prerequisites

Before production restore readiness can be claimed:

- Restore must validate durable backup artifacts and WAL archive coverage before claiming success.
- Restored visible commits must be reconstructed only from durable commit evidence.
- Restore metadata bytes must use explicit codecs, not Rust native struct layout.
- Mission-critical behavior must include deterministic restore and crash/recovery validation.

## Procedure

1. Define the restore responsibility being split.
2. Keep `src/lib.rs` focused on restore ownership and intentional reexports.
3. Specify restore metadata bytes, PITR target validation, and report evidence before implementation.
4. Fail closed on missing or ambiguous artifact coverage.
5. Add restore, PITR, corruption-rejection, and recovery-validation tests before moving behavior.

## Validation

Local restore contracts require `cargo fmt`, `cargo check`, `cargo clippy`, restore orchestration tests, PITR range tests, artifact validation tests, and crash/recovery scenarios. Production restore readiness additionally requires retained WAL replay, `RecoveryReport`, and ReadOnly/ForensicOnly open-mode evidence.

## Troubleshooting

If restore success depends on RAM state, temporary files, GPU output, or benchmark output, reject the report and require durable artifact evidence.

## References

- `src/lib.rs`
- Related durable owners: `crates/andromeda-storage/`, `crates/andromeda-wal/`, `crates/andromeda-backup/`
