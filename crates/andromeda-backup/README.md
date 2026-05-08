# andromeda-backup

## Purpose

`andromeda-backup` is a future C5 owner crate for backup planning, backup artifacts, retention evidence, and backup validation.

This scaffold reserves a boundary for backup manifests, snapshot checkpoints, WAL archive requirements, artifact verification, and immutable retention policy. No behavior has moved from `andromeda-storage`.

Current status: ownership-boundary scaffold only. This crate does not claim production readiness, backup recoverability, or operational completeness.

## Scope

Future work in this crate may own:

- Backup plan descriptors, checkpoint evidence, artifact inventories, and validation reports.
- WAL archive coverage requirements for consistent backups and point-in-time recovery inputs.
- Explicit backup metadata codecs with version fields, hashes, and compatibility gates.
- Typed errors for incomplete artifacts, missing WAL coverage, invalid retention, and unsafe backup publication.

## Non-goals

- No application-facing ad hoc SQL.
- No bypass of typed Procedure contracts.
- No restore execution, recovery replay, transaction status authority, page format ownership, physical WAL byte ownership, or HA/DR quorum policy.
- No GPU output, benchmark output, RAM state, or temporary storage as backup truth.
- No behavior move in this scaffold.

## Prerequisites

Before behavior lands here:

- Backup consistency must be tied to durable checkpoint and WAL archive evidence.
- Backup metadata bytes must use explicit codecs, not Rust native struct layout.
- A backup must not claim recoverability until restore validation or equivalent recovery evidence exists.
- Mission-critical behavior must include deterministic backup and recovery validation.

## Procedure

1. Define the backup responsibility being split.
2. Keep `src/lib.rs` limited to module declarations and intentional reexports.
3. Specify backup metadata bytes, hashes, retention state, and WAL archive coverage before implementation.
4. Keep backup evidence separate from restore execution.
5. Add artifact, retention, PITR-input, and recovery-validation tests before moving behavior.

## Validation

This scaffold is documentation-only. Future behavior requires `cargo fmt`, `cargo check`, `cargo clippy`, backup artifact tests, retention tests, WAL archive coverage tests, and restore validation scenarios.

## Troubleshooting

If a backup is marked recoverable without durable checkpoint and WAL archive evidence, reject the backup report.

## References

- `src/lib.rs`
- Existing owner: `crates/andromeda-storage/`
