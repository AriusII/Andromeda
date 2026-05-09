# Andromeda Catalog Diff

## Purpose

`andromeda-catalog-diff` is the runtime-free owner for catalog object diff
evidence. It classifies object additions, removals, replacements, shape-hash
changes, and review severity without applying catalog state or publishing
anything visible.

## Scope

- Catalog object diff DTOs.
- Runtime-free diff computation for optional catalog object definitions.
- Catalog diff kind taxonomy identifiers.
- Contract impact categories.
- Review severity names.

## Non-goals

- No catalog publication without durable WAL.
- No ad hoc SQL or dynamic application-facing query surface.
- No Procedure compatibility evaluator, apply engine, or rollback engine.
- No persistence, network serialization, or Rust native struct layout contract.
- No runtime dependency ownership.
- No release claim.

## Prerequisites

- Rust toolchain compatible with the repository baseline.
- Cargo invoked directly against this manifest until the root workspace chooses
  an owner for the crate.

## Procedure

Use `diff_catalog_object_definitions` for object-level evidence when callers
need to prove that catalog object shape or identity changed. Keep Procedure
compatibility diagnostics, DefinitionBatch source hashes, durable WAL replay,
and publication gates in their existing owner crates.

## Validation

```powershell
cargo fmt --check -p andromeda-catalog-diff
cargo check -p andromeda-catalog-diff --all-targets --all-features
cargo test -p andromeda-catalog-diff --all-targets --all-features
cargo test -p andromeda-catalog --test catalog_diff_contract -- --nocapture
```

## Troubleshooting

If a caller needs Procedure compatibility diagnostics, use the procedure
contract owner first and keep this crate limited to object-level diff evidence.

## References

- Root `AGENTS.md` Andromeda invariants.
- `crates/AGENTS.md` Rust crate instructions.
