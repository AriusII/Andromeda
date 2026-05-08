# Andromeda Definition Batch

## Purpose

`andromeda-definition-batch` is a runtime-free scaffold for future
DefinitionBatch ownership. It reserves stable taxonomy placeholders for
definition operations, validation phases, dry-run reporting, and WAL-aware apply
barriers.

## Scope

- Definition operation taxonomy identifiers.
- Validation and dry-run phase placeholders.
- Conflict and apply barrier names.
- Compile-ready Rust 2024 crate metadata.

## Non-goals

- No catalog publication without durable WAL.
- No ad hoc SQL or dynamic application-facing query surface.
- No DefinitionBatch parser, binder, executor, or rollback engine.
- No persistence, network serialization, or Rust native struct layout contract.
- No runtime dependency ownership.
- No release claim.

## Prerequisites

- Rust toolchain compatible with the repository baseline.
- Cargo invoked directly against this manifest until the root workspace chooses
  an owner for the crate.

## Procedure

Use the exported taxonomy entries as stable names only. A future owner can move
the crate into the root workspace and replace placeholders with typed
DefinitionBatch operations after catalog, contract, WAL, recovery, and audit
responsibilities are assigned.

## Validation

```powershell
cargo fmt --manifest-path crates/andromeda-definition-batch/Cargo.toml --check
cargo check --manifest-path crates/andromeda-definition-batch/Cargo.toml
cargo test --manifest-path crates/andromeda-definition-batch/Cargo.toml
```

## Troubleshooting

If Cargo reports that the crate is not a root workspace member, verify that this
manifest still contains its local `[workspace]` table. Do not add the crate to
the repository root workspace until that ownership change is explicitly
requested.

## References

- Root `AGENTS.md` Andromeda invariants.
- `crates/AGENTS.md` Rust crate instructions.
