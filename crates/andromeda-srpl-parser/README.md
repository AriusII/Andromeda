# andromeda-srpl-parser

## Purpose

`andromeda-srpl-parser` owns SRPL tokenization and syntax parsing for the
bounded compiler slice.

Use this crate when code needs to turn SRPL source text into `ProcedureAst`
while preserving source spans and parser diagnostics.

The parser accepts a narrow Procedure declaration grammar. It does not bind
catalog objects, lower IR, execute runtime behavior, or publish Procedure
contracts.

## Scope

This crate is responsible for:

| Area | Responsibility |
| --- | --- |
| Lexer facade | Reexporting `lex`, `Token`, and `TokenKind` from the SRPL lexer crate for parser callers. |
| Procedure parsing | Parsing one narrow Procedure declaration into `ProcedureAst`. |
| Type syntax | Parsing the accepted scalar type names and rejecting forbidden or ambiguous type surfaces. |
| Cardinality syntax | Parsing explicit `one`, `optional one`, `optional_one`, `many`, `nonempty many`, and `non_empty_many` result forms. |
| Body grammar | Parsing bounded `body { ... }` and `begin ... end` operation forms into AST operations. |
| Diagnostics | Returning `SrplDiagnostic` values with parsing phase and source spans. |

The parser is an SRPL language-model crate. It must stay independent of catalog
storage, execution engines, transport runtimes, benchmarks, analytics, and GPU
work.

## Non-goals

- Do not bind catalog objects, lower IR, optimize, execute, resolve Procedures,
  or materialize catalog definitions.
- Do not introduce application-facing ad hoc SQL.
- Do not parse dynamic table names, dynamic predicates, shape-shifting returns,
  or implicit null semantics as accepted SRPL.
- Do not accept wildcard result shapes such as `select *`.
- Do not depend on catalog store implementations, execution crates, storage,
  transport runtime crates, benchmark crates, analytics crates, or GPU crates.
- Do not serialize Rust native structs directly to disk or network.

## Prerequisites

Before changing this crate, understand:

- The parser accepts only one narrow Procedure declaration per source input.
- Parser diagnostics must carry source spans when possible.
- Procedure body operations are bounded by `MAX_SRPL_BODY_OPERATIONS`.
- Cardinality and result columns must be explicit.
- Ambiguous absence syntax and float scalar surfaces are rejected by current
  parser owner tests.
- Parser output is syntax only. Binder and lowering own semantic validation.

## Procedure

1. Keep `lib.rs` limited to module declarations and intentional reexports.
2. Parse only deterministic grammar forms that lower into typed AST nodes.
3. Preserve source spans for names, types, cardinality, result streams, and body
   operations.
4. Keep body operation counts bounded and ordinals dense.
5. Reject dynamic table names, dynamic predicates, shape-shifting returns,
   wildcard returns, and implicit null semantics during parsing when they appear
   in source syntax.
6. Add parser tests for every accepted grammar form and every rejected ambiguous
   surface.
7. Keep lexer behavior in the lexer owner crate and expose it here only as a
   parser-facing compatibility import.

## Validation

For documentation-only changes, validate this README against:

- `AGENTS.md`
- `crates/AGENTS.md`
- `crates/README.md`
- `docs/adr/ADR-0011-workspace-crate-boundaries.md`

For code changes in this crate, prefer:

```bash
cargo fmt --package andromeda-srpl-parser --check
cargo test -p andromeda-srpl-parser
cargo check -p andromeda-srpl-parser --all-targets
```

If parser behavior changes the compiler surface, also run downstream
`andromeda-srpl` compiler pipeline and validation-gate tests.

## Troubleshooting

| Symptom | Corrective action |
| --- | --- |
| A source contains two Procedure declarations. | Reject it or split the source before parsing; this parser accepts one narrow declaration. |
| A result stream wants wildcard or runtime-selected columns. | Reject it and require explicit columns in the Procedure contract. |
| A predicate appears as arbitrary text. | Keep only accepted bounded syntax and let binder/lowering produce closed predicate IR. |
| A target name is assembled from input values. | Reject dynamic table names and require parsed `QualifiedName` syntax. |
| A nullable or float surface is requested. | Keep absence and numeric policy explicit through accepted type descriptors and diagnostics. |
| A parser diagnostic lacks a useful span. | Return an `SrplDiagnostic` with parsing phase and the narrowest available source span. |

## References

- `AGENTS.md`
- `crates/AGENTS.md`
- `crates/README.md`
- `docs/adr/ADR-0011-workspace-crate-boundaries.md`
- `crates/andromeda-srpl-parser/src/lib.rs`
- `crates/andromeda-srpl-parser/src/parser.rs`
- `crates/andromeda-srpl-parser/src/parser/statements.rs`
- `crates/andromeda-srpl-parser/src/parser/types.rs`
- `crates/andromeda-srpl-parser/tests/owner_direct.rs`
