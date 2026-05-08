# andromeda-runbooks

## Purpose

`andromeda-runbooks` is a future C5 owner crate for operational runbook descriptors.

This scaffold reserves a boundary for typed runbook descriptors covering backup, restore, HA/DR, forensic startup, and incident response procedures. No executable automation or production control-plane behavior has moved into this crate.

Current status: ownership-boundary scaffold only. This crate does not claim production readiness, executable automation, or operational completeness.

## Scope

Future work in this crate may own:

- Runbook descriptors for backup validation, restore/PITR drills, HA/DR failover, forensic startup, and incident response.
- Preconditions, operator steps, evidence requirements, rollback limits, validation checks, and escalation fields.
- Explicit runbook metadata codecs with version fields, hashes, and compatibility gates.
- Typed errors for missing evidence, unsafe preconditions, incomplete validation, and forbidden surface exposure.

## Non-goals

- No application-facing ad hoc SQL.
- No bypass of typed Procedure contracts.
- No executable orchestration, scheduler integration, destructive repair, or control-plane behavior.
- No Administration or HA/DR capability exposure through the Application Surface.
- No production procedure claim in this scaffold.

## Prerequisites

Before behavior lands here:

- Runbook descriptors must define preconditions, evidence inputs, operator actions, validation, and rollback limits.
- Operational controls must remain on administrative surfaces only.
- Runbook metadata bytes must use explicit codecs, not Rust native struct layout.
- Mission-critical behavior must include deterministic drill, failover, restore, and crash/recovery validation.

## Procedure

1. Define the runbook responsibility being split.
2. Keep `src/lib.rs` limited to module declarations and intentional reexports.
3. Specify preconditions, evidence inputs, operator actions, and validation before implementation.
4. Keep runbook descriptors separate from executable automation.
5. Add drill, validation, forbidden-surface, and failure-path tests before moving behavior.

## Validation

This scaffold is documentation-only. Future behavior requires `cargo fmt`, `cargo check`, `cargo clippy`, runbook descriptor tests, forbidden-surface tests, drill validation tests, and crash/recovery scenarios.

## Troubleshooting

If a runbook can trigger Administration or HA/DR behavior through the Application Surface, reject the design and move it back to the administrative control boundary.

## References

- `src/lib.rs`
- Existing source domains: `crates/andromeda-storage/`, `crates/andromeda-backup/`, `crates/andromeda-restore/`, `crates/andromeda-hadr/`, and `crates/andromeda-forensic/`
