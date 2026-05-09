# Andromeda Contract Compat

## Purpose

`andromeda-contract-compat` is the runtime-free owner for Procedure contract
compatibility taxonomy identifiers. It defines stable reserved names that design
work can reference before any compatibility decision engine is implemented.

## Scope

- Procedure contract compatibility taxonomy identifiers.
- Reserved compatibility outcome classes.
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

Use the exported taxonomy entries as stable names only. Typed compatibility
rules can be added here only after catalog, contract, WAL, and audit
responsibilities are assigned.

## Validation

```powershell
cargo fmt --manifest-path crates/andromeda-contract-compat/Cargo.toml --check
cargo check --manifest-path crates/andromeda-contract-compat/Cargo.toml
cargo test --manifest-path crates/andromeda-contract-compat/Cargo.toml
```

## Troubleshooting

If a caller needs an actual compatibility decision, keep that behavior in the
contract, catalog, audit, or release owner until this crate receives typed rules
and owner tests for that decision surface.

## References

- Root `AGENTS.md` Andromeda invariants.
- `crates/AGENTS.md` Rust crate instructions.
