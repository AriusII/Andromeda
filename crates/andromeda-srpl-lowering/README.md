# andromeda-srpl-lowering

## Purpose

`andromeda-srpl-lowering` owns SRPL lowering from typed, bound semantic input
into bounded SRPL IR.

The historical `andromeda-srpl` crate remains the compatibility facade for
source parsing, facade-local binding wrappers, optimizer entry points, catalog
binding, DefinitionBatch helpers, and existing public imports.

## Scope

This crate is intended to own:

| Area | Responsibility |
| --- | --- |
| Bound-to-IR lowering | Convert typed SRPL Procedure body shapes into bounded semantic IR. |
| Lowering validation | Preserve dense operation ordinals, fixed result streams, explicit cardinality, and closed predicate/value forms. |
| Lowering input | Accept `ProcedureSignature` plus `ProcedureBodyAst` through `BoundProcedureLoweringInput`. |

The lowering crate sits after parser or binder ownership and before optimizer,
interpreter, execution adapter, catalog bridge behavior, or runtime execution.

## Dependency Direction

Dependencies must point only toward lower or contract-safe crates. Current
dependencies are limited to `andromeda-error`, `andromeda-srpl-ast`, and
`andromeda-srpl-ir`.

Do not depend on `andromeda-srpl` as a facade from this crate after extraction.
Do not depend on catalog storage, execution, storage, transaction, WAL, QUIC
transport, benchmark, analytics, GPU, or application-surface crates.

## Non-goals

- Do not lex or parse SRPL source text.
- Do not perform catalog name binding.
- Do not materialize catalog definitions or DefinitionBatches.
- Do not execute Procedure behavior or dispatch runtime adapters.
- Do not read or write storage, write WAL, publish catalog versions, or decide
  transaction visibility.
- Do not introduce application-facing ad hoc SQL.
- Do not represent dynamic table names, dynamic predicates, shape-shifting
  returns, or implicit null semantics in lowered output.
- Do not serialize Rust native structs directly to disk or network.
- Do not depend on `andromeda-srpl` for facade convenience.

## Prerequisites

Before changing this crate, understand:

- Lowering consumes typed semantic input supplied by the facade or binder owner.
- `andromeda-srpl-ir` owns semantic IR data shapes and validation.
- Lowering must preserve fixed Procedure contracts and bounded body operation
  semantics.
- Catalog binding and DefinitionBatch helpers still live behind the
  `andromeda-srpl` facade.

## Procedure

1. Keep `src/lib.rs` limited to module declarations and intentional reexports.
2. Preserve facade compatibility by routing `andromeda-srpl` entry points
   through this crate.
3. Keep predicate, assignment, emit, and constant lowering closed and explicit.
4. Reject any design that depends on hidden runtime state or dynamic result
   shapes.

## Validation

```powershell
cargo fmt --package andromeda-srpl-lowering --check
cargo check -p andromeda-srpl-lowering --all-targets
cargo test -p andromeda-srpl-lowering
```

## Troubleshooting

| Symptom | Corrective action |
| --- | --- |
| Lowering needs to parse source text. | Keep parsing in the parser crate and pass typed AST or bound input forward. |
| Lowering needs catalog store access. | Move name resolution to binder or use typed binding evidence. |
| A lowered predicate is arbitrary executable text. | Replace it with a closed typed IR predicate. |
| Lowering wants to execute an operation to know its shape. | Reject the design; result shape must be known before runtime. |

## References

- `AGENTS.md`
- `crates/AGENTS.md`
- `crates/README.md`
- `docs/adr/ADR-0011-workspace-crate-boundaries.md`
- `docs/adr/ADR-0018-engine-crate-mapping-policy.md`
- `crates/andromeda-srpl/src/lowering.rs`
- `crates/andromeda-srpl/src/lowering/`
- `crates/andromeda-srpl-ir/README.md`
