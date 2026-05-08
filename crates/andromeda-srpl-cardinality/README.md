# andromeda-srpl-cardinality

## Purpose

`andromeda-srpl-cardinality` owns the SRPL result cardinality policy shared by
parser, AST, IR, contract materialization, optimizer validation, and execution
adapter boundaries.

Use this crate when code needs to reason about whether a result stream or read
operation is `one`, `optional one`, `many`, or `nonempty many`.

Cardinality is explicit SRPL contract data. It is not a runtime guess and must
not be inferred from stored rows, optimizer estimates, benchmark output, or
adapter behavior.

## Scope

This crate is responsible for:

| Area | Responsibility |
| --- | --- |
| Cardinality variants | Closed SRPL cardinality vocabulary: `One`, `OptionalOne`, `Many`, and `NonEmptyMany`. |
| Row-count policy | Minimum row counts, intrinsic maximum row counts, exact-count requirements, and declared `row_count_max` compatibility. |
| Emit-count policy | Validation helpers for result-stream emit operation counts. |
| Contract conversion | Conversion to and from `andromeda_contract::ResultStreamCardinality`. |

This crate is a contract-safe language-model crate. It must stay free of
catalog storage, execution engines, transport runtimes, benchmarks, analytics,
and GPU work.

## Non-goals

- Do not parse source text, build AST nodes, lower IR, execute Procedures, or
  inspect catalog storage.
- Do not introduce application-facing ad hoc SQL.
- Do not use cardinality to allow dynamic table names, dynamic predicates,
  shape-shifting returns, or implicit null semantics.

The SRPL restriction set stays explicit: no dynamic table names, dynamic
predicates, shape-shifting returns, or implicit null semantics.
- Do not infer result shape from observed runtime row counts.
- Do not make optimizer estimates or benchmark results authoritative.
- Do not serialize Rust native structs directly to disk or network.

## Prerequisites

Before changing this crate, understand:

- Result cardinality is part of the typed Procedure contract.
- `One` and `OptionalOne` have an intrinsic upper bound of one row.
- `Many` and `NonEmptyMany` require explicit contract bounds when a bounded
  upper row count is needed by downstream code.
- `One` and `NonEmptyMany` require at least one emitted row.
- Cardinality does not imply nullable fields. Field absence policy belongs to
  the type descriptor.

## Procedure

1. Keep the cardinality vocabulary closed unless a language ADR accepts a new
   variant.
2. Add validation helpers as pure functions over cardinality and row counts.
3. Keep conversions to Procedure contract cardinality explicit and exhaustive.
4. Preserve row-count minimums and intrinsic maximums when adding downstream
   validations.
5. Reject designs that use cardinality to hide dynamic returns or implicit null
   behavior.
6. Keep tests exhaustive across all variants.

## Validation

For documentation-only changes, validate this README against:

- `AGENTS.md`
- `crates/AGENTS.md`
- `crates/README.md`
- `docs/adr/ADR-0011-workspace-crate-boundaries.md`

For code changes in this crate, prefer:

```bash
cargo fmt --package andromeda-srpl-cardinality --check
cargo test -p andromeda-srpl-cardinality
cargo check -p andromeda-srpl-cardinality --all-targets
```

If a change affects Procedure contract conversion, also run downstream SRPL IR,
parser, and facade tests.

## Troubleshooting

| Symptom | Corrective action |
| --- | --- |
| A result stream sometimes emits different columns. | Reject the design. Cardinality controls row counts, not result shape. |
| A caller treats `OptionalOne` as a nullable row. | Keep the row optional and model field absence through explicit type policy. |
| A `Many` result needs an upper bound for execution. | Require an explicit bound from the Procedure contract or adapter request. |
| A row count estimate is used to validate contract cardinality. | Replace the estimate with declared cardinality and validated row-count metadata. |
| A new cardinality variant seems convenient. | Treat it as an SRPL language change and update parser, AST, IR, contract conversion, and tests together. |

## References

- `AGENTS.md`
- `crates/AGENTS.md`
- `crates/README.md`
- `docs/adr/ADR-0011-workspace-crate-boundaries.md`
- `crates/andromeda-srpl-cardinality/src/lib.rs`
- `crates/andromeda-srpl-cardinality/src/cardinality.rs`
