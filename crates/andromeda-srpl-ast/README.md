# andromeda-srpl-ast

## Purpose

`andromeda-srpl-ast` owns the SRPL syntax data shapes produced by the parser.

Use this crate when code needs parser-owned Procedure AST nodes with source
spans, field declarations, result stream declarations, bounded body operation
shapes, and explicit cardinality markers.

This crate preserves source-level structure. It does not decide catalog binding,
runtime execution, optimizer behavior, or durable publication.

## Scope

This crate is responsible for:

| Area | Responsibility |
| --- | --- |
| Spanned values | `Spanned<T>` values that preserve source byte spans for parsed syntax. |
| Procedure shape | `ProcedureAst`, parameter fields, result streams, and Procedure body nodes. |
| Body operations | Syntax-level read, assert, update, emit, raise, ensure, update-set, and return forms accepted by the bounded parser slice. |
| Cardinality carry-through | Explicit result and read cardinality values without collapsing `one`, `optional one`, `many`, or `nonempty many`. |

The AST is an intermediate syntax model. It may reference contract-safe
identifiers and type descriptors, but it must remain independent of catalog
storage, execution engines, transport runtimes, benchmarks, analytics, and GPU
work.

## Non-goals

- Do not lex or parse source text in this crate.
- Do not bind catalog objects, lower to IR, optimize, execute, resolve
  Procedures, or materialize catalog definitions.
- Do not introduce application-facing ad hoc SQL.
- Do not represent dynamic table names, dynamic predicates, shape-shifting returns,
  or implicit null semantics as accepted SRPL AST features.
- Do not use the AST as a durable disk or network format.
- Do not serialize Rust native structs directly to disk or network.
- Do not depend on catalog store implementations, execution crates, storage,
  transport runtime crates, benchmark crates, analytics crates, or GPU crates.

## Prerequisites

Before changing this crate, understand:

- AST nodes are source-preserving shapes, not semantic authority.
- Source spans must remain valid byte offsets into the original source.
- Field and stream shapes must stay explicit so downstream binding can reject
  ambiguity.
- Null behavior is not implicit. Absence must remain part of the explicit type
  contract owned by Andromeda type descriptors.
- Adding an AST variant can expand the SRPL surface and should be treated as a
  language-design change.

## Procedure

1. Add only syntax data shapes that the parser can construct deterministically.
2. Keep `lib.rs` limited to module declarations and intentional reexports.
3. Preserve `SourceSpan` on parsed user-facing names, types, cardinality, and
   body operations.
4. Keep result streams and fields explicit; do not add open-ended maps,
   dynamic column lists, or shape-dependent result containers.
5. Keep predicate-like and target-like source fragments as bounded syntax forms
   until binder and lowering can validate them.
6. Update owner tests when a public AST shape changes.

## Validation

For documentation-only changes, validate this README against:

- `AGENTS.md`
- `crates/AGENTS.md`
- `crates/README.md`
- `docs/adr/ADR-0011-workspace-crate-boundaries.md`

For code changes in this crate, prefer:

```bash
cargo fmt --package andromeda-srpl-ast --check
cargo test -p andromeda-srpl-ast
cargo check -p andromeda-srpl-ast --all-targets
```

If a change adds a syntax variant, also run parser and compatibility tests that prove
the new shape is constructed, rejected, or reexported intentionally.

## Troubleshooting

| Symptom | Corrective action |
| --- | --- |
| A new AST field needs catalog lookup to validate. | Keep the field syntactic and move catalog validation to binder or lowering. |
| A result stream can change columns at runtime. | Reject the shape. SRPL result streams must have fixed declared columns. |
| A predicate is represented as arbitrary executable text. | Replace it with a bounded syntax form that binder and IR can validate. |
| A target name is assembled from runtime fragments. | Reject dynamic table names and require a parsed `QualifiedName`. |
| A value needs implicit null behavior. | Use explicit `TypeDescriptor` absence policy instead of adding an implicit AST null path. |

## References

- `AGENTS.md`
- `crates/AGENTS.md`
- `crates/README.md`
- `docs/adr/ADR-0011-workspace-crate-boundaries.md`
- `crates/andromeda-srpl-ast/src/lib.rs`
- `crates/andromeda-srpl-ast/src/ast.rs`
- `crates/andromeda-srpl-ast/tests/owner_direct.rs`
