# andromeda-srpl-ir

## Purpose

`andromeda-srpl-ir` owns the bounded semantic IR and Procedure signature model
used after SRPL parsing and binding.

Use this crate when code needs typed Procedure signatures, result contracts,
bounded Procedure body IR, operation plans, catalog binding evidence,
compile-time constants, predicates, assignments, emit values, or Procedure
contract metadata.

The IR is the semantic contract boundary for SRPL compiler work. It must keep
Procedure names, inputs, result streams, row-count policy, predicates,
assignments, and constants explicit.

## Scope

This crate is responsible for:

| Area | Responsibility |
| --- | --- |
| Procedure signatures | `ProcedureSignature` and `ResultContract` validation for dense fields, unique names, and explicit result contracts. |
| Bounded Procedure IR | `SrplProcedureIr`, result streams, body operations, and the `MAX_SRPL_BODY_OPERATIONS` guard. |
| Operation vocabulary | Typed read, assert, update, emit, and raise operations over validated symbols and qualified names. |
| Predicate and value IR | Closed predicate forms, assignments, emit values, constants, and bounded arithmetic expressions. |
| Plan shapes | Bound body plans, bound operation plans, and executable Procedure plan data shapes. |
| Evidence shapes | Catalog binding evidence and Procedure contract metadata used by higher-level compiler and bridge code. |

This crate is a language-model crate. It does not parse source text, execute
runtime behavior, own catalog storage, write WAL, or publish catalog changes.

## Non-goals

- Do not lex or parse source text.
- Do not execute Procedure behavior, read storage, write WAL, publish catalog
  versions, or own transaction visibility.
- Do not introduce application-facing ad hoc SQL.
- Do not represent dynamic table names, dynamic predicates, shape-shifting returns,
  or implicit null semantics in the IR.
- Do not add a `Null` constant or hidden nullable value path.
- Do not make optimizer estimates or benchmark output part of semantic truth.
- Do not serialize Rust native structs directly to disk or network.
- Do not depend on catalog store implementations, execution crates, storage,
  transport runtime crates, benchmark crates, analytics crates, or GPU crates.

## Prerequisites

Before changing this crate, understand:

- IR shapes are typed and bounded. They are not raw source text.
- `SrplProcedureBodyIr` must keep operation ordinals dense and zero-based.
- `SrplPredicateIr` intentionally has closed predicate forms. Dynamic predicate
  text is not allowed.
- `ConstantLiteral` is closed and deliberately has no `Null` variant.
- `MAX_EXPR_DEPTH` protects recursive expression handling.
- Result contracts require explicit names, cardinality, and non-empty dense
  columns.

## Procedure

1. Model new semantic constructs as typed IR variants with explicit validation.
2. Keep `lib.rs` limited to module declarations and intentional reexports.
3. Validate qualified names, symbols, dense ordinals, row bounds, assignments,
   emit values, and expression depth at IR boundaries.
4. Keep predicate forms closed. Add new predicates only when parser, binder,
   lowering, optimizer, execution adapter, and tests can preserve the same
   semantics.
5. Keep result shape fixed. Do not add maps, wildcard columns, dynamic result
   streams, or runtime-dependent column lists.
6. Keep constant folding semantics compatible with the closed constant literal
   vocabulary and explicit absence policy.

## Validation

For documentation-only changes, validate this README against:

- `AGENTS.md`
- `crates/AGENTS.md`
- `crates/README.md`
- `docs/adr/ADR-0011-workspace-crate-boundaries.md`

For code changes in this crate, prefer:

```bash
cargo fmt --package andromeda-srpl-ir --check
cargo test -p andromeda-srpl-ir
cargo check -p andromeda-srpl-ir --all-targets
```

If a change affects lowering, optimizer, contract materialization, or execution
adapter contracts, also run the relevant `andromeda-srpl` compatibility tests.

## Troubleshooting

| Symptom | Corrective action |
| --- | --- |
| A predicate is stored as arbitrary text. | Replace it with a closed `SrplPredicateIr` variant and validation. |
| A result stream can add or remove columns at runtime. | Reject the design and keep result contracts fixed. |
| A target table name is assembled from values. | Require a validated `QualifiedName`; dynamic table names are not SRPL core. |
| A constant needs null semantics. | Model absence through explicit type policy outside `ConstantLiteral`; do not add implicit null. |
| An operation list has sparse ordinals. | Normalize at construction or reject with SRPL validation. |
| A new IR variant affects Procedure compatibility. | Update contract materialization, compatibility tests, and documentation in the same change. |

## References

- `AGENTS.md`
- `crates/AGENTS.md`
- `crates/README.md`
- `docs/adr/ADR-0011-workspace-crate-boundaries.md`
- `crates/andromeda-srpl-ir/src/lib.rs`
- `crates/andromeda-srpl-ir/src/signature.rs`
- `crates/andromeda-srpl-ir/src/ir.rs`
- `crates/andromeda-srpl-ir/src/ir/procedure.rs`
- `crates/andromeda-srpl-ir/src/ir/values.rs`
