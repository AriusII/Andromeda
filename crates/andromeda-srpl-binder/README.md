# andromeda-srpl-binder

## Purpose

`andromeda-srpl-binder` is the future owner for SRPL semantic binding.

Use this crate when a later extraction needs a dedicated boundary for turning
parsed SRPL AST shapes into bound, catalog-aware compiler input for lowering.
The current scaffold intentionally contains no behavior. Existing binder code
remains in `andromeda-srpl` until a later behavior-preserving migration moves
it with compatibility tests.

## Scope

This crate is intended to own:

| Area | Responsibility |
| --- | --- |
| Name binding | Resolve parsed Procedure names, parameter names, result stream names, bindings, fields, and targets into typed compiler-owned references. |
| Contract validation | Validate fixed Procedure input and result shapes against contract-safe descriptors. |
| Catalog evidence | Carry catalog binding evidence needed by lowering without owning catalog storage or publication. |
| Diagnostics | Report SRPL binding failures with stable source locations and typed diagnostics. |

The binder sits after parser/AST ownership and before lowering/IR ownership. It
must preserve explicit cardinality, explicit absence policy, fixed result
shapes, and deterministic Procedure contracts.

## Dependency Direction

This scaffold has no dependencies because no behavior has moved yet.

When behavior is extracted, dependencies must point only toward lower or
contract-safe crates such as `andromeda-error`, `andromeda-types`,
`andromeda-contract`, `andromeda-srpl-diagnostics`,
`andromeda-srpl-cardinality`, `andromeda-srpl-ast`, and
`andromeda-srpl-ir`.

Do not depend on `andromeda-srpl` as a facade from this crate after extraction.
Do not depend on execution, storage, transaction, WAL, QUIC transport,
benchmark, analytics, GPU, or application-surface crates.

## Non-goals

- Do not lex or parse SRPL source text.
- Do not lower bound forms into executable IR.
- Do not execute Procedure behavior, read or write storage, write WAL, publish
  catalog versions, or decide transaction visibility.
- Do not introduce application-facing ad hoc SQL.
- Do not allow dynamic table names, dynamic predicates, shape-shifting returns,
  or implicit null semantics.
- Do not serialize Rust native structs directly to disk or network.
- Do not move behavior from `andromeda-srpl` in this scaffold.

## Prerequisites

Before changing this crate, understand:

- Binding is semantic validation, not runtime execution.
- Catalog facts used by the binder must be typed, versioned, and explicit.
- Result stream shapes and cardinality must remain fixed before lowering.
- The current source of behavior is still `crates/andromeda-srpl/src/binder.rs`.

## Procedure

1. Keep `src/lib.rs` limited to crate-level documentation until behavior moves.
2. Move binder behavior only in a dedicated extraction change with facade
   compatibility tests.
3. Preserve diagnostics and source spans when introducing public binding types.
4. Keep catalog interaction behind contract-safe evidence shapes.
5. Reject any design that requires application-facing SQL or runtime-dependent
   result shapes.

## Validation

For this scaffold, file-shape validation is sufficient because the crate is not
yet a workspace member:

```powershell
rg -n "forbid\\(unsafe_code\\)|Purpose|Scope|Non-goals|Dependency Direction" crates/andromeda-srpl-binder
rg --files crates/andromeda-srpl-binder
```

When this crate becomes a workspace member, add package-level Rust gates such as:

```powershell
cargo fmt --package andromeda-srpl-binder --check
cargo check -p andromeda-srpl-binder --all-targets
cargo test -p andromeda-srpl-binder
```

## Troubleshooting

| Symptom | Corrective action |
| --- | --- |
| Binding needs to read mutable catalog storage directly. | Introduce or reuse a typed catalog evidence boundary instead of depending on store internals. |
| A result stream can change shape after binding. | Reject the shape; SRPL result contracts must stay fixed. |
| Binder output contains raw executable predicate text. | Replace it with typed bound predicate data that lowering can validate. |
| The crate needs runtime execution types. | Move the dependency to the execution adapter or execution owner instead. |

## References

- `AGENTS.md`
- `crates/AGENTS.md`
- `crates/README.md`
- `docs/adr/ADR-0011-workspace-crate-boundaries.md`
- `docs/adr/ADR-0018-engine-crate-mapping-policy.md`
- `crates/andromeda-srpl/src/binder.rs`
