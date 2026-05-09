# andromeda-test-support

## Purpose

`andromeda-test-support` owns shared, runtime-free mechanical test support for Andromeda crates.

## Scope

- Own mechanical helpers for process execution, workspace paths, temporary paths, and small collection builders.
- Keep reusable test-only utilities out of production crates.
- Keep semantic assertions in the crate-specific tests that own the behavior.

## Non-goals

- Do not own production logic, benchmark authority, catalog behavior, storage behavior, recovery protocol behavior, application-facing behavior, durable runtime truth, doctrine constants, or command semantics.
- Do not hide subsystem-specific assertions in generic test helpers.

## Prerequisites

- Rust 2024 toolchain aligned with the repository baseline.
- Helpers are mechanical and do not depend on production Andromeda crates.

## Procedure

1. Add only narrow mechanical helpers.
2. Keep semantic assertions, protocol meaning, runtime state, and doctrine policy in the owning crate tests.
3. Avoid dependencies on production crates.

## Validation

- Inspect `src/lib.rs` and helper modules for mechanical test-support scope.
- When validating by command, use `cargo check -p andromeda-test-support --all-targets`.

## Troubleshooting

- If tests fail with missing workspace values, verify this crate path and root workspace files are intact.
- If a helper needs subsystem semantics, move that logic to the owning crate test.

## References

- `AGENTS.md`
- `crates/README.md`
