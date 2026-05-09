# andromeda-manifest

## Purpose

`andromeda-manifest` is the C5 boundary crate for manifest publication, root pointers, checkpoint evidence, and storage topology descriptors.

This crate now owns the implementation-neutral manifest durability boundary used by `andromeda-storage`. Manifest byte codecs, root switching, and publication execution remain in storage during migration.

## Scope

This crate owns:

- Manifest recovery-root fields and validation for identity, checkpoint LSN, recovery floor, and manifest CRC.

Future work in this crate may own:

- Manifest identifiers, root pointers, checkpoint LSNs, topology descriptors, and publication records.
- Atomic manifest switch validation and recovery-floor evidence.
- Explicit manifest encoders and decoders with version fields and checksums or hashes.
- Typed errors for invalid manifests, unsupported versions, unsafe switches, and recovery-floor violations.

## Non-goals

- No application-facing ad hoc SQL.
- No bypass of typed Procedure contracts.
- No transaction commit publication, physical WAL file ownership, buffer-pool residency, heap storage, backup execution, restore execution, or HA/DR quorum policy.
- No GPU output, benchmark output, RAM state, or temporary storage as manifest truth.
- No manifest byte-codec promotion without explicit layout, checksum, and crash/recovery evidence.

## Prerequisites

Before behavior lands here:

- Manifest publication must prove checkpoint and recovery-floor coverage against durable WAL evidence.
- Manifest bytes must use explicit versioned codecs, not Rust native struct layout.
- Recovery must reject manifests that cannot be tied to durable WAL and durable artifacts.
- Mission-critical behavior must include deterministic crash/recovery validation.

## Procedure

1. Define the manifest responsibility being split.
2. Keep `src/lib.rs` limited to module declarations and intentional reexports.
3. Specify manifest byte fields, offsets, versions, hashes, and compatibility gates before implementation.
4. Preserve WAL coverage checks for manifest switches.
5. Add codec, switch, corruption-rejection, and crash/recovery tests before moving behavior.

## Validation

Run `cargo check -p andromeda-manifest`. Future codec and publication behavior requires `cargo fmt`, `cargo clippy`, manifest codec tests, atomic switch tests, recovery-floor tests, and crash/recovery scenarios.

## Troubleshooting

If a manifest can be published without durable WAL evidence for its checkpoint and recovery floor, reject the switch.

## References

- `src/lib.rs`
- Existing owner: `crates/andromeda-storage/`
