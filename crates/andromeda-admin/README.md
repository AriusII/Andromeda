# andromeda-admin

## Purpose

`andromeda-admin` reserves the administration facade boundary for Andromeda.

## Scope

- Own marker-only administration crate identity.
- Keep administration code separate from the Application Surface.
- Keep the crate runtime-free until concrete administration contracts are defined.

## Non-goals

- Do not implement permission enforcement, HA/DR behavior, storage access, Procedure execution, or business logic.
- Do not expose administration behavior through the application runtime.
- Do not add dependencies while the crate remains marker-only.

## Prerequisites

- Rust 2024 workspace configuration is available from the repository root.
- Administration contracts are defined before behavior is added.

## Procedure

1. Keep `Cargo.toml` dependency-free unless a documented administration owner requires otherwise.
2. Keep `src/lib.rs` limited to marker types and crate-level ownership documentation.
3. Move runtime authorization or audit behavior to the owning security, IAM, or audit crate.

## Validation

- Inspect `src/lib.rs` and `Cargo.toml` for marker-only scope.
- When behavior is added, run a package-level check before updating this README.

## Troubleshooting

- If application execution appears here, move it to the Procedure runtime owner.
- If security decisions appear here, move them to `andromeda-iam` or the security contract owner.

## References

- `AGENTS.md`
- `crates/README.md`
