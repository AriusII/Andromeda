# Andromeda Catalog Diff

## Purpose

`andromeda-catalog-diff` is a runtime-free scaffold for future catalog diff
ownership. It reserves stable taxonomy placeholders for catalog object changes,
contract impact categories, and review severity names.

## Scope

- Catalog diff kind taxonomy identifiers.
- Contract impact placeholders.
- Review severity names.
- Compile-ready Rust 2024 crate metadata.

## Non-goals

- No catalog publication without durable WAL.
- No ad hoc SQL or dynamic application-facing query surface.
- No diff engine, compatibility evaluator, apply engine, or rollback engine.
- No persistence, network serialization, or Rust native struct layout contract.
- No runtime dependency ownership.
- No release claim.

## Prerequisites

- Rust toolchain compatible with the repository baseline.
- Cargo invoked directly against this manifest until the root workspace chooses
  an owner for the crate.

## Procedure

Use the exported taxonomy entries as stable names only. A future owner can move
the crate into the root workspace and replace placeholders with typed catalog
diff logic after contract compatibility, catalog snapshots, WAL, and audit
responsibilities are assigned.

## Validation

```powershell
cargo fmt --manifest-path crates/andromeda-catalog-diff/Cargo.toml --check
cargo check --manifest-path crates/andromeda-catalog-diff/Cargo.toml
cargo test --manifest-path crates/andromeda-catalog-diff/Cargo.toml
```

## Troubleshooting

If Cargo reports that the crate is not a root workspace member, verify that this
manifest still contains its local `[workspace]` table. Do not add the crate to
the repository root workspace until that ownership change is explicitly
requested.

## References

- Root `AGENTS.md` Andromeda invariants.
- `crates/AGENTS.md` Rust crate instructions.
