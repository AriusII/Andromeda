# andromeda-storage-index

## Purpose

`andromeda-storage-index` is the owner crate for storage index identity and configuration boundaries.

## Scope

- Own storage-index identity newtypes and configuration guardrails.
- Reserve module boundaries for index metadata and lookup structure ownership.
- Keep a clean boundary for future storage index interfaces.
- Avoid implementing protocol-critical behavior in the scaffold state.

## Non-goals

- No physical index write path logic yet.
- No application-facing ad hoc SQL.
- No durable protocol or format ownership until behavior is explicitly assigned.

## Prerequisites

- Rust 2024 toolchain aligned with the repository baseline.
- Design of index ownership should be finalized by the subsystem owner before adding behavior.

## Procedure

This crate is intentionally minimal. Keep `src/lib.rs` limited to module declarations and
public reexports when future modules are introduced.

## Validation

```powershell
cargo fmt --manifest-path crates/andromeda-storage-index/Cargo.toml --check
cargo check --manifest-path crates/andromeda-storage-index/Cargo.toml
cargo test --manifest-path crates/andromeda-storage-index/Cargo.toml
```

## Troubleshooting

If this crate cannot resolve workspace values, ensure it remains under the repository
`crates/` tree with the existing root workspace files in place.

## References

- AGENTS instructions in repository root and `crates/AGENTS.md`.
