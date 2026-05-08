# Andromeda Contract Compat

## Purpose

`andromeda-contract-compat` is a runtime-free scaffold for future Procedure
contract compatibility ownership. It defines stable taxonomy placeholders that
other design work can reference before the final compatibility engine is
implemented.

## Scope

- Procedure contract compatibility taxonomy identifiers.
- Compatibility outcome placeholders.
- Review and incompatibility category names.
- Compile-ready Rust 2024 crate metadata.

## Non-goals

- No catalog publication without durable WAL.
- No ad hoc SQL or dynamic application-facing query surface.
- No compatibility decision engine.
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
compatibility rules after the catalog, contract, WAL, and audit responsibilities
are assigned.

## Validation

```powershell
cargo fmt --manifest-path crates/andromeda-contract-compat/Cargo.toml --check
cargo check --manifest-path crates/andromeda-contract-compat/Cargo.toml
cargo test --manifest-path crates/andromeda-contract-compat/Cargo.toml
```

## Troubleshooting

If Cargo reports that the crate is not a root workspace member, verify that this
manifest still contains its local `[workspace]` table. Do not add the crate to
the repository root workspace until that ownership change is explicitly
requested.

## References

- Root `AGENTS.md` Andromeda invariants.
- `crates/AGENTS.md` Rust crate instructions.
