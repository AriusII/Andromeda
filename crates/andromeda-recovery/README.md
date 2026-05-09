# andromeda-recovery

## Purpose

`andromeda-recovery` is the C5 boundary crate for startup recovery, replay planning, recovery reports, and forensic evidence.

This crate now owns the startup mode boundary used by `andromeda-storage`. Replay planning, durable artifact discovery, and report construction remain in storage, transaction, and WAL crates during migration.

## Scope

This crate owns:

- `StartupMode` and its clean-replay and forensic-report boundary helpers.

Future work in this crate may own:

- Recovery startup modes, replay plans, durable artifact inventories, and recovery reports.
- WAL prefix selection, manifest validation, page and segment evidence checks, and transaction terminal-state reconstruction.
- Forensic traces explaining accepted, rejected, skipped, or quarantined durable evidence.
- Typed errors for corrupt artifacts, unsafe replay, missing manifests, broken WAL chains, and incomplete recovery evidence.

## Non-goals

- No application-facing ad hoc SQL.
- No bypass of typed Procedure contracts.
- No physical WAL byte ownership unless a later explicit split assigns it.
- No page format, heap format, buffer-pool, backup creation, restore orchestration, or HA/DR quorum ownership.
- No GPU output, benchmark output, RAM state, or temporary storage as recovery truth.
- No replay behavior that treats RAM, temp storage, GPU output, or benchmark output as recovery truth.

## Prerequisites

Before behavior lands here:

- Recovery must derive truth from durable WAL and durable artifacts, not RAM summaries.
- A visible commit after restart must be reconstructed only from durable commit evidence.
- Persistent recovery metadata must use explicit codecs, not Rust native struct layout.
- Mission-critical behavior must include deterministic crash/recovery validation.

## Procedure

1. Define the recovery responsibility being split.
2. Keep `src/lib.rs` limited to module declarations and intentional reexports.
3. Make every accepted replay decision traceable to durable evidence.
4. Reject ambiguous, corrupt, or non-durable artifacts by default.
5. Add crash/recovery, corruption, replay, and forensic-report tests before moving behavior.

## Validation

Run `cargo check -p andromeda-recovery`. Future replay and report behavior requires `cargo fmt`, `cargo clippy`, recovery replay tests, recovery report tests, corruption-rejection tests, and deterministic crash/recovery scenarios.

## Troubleshooting

If recovery needs RAM state or benchmark output to decide truth, reject the startup decision and require durable evidence.

## References

- `src/lib.rs`
- Existing owners: `crates/andromeda-storage/`, `crates/andromeda-transaction/`, `crates/andromeda-transaction-log/`, `crates/andromeda-wal/`
