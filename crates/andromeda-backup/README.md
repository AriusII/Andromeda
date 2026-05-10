# andromeda-backup

## Purpose

`andromeda-backup` is the C5 owner crate for backup planning, backup artifacts, retention evidence, and backup validation.

This crate owns backup manifest, checkpoint, WAL archive coverage, file-backed artifact, immutability, retention, and validation contracts. It is no longer a documentation-only scaffold.

Current status: partially implemented contract and local artifact boundary. This crate does not claim production readiness, backup recoverability, or operational completeness.

## Scope

This crate owns or is expected to own:

- Backup plan descriptors, checkpoint evidence, artifact inventories, and validation reports.
- WAL archive coverage requirements for consistent backups and point-in-time recovery inputs.
- Explicit backup metadata codecs with version fields, hashes, and compatibility gates.
- Typed errors for incomplete artifacts, missing WAL coverage, invalid retention, and unsafe backup publication.

## Non-goals

- No application-facing ad hoc SQL.
- No bypass of typed Procedure contracts.
- No restore execution, recovery replay, transaction status authority, page format ownership, physical WAL byte ownership, or HA/DR quorum policy.
- No GPU output, benchmark output, RAM state, or temporary storage as backup truth.
- No claim that local file-backed or fixture artifacts prove production backup recoverability.

## Prerequisites

Before production backup readiness can be claimed:

- Backup consistency must be tied to durable checkpoint and WAL archive evidence.
- Backup metadata bytes must use explicit codecs, not Rust native struct layout.
- A backup must not claim recoverability until restore validation or equivalent recovery evidence exists.
- Mission-critical behavior must include deterministic backup and recovery validation.

## Procedure

1. Define the backup responsibility being split.
2. Keep `src/lib.rs` focused on backup ownership and intentional reexports.
3. Specify backup metadata bytes, hashes, retention state, and WAL archive coverage before implementation.
4. Keep backup evidence separate from restore execution.
5. Add artifact, retention, PITR-input, and recovery-validation tests before moving behavior.

## Validation

Local backup contracts require `cargo fmt`, `cargo check`, `cargo clippy`, backup artifact tests, retention tests, WAL archive coverage tests, and restore validation scenarios. Production backup readiness additionally requires retained restore/PITR drill evidence.

## Troubleshooting

If a backup is marked recoverable without durable checkpoint and WAL archive evidence, reject the backup report.

## References

- `src/lib.rs`
- Related durable owners: `crates/andromeda-storage/`, `crates/andromeda-wal/`, `crates/andromeda-restore/`
