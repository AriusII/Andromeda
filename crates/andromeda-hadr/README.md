# andromeda-hadr

## Purpose

`andromeda-hadr` is the C5 owner crate for high availability and disaster recovery evidence.

This crate owns single-primary quorum, fencing, promotion decision, membership, WAL shipping contract, and cluster evidence models. It is no longer a documentation-only scaffold.

Current status: partially implemented control-plane and local persistence boundary. This crate does not claim production readiness, failover safety, or operational completeness.

## Scope

This crate owns or is expected to own:

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
- No claim that a pure promotion decision alone proves operational failover safety.

## Prerequisites

Before production HA/DR readiness can be claimed:

- Promotion and failover decisions must be backed by quorum, fencing, epoch, and durable WAL shipping evidence.
- HA/DR metadata bytes must use explicit codecs, not Rust native struct layout.
- Replicas must not publish visible commits that lack durable commit evidence.
- Mission-critical behavior must include deterministic failover and crash/recovery validation.

## Procedure

1. Define the HA/DR responsibility being split.
2. Keep `src/lib.rs` focused on HA/DR ownership and intentional reexports.
3. Specify quorum, fencing, epoch, and WAL shipping evidence before implementation.
4. Keep HA/DR controls off the Application Surface.
5. Add failover, fencing, WAL shipping, and crash/recovery tests before moving behavior.

## Validation

Local HA/DR contracts require `cargo fmt`, `cargo check`, `cargo clippy`, HA/DR quorum tests, fencing tests, WAL shipping tests, failover tests, and crash/recovery scenarios. Production HA/DR readiness additionally requires retained cluster drill evidence for primary suspect, quorum, fencing, recovery, promotion, manifest update, replica repointing, and old-primary rejection.

## Troubleshooting

If a node can promote without quorum, fencing, epoch, and durable WAL evidence, reject the promotion path as unsafe.

## References

- `src/lib.rs`
- Related durable owners: `crates/andromeda-storage/`, `crates/andromeda-wal/`, `crates/andromeda-restore/`
