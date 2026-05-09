# andromeda-business-fixtures

## Purpose

`andromeda-business-fixtures` owns reusable business-domain fixtures for tests and examples outside production execution.

## Scope

- Own ProductStock fixture constants and value helpers.
- Provide reusable business-domain inputs for tests and examples.
- Keep fixture data separate from `andromeda-exec` production behavior.

## Non-goals

- Do not own production execution, Procedure dispatch, catalog state, storage truth, WAL durability, recovery, or security behavior.
- Do not become an alternate business runtime.
- Do not treat fixture values as doctrine constants or production defaults.

## Prerequisites

- Consumers use fixtures only in tests, examples, or validation scaffolding.
- Durable behavior remains owned by the storage, WAL, transaction, or recovery crate under test.

## Procedure

1. Add only small, deterministic business-domain fixtures.
2. Keep semantic assertions in the tests that own the target subsystem.
3. Avoid dependencies that turn fixtures into runtime behavior.

## Validation

- Inspect `src/lib.rs` for fixture-only exports.
- When validating by command, use `cargo check -p andromeda-business-fixtures --all-targets`.

## Troubleshooting

- If a fixture starts enforcing production semantics, move that assertion to the owning crate test.
- If runtime code depends on this crate, split the production type into the owning runtime crate.

## References

- `AGENTS.md`
- `crates/README.md`
- `crates/andromeda-exec/README.md`
