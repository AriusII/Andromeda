# andromeda-hadr

## Purpose

`andromeda-hadr` is a future C5 owner crate for high availability and disaster recovery evidence.

This scaffold reserves a boundary for single-primary replication, quorum evidence, fencing, promotion epochs, WAL shipping state, and cluster recovery reports. No behavior has moved from `andromeda-storage`.

Current status: ownership-boundary scaffold only. This crate does not claim production readiness, failover safety, or operational completeness.

## Scope

Future work in this crate may own:

- Replication role descriptors, primary epochs, quorum evidence, fencing state, and promotion records.
- WAL shipping coverage, replica catch-up evidence, and failover validation reports.
- Explicit HA/DR metadata codecs with version fields, hashes, and compatibility gates.
- Typed errors for split-brain risk, missing quorum, stale epochs, unsafe promotion, and incomplete WAL shipping.

## Non-goals

- No application-facing ad hoc SQL.
- No bypass of typed Procedure contracts.
- No Administration or HA/DR capability exposure through the Application Surface.
- No physical WAL byte ownership, page format ownership, backup creation, restore execution, or transaction visibility authority.
- No GPU output, benchmark output, RAM state, or temporary storage as HA/DR truth.
- No behavior move in this scaffold.

## Prerequisites

Before behavior lands here:

- Promotion and failover decisions must be backed by quorum, fencing, epoch, and durable WAL shipping evidence.
- HA/DR metadata bytes must use explicit codecs, not Rust native struct layout.
- Replicas must not publish visible commits that lack durable commit evidence.
- Mission-critical behavior must include deterministic failover and crash/recovery validation.

## Procedure

1. Define the HA/DR responsibility being split.
2. Keep `src/lib.rs` limited to module declarations and intentional reexports.
3. Specify quorum, fencing, epoch, and WAL shipping evidence before implementation.
4. Keep HA/DR controls off the Application Surface.
5. Add failover, fencing, WAL shipping, and crash/recovery tests before moving behavior.

## Validation

This scaffold is documentation-only. Future behavior requires `cargo fmt`, `cargo check`, `cargo clippy`, HA/DR quorum tests, fencing tests, WAL shipping tests, failover tests, and crash/recovery scenarios.

## Troubleshooting

If a node can promote without quorum, fencing, epoch, and durable WAL evidence, reject the promotion path as unsafe.

## References

- `src/lib.rs`
- Existing owner: `crates/andromeda-storage/`
