# andromeda-srpl

## Purpose

`andromeda-srpl` owns the narrow SRPL compiler orchestration surface.

Use this crate when callers need to compile SRPL source into typed IR or
Procedure contract materialization. Use the dedicated owner crates directly for
lexing, parsing, binding, catalog binding, DefinitionBatch dry-run,
optimization internals, execution adapters, interpreter behavior, AST,
diagnostics, cardinality, and IR data shapes.

SRPL is not an ad hoc SQL surface. SRPL source must compile into typed,
cataloged Procedure contracts with bounded shapes before it can participate in
catalog publication or execution.

## Scope

This crate is responsible for:

| Area | Responsibility |
| --- | --- |
| Compiler orchestration | Source validation, parser/binder/lowering coordination, optimizer entry points, and Procedure contract materialization. |
| Inventory contract fixture | Canonical `Inventory.ReserveStock` source and metadata helpers used by tests and the inventory vertical slice. |
| DefinitionBatch handoff | Raw single-operation helpers for tests; source-evidence dry-run lives in `andromeda-srpl-definition-batch`. |

The crate may currently depend on catalog-facing code for Procedure contract
materialization. Extracted language-model crates remain the owner for
parser-independent diagnostics, cardinality, AST, parser, binder, lowering, and
IR data shapes.

## Non-goals

- Do not introduce application-facing ad hoc SQL.
- Do not accept dynamic table names, dynamic predicates, shape-shifting returns,
  or implicit null semantics in SRPL core work.
- Do not bypass typed, cataloged Procedure contracts.
- Do not make catalog changes visible before durable WAL through SRPL compiler
  code.
- Do not treat optimizer output, interpreter output, RAM, temp storage,
  benchmark output, or GPU output as durable truth.
- Do not place GPU work in SRPL compile, commit, WAL, rollback, recovery, MVCC
  short-visibility, catalog publication, or security-critical paths.
- Do not serialize Rust native structs directly to disk or network.
- Do not move general catalog storage, transaction, WAL, recovery, transport,
  IAM runtime, benchmark, or analytics ownership into this crate.

## Prerequisites

Before changing this crate, understand:

- Application behavior is exposed through typed, cataloged Procedures.
- SRPL source must lower to deterministic Procedure contracts with explicit
  input and result shapes.
- Result streams must use explicit cardinality. Absence is modeled through
  explicit type policy, not implicit null behavior.
- Optimizer decisions must remain bounded, observable, explainable, versioned,
  and disableable.
- DefinitionBatch dry-run paths are side-effect free until catalog code owns a
  durable apply path with WAL callbacks.
- Prefer moving parser-independent language-model behavior into
  `andromeda-srpl-*` owner crates.

## Procedure

1. Choose the owner crate that already owns the behavior: lexer, parser, AST,
   binder, lowering, diagnostics, IR, catalog binding, DefinitionBatch bridge,
   optimizer, execution adapter, interpreter, or this compiler orchestration
   crate.
2. Keep `lib.rs` limited to intentional compiler entry points.
3. Validate source text through `SrplSource` and parser diagnostics before
   binding or lowering.
4. Lower only to typed IR and Procedure contracts. Keep Procedure names,
   targets, predicates, result streams, row bounds, and failure codes explicit.
5. Reject dynamic table names, dynamic predicates, shape-shifting returns, and
   implicit null semantics as language-design failures, not runtime surprises.
6. Keep optimizer passes deterministic and diagnostic-rich. Add tests for every
   new rewrite, fold, or cost choice.
7. Keep DefinitionBatch integration side-effect free during dry-run and preserve
   source digest evidence for materialized Procedure contracts.

## Validation

For documentation-only changes, validate this README against:

- `AGENTS.md`
- `crates/AGENTS.md`
- `crates/README.md`
- `docs/adr/ADR-0011-workspace-crate-boundaries.md`

For code changes in this crate, prefer:

```bash
cargo fmt --package andromeda-srpl --check
cargo test -p andromeda-srpl --test api_compat_reexports
cargo test -p andromeda-srpl --test compiler_pipeline_e2e
cargo test -p andromeda-srpl --test definitionbatch_compat
cargo check -p andromeda-srpl-definition-batch -p andromeda-srpl-catalog-binding --all-targets
cargo test -p andromeda-optimizer --test srpl_optimizer_pipeline_contract
cargo test -p andromeda-srpl --test validation_gates
cargo check -p andromeda-srpl --all-targets
```

If a change affects catalog publication, Procedure contract hashes, durable
apply, recovery, security, or execution behavior, add the appropriate catalog,
WAL, crash/recovery, property, fuzz, or security validation.

## Troubleshooting

| Symptom | Corrective action |
| --- | --- |
| A caller wants to pass SQL text through SRPL. | Replace it with typed SRPL source that lowers to a Procedure contract, or reject the request if it requires ad hoc SQL. |
| A change needs dynamic table selection or dynamic predicate text. | Move the choice into cataloged Procedure definitions or typed parameters with fixed predicate forms. Do not add runtime text construction. |
| A result shape depends on runtime data. | Declare a fixed result stream contract with explicit cardinality and columns. |
| A nullable value is introduced implicitly. | Use the project type system and explicit absence policy. Do not add a hidden `Null` value path. |
| A new feature belongs to an extracted language-model crate. | Implement it in the owner crate and import that crate directly. |
| Optimizer output changes a Procedure contract. | Treat that as a contract bug. Optimizer passes may change plan form, not the typed Procedure contract shape. |

## References

- `AGENTS.md`
- `crates/AGENTS.md`
- `crates/README.md`
- `docs/adr/ADR-0011-workspace-crate-boundaries.md`
- `crates/andromeda-srpl/src/lib.rs`
- `crates/andromeda-srpl-definition-batch/src/lib.rs`
- `crates/andromeda-srpl-catalog-binding/src/lib.rs`
- `crates/andromeda-srpl-execution-adapter/src/lib.rs`
- `crates/andromeda-optimizer/src/srpl/mod.rs`
