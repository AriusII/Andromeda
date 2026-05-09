# Andromeda Definition Batch

## Purpose

`andromeda-definition-batch` owns runtime-free DefinitionBatch identity,
DefinitionBatch import correlation, and source-hash primitives. It also
reserves stable taxonomy placeholders for definition operations, validation
phases, dry-run reporting, and WAL-aware apply barriers.

## Scope

- `DefinitionBatchId`, `DefinitionBatchImportId`, and
  `DefinitionBatchSourceHash` value types.
- Definition operation taxonomy identifiers.
- Validation and dry-run phase placeholders.
- Conflict and apply barrier names.
- Compile-ready Rust 2024 crate metadata.

## Non-goals

- No catalog publication without durable WAL.
- No ad hoc SQL or dynamic application-facing query surface.
- No DefinitionBatch parser, binder, executor, or rollback engine.
- No administration surface for DefinitionBatch import.
- No persistence, network serialization, or Rust native struct layout contract.
- No catalog object, mutation-plan, or publication ownership.
- No release claim.

## Prerequisites

- Rust toolchain compatible with the repository baseline.
- Cargo invoked directly against this manifest until the root workspace chooses
  an owner for the crate.

## Procedure

Use the exported identity/hash primitives for catalog-facing DefinitionBatch
correlation. Use `DefinitionBatchImportId` only as an administrative import
correlation identifier; it does not authorize import or publication. Use
taxonomy entries as stable names only. Typed DefinitionBatch operations remain
catalog-owned until catalog, contract, WAL, recovery, and audit
responsibilities are split explicitly.

## Validation

```powershell
cargo fmt --manifest-path crates/andromeda-definition-batch/Cargo.toml --check
cargo check --manifest-path crates/andromeda-definition-batch/Cargo.toml
cargo test --manifest-path crates/andromeda-definition-batch/Cargo.toml
```

## Troubleshooting

If a downstream crate cannot resolve `andromeda-definition-batch`, verify that
the dependency is declared through the root workspace dependency table and that
the downstream crate depends on `andromeda-definition-batch.workspace = true`.

## References

- Root `AGENTS.md` Andromeda invariants.
- `crates/AGENTS.md` Rust crate instructions.
