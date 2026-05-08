# andromeda-forensic

## Purpose

`andromeda-forensic` is a future C5 owner crate for forensic startup and incident evidence.

This scaffold reserves a boundary for forensic startup reports, recovery evidence review, incident capture descriptors, and tamper-evident investigation inputs. No behavior has moved from `andromeda-storage`, `andromeda-recovery`, or operations documentation.

Current status: ownership-boundary scaffold only. This crate does not claim production readiness, forensic completeness, or operational authority.

## Scope

Future work in this crate may own:

- Forensic startup descriptors, recovery evidence summaries, incident capture manifests, and review reports.
- Evidence correlation across WAL, manifest, backup artifact, audit, and restore validation records.
- Explicit forensic metadata codecs with version fields, hashes, and compatibility gates.
- Typed errors for missing evidence, ambiguous recovery state, unsafe repair, and tamper suspicion.

## Non-goals

- No application-facing ad hoc SQL.
- No bypass of typed Procedure contracts.
- No restore execution, backup creation, HA/DR promotion, destructive repair, or transaction visibility authority.
- No GPU output, benchmark output, RAM state, or temporary storage as forensic truth.
- No behavior move in this scaffold.

## Prerequisites

Before behavior lands here:

- Forensic startup must preserve durable evidence before any destructive repair.
- Recovery conclusions must be backed by durable WAL, manifest, backup, and audit evidence.
- Forensic metadata bytes must use explicit codecs, not Rust native struct layout.
- Mission-critical behavior must include deterministic recovery and corruption-handling validation.

## Procedure

1. Define the forensic responsibility being split.
2. Keep `src/lib.rs` limited to module declarations and intentional reexports.
3. Specify evidence inputs, preservation rules, hashes, and review reports before implementation.
4. Keep forensic review separate from restore execution and destructive repair.
5. Add recovery, corruption, evidence-gap, and tamper-rejection tests before moving behavior.

## Validation

This scaffold is documentation-only. Future behavior requires `cargo fmt`, `cargo check`, `cargo clippy`, forensic evidence tests, corruption-rejection tests, and crash/recovery scenarios.

## Troubleshooting

If forensic conclusions depend on RAM state, temporary files, GPU output, or benchmark output, reject the report and require durable evidence.

## References

- `src/lib.rs`
- Existing implementation sources: `crates/andromeda-storage/`, `crates/andromeda-recovery/`, and `documentations/operations/runbooks/`
