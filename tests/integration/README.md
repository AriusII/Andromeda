# Integration Test Index

## Purpose

This directory is a documentation index for integration validation. It does not own an executable Rust harness.

Executable integration tests remain in the crates that own the Procedure path, catalog behavior, transaction behavior, storage behavior, RPC boundary, or observability surface being validated.

## Scope

Use this index to track root-level roadmap references for cross-crate integration scenarios.

Entries should name the owning crate, target behavior, risk class, and command that validates the scenario.

## Test destination

| Scenario type | Destination |
| --- | --- |
| Single-crate behavior, API compatibility, or ownership contract | The owning crate's `tests/` directory or crate-local unit tests. |
| Cross-crate Procedure execution path | The crate that owns orchestration, usually `crates/andromeda-exec/tests/`, with dependent crate suites referenced as evidence. |
| Catalog, transaction, storage, RPC, security, or observability behavior exercised through integration | The crate that owns the highest-risk boundary being asserted. |
| Fuzz-discovered integration regression | A deterministic crate-local regression test; keep fuzz target and corpus metadata in `fuzz/` and index them through `tests/fuzzing/`. |
| Roadmap-only integration coverage | This README, as an index entry that points to the owning crate command. |

## Non-goals

- Do not move crate-owned tests into this directory.
- Do not create root-level harnesses without an explicit future work order.
- Do not duplicate crate-local tests, fuzz targets, seed corpora, or generated fixtures in this directory.
- Do not use ad hoc SQL or bypass typed Procedure contracts.
- Do not treat benchmark, RAM, or temporary output as correctness evidence.

## Prerequisites

- Review `tests/README.md` before adding or updating integration entries.
- Identify the owning crate and release gate for the behavior under test.

## Procedure

1. Locate the crate that owns the integration behavior.
2. Add or update the executable test in that crate.
3. Convert fuzz-discovered failures into deterministic crate-local regressions before citing them here.
4. Record the validation command and evidence location in this index when the scenario is ready.

## Validation

For this documentation index, run:

```powershell
rg -n "Purpose|Scope|Validation" tests/integration
```

Runtime validation belongs to the owning crate test command recorded by the future scenario entry.

## Troubleshooting

If an integration scenario needs shared setup, define the setup at the owning crate or test-support boundary before adding executable root-level assets.

## References

- `tests/README.md`
- `docs/codex/rust-critical-quality-gates.md`
- `documentations/testing/step-11-validation-matrix.md`
