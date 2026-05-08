# andromeda-srpl-lowering

## Purpose

`andromeda-srpl-lowering` is the future owner for SRPL lowering from bound
semantic input into typed IR and contract-ready compiler output.

The current scaffold intentionally contains no behavior. Existing lowering code
remains in `andromeda-srpl` until a later behavior-preserving migration moves it
with facade compatibility tests.

## Scope

This crate is intended to own:

| Area | Responsibility |
| --- | --- |
| Bound-to-IR lowering | Convert bound SRPL Procedure shapes into bounded semantic IR. |
| Contract candidate shaping | Produce contract-safe Procedure metadata and result contract candidates from validated compiler input. |
| Lowering validation | Preserve dense operation ordinals, fixed result streams, explicit cardinality, and closed predicate/value forms. |
| Lowering diagnostics | Report deterministic lowering failures without panics or runtime-dependent state. |

The lowering crate sits after binder ownership and before optimizer,
interpreter, execution adapter, or catalog bridge behavior. It must not become a
runtime execution owner.

## Dependency Direction

This scaffold has no dependencies because no behavior has moved yet.

When behavior is extracted, dependencies must point only toward lower or
contract-safe crates such as `andromeda-error`, `andromeda-types`,
`andromeda-contract`, `andromeda-srpl-diagnostics`,
`andromeda-srpl-cardinality`, `andromeda-srpl-ast`,
`andromeda-srpl-binder`, and `andromeda-srpl-ir`.

Do not depend on `andromeda-srpl` as a facade from this crate after extraction.
Do not depend on execution, storage, transaction, WAL, QUIC transport,
benchmark, analytics, GPU, or application-surface crates.

## Non-goals

- Do not lex or parse SRPL source text.
- Do not perform catalog name binding.
- Do not execute Procedure behavior or dispatch runtime adapters.
- Do not read or write storage, write WAL, publish catalog versions, or decide
  transaction visibility.
- Do not introduce application-facing ad hoc SQL.
- Do not represent dynamic table names, dynamic predicates, shape-shifting
  returns, or implicit null semantics in lowered output.
- Do not serialize Rust native structs directly to disk or network.
- Do not move behavior from `andromeda-srpl` in this scaffold.

## Prerequisites

Before changing this crate, understand:

- Lowering consumes already-bound semantic input.
- `andromeda-srpl-ir` owns semantic IR data shapes and validation.
- Lowering must preserve fixed Procedure contracts and bounded body operation
  semantics.
- The current source of behavior is still `crates/andromeda-srpl/src/lowering.rs`
  and `crates/andromeda-srpl/src/lowering/`.

## Procedure

1. Keep `src/lib.rs` limited to crate-level documentation until behavior moves.
2. Move lowering behavior only in a dedicated extraction change with facade
   compatibility tests.
3. Preserve typed diagnostics and validation behavior when introducing public
   lowering APIs.
4. Keep predicate, assignment, emit, and constant lowering closed and explicit.
5. Reject any design that depends on hidden runtime state or dynamic result
   shapes.

## Validation

For this scaffold, file-shape validation is sufficient because the crate is not
yet a workspace member:

```powershell
rg -n "forbid\\(unsafe_code\\)|Purpose|Scope|Non-goals|Dependency Direction" crates/andromeda-srpl-lowering
rg --files crates/andromeda-srpl-lowering
```

When this crate becomes a workspace member, add package-level Rust gates such as:

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
