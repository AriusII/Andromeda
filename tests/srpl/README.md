# SRPL Test Index

## Purpose

This directory is a documentation index for SRPL validation. It does not own an executable Rust harness.

Executable SRPL tests remain with the crates that own parsing, binding, cardinality, lowering, typed IR, diagnostics, optimization, Procedure contracts, and DefinitionBatch integration.

## Scope

Use this index for roadmap entries that refer to `tests/srpl`.

Entries should name the owning crate, SRPL layer, rejected or accepted behavior, diagnostic expectation, and validation command.

## Test destination

| Scenario type | Destination |
| --- | --- |
| Lexer, parser, AST, binding, cardinality, diagnostics, or lowering behavior | The owning SRPL crate's `tests/` directory or crate-local unit tests. |
| Typed IR, optimizer, semantic hash, ProcedureContract, or DefinitionBatch integration | The owning SRPL, contract, catalog, or DefinitionBatch crate's tests. |
| Rejected dynamic names, predicates, shape-shifting returns, or implicit null semantics | The crate that owns the semantic rejection and diagnostic code. |
| Fuzz-discovered SRPL parser or canonicalization defect | A deterministic crate-local parser, binder, or diagnostics regression; keep fuzz target and corpus metadata in `fuzz/` and index them through `tests/fuzzing/`. |
| Root roadmap SRPL coverage | This README, as an index entry that points to the owning crate command. |

## Non-goals

- Do not introduce application-facing ad hoc SQL.
- Do not permit dynamic table names, dynamic predicates, shape-shifting returns, or implicit null semantics.
- Do not bypass typed Procedure contracts.
- Do not create root-level SRPL harnesses without an explicit future work order.
- Do not duplicate crate-local SRPL tests, fuzz targets, seed corpora, or generated fixtures in this directory.

## Prerequisites

- Review `tests/README.md`.
- Identify the parser, binder, IR, optimizer, contract, or catalog boundary that owns the behavior.

## Procedure

1. Map the SRPL scenario to the owning crate.
2. Keep executable parser, binder, lowering, optimizer, or contract tests in that crate.
3. Convert fuzz-discovered SRPL failures into deterministic parser, binder, diagnostics, or lowering regressions before citing them here.
4. Record the expected diagnostic, IR, contract, or catalog evidence in this index when the scenario is ready.

## Validation

For this documentation index, run:

```powershell
rg -n "Purpose|Scope|Validation" tests/srpl
```

Runtime validation belongs to the owning SRPL or catalog crate test command.

## Troubleshooting

If a scenario depends on catalog publication or ProcedureContract compatibility, record both owners instead of moving execution into this directory.

## References

- `tests/README.md`
- `docs/codex/rust-critical-quality-gates.md`
- `documentations/testing/step-11-validation-matrix.md`
