# andromeda-srpl-interpreter

## Purpose

`andromeda-srpl-interpreter` owns deterministic SRPL IR interpretation and execution-shape orchestration.

## Scope

- Define a clean boundary for SRPL interpreter orchestration APIs.
- Keep interpretation concerns separated from lowering, diagnostics, and parser concerns.
- Execute catalog-bound SRPL IR only through typed execution-adapter contracts.

## Non-goals

- Own IR lowering or parsing logic (existing SRPL crates already cover those responsibilities).
- Handle catalog mutation or storage durability concerns.
- Implement end-to-end transaction execution logic.

## Prerequisites

- Rust 2024 workspace baseline.
- Familiarity with existing SRPL parser/binder/lowering crates.

## Procedure

1. Keep public API narrow while the interpreter module shape evolves.
2. Place execution semantics under explicit module boundaries.
3. Keep adapter side effects behind whole-plan validation.

## Validation

- `cargo check -p andromeda-srpl-interpreter --all-targets`
- Add focused tests when interpreter behavior changes.

## Troubleshooting

- If scope drifts into parser or optimizer areas, split the API boundary before coding.
- If tests require shared SRPL AST fixtures, evaluate placement in dedicated fixture crates.

## References

- `AGENTS.md`
- `crates/AGENTS.md`
- `crates/andromeda-srpl/src/lib.rs`
