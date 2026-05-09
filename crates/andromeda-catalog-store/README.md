# Andromeda Catalog Store

## Purpose

`andromeda-catalog-store` is a runtime-free scaffold for future catalog store
ownership. It reserves stable taxonomy placeholders for catalog storage
responsibilities, snapshot access, and publication safety barriers.

## Scope

- Catalog store responsibility taxonomy identifiers.
- Snapshot and lookup class placeholders.
- Publication safety barrier names.
- Compile-ready Rust 2024 crate metadata.

## Non-goals

- No catalog publication without durable WAL.
- No ad hoc SQL or dynamic application-facing query surface.
- No storage engine, buffer pool, WAL replay, or recovery implementation.
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
store APIs after WAL, recovery, audit, and catalog publication responsibilities
are assigned.

## Validation

```powershell
cargo fmt --manifest-path crates/andromeda-catalog-store/Cargo.toml --check
cargo check --manifest-path crates/andromeda-catalog-store/Cargo.toml
cargo test --manifest-path crates/andromeda-catalog-store/Cargo.toml
```

## Troubleshooting

If Cargo reports that the crate is not a root workspace member, verify that this
manifest still contains its local `[workspace]` table. Do not add the crate to
the repository root workspace until that ownership change is explicitly
requested.

## References

- Root `AGENTS.md` Andromeda invariants.
- `crates/AGENTS.md` Rust crate instructions.
