# andromeda-srpl-execution-adapter

## Purpose

`andromeda-srpl-execution-adapter` owns runtime-free SRPL
execution adapter contracts.

Use this crate as the dedicated boundary between catalog-bound SRPL plans and
concrete execution orchestration. `andromeda-srpl` keeps compatibility reexports
and must not own adapter behavior.

## Scope

This crate is intended to own:

| Area | Responsibility |
| --- | --- |
| Adapter requests | Typed, bounded read, update, assert, emit, and failure requests derived from catalog-bound SRPL plans. |
| Adapter results | Typed result shapes and cardinality failures returned to the SRPL interpreter or execution orchestration. |
| Binding environment | Runtime-free bound value and row shapes needed by adapter traits. |
| Adapter traits | Narrow interfaces that concrete execution owners can implement without pulling SRPL parser or compatibility-surface code into runtime crates. |

The execution adapter is a contract boundary. It defines what SRPL execution
needs, but it must not own execution scheduling, transaction commit, storage
truth, WAL durability, transport sessions, or catalog publication.

## Dependency Direction

Dependencies must point only toward lower or contract-safe crates such as
`andromeda-error`, `andromeda-types`,
`andromeda-procedure-contract`, `andromeda-srpl-diagnostics`,
`andromeda-srpl-cardinality`, and `andromeda-srpl-ir`.

Do not depend on `andromeda-srpl` as a broad compatibility surface from this crate after extraction.
Do not depend on parser, binder, lowering, optimizer, storage, transaction,
WAL, QUIC transport, benchmark, analytics, GPU, or application-surface crates.
Concrete execution crates may depend on this adapter boundary, but this adapter
boundary must not depend back on concrete execution runtime crates.

## Non-goals

- Do not lex, parse, bind, lower, optimize, or interpret SRPL source.
- Do not execute Procedure behavior or own execution scheduling.
- Do not read or write storage, write WAL, publish catalog versions, or decide
  transaction visibility.
- Do not expose Administration or HA/DR behavior through the Application
  Surface.
- Do not introduce application-facing ad hoc SQL.
- Do not treat adapter results, traces, RAM, or benchmark output as durable
  truth.
- Do not serialize Rust native structs directly to disk or network.
- Do not depend on `andromeda-srpl` as a broad compatibility surface.

## Prerequisites

Before changing this crate, understand:

- Adapter contracts are typed boundaries, not runtime execution engines.
- Visible commit and recovery truth remain owned by WAL, transaction, storage,
  and catalog publication layers.
- SRPL result cardinality and row bounds must stay explicit at adapter
  boundaries.
- The current source of behavior is still
  `crates/andromeda-srpl/src/execution_adapter.rs` and
  `crates/andromeda-srpl/src/execution_adapter/`.

## Procedure

1. Keep adapter APIs typed, bounded, and runtime-free.
2. Preserve row-bound and cardinality validation when introducing public
   request and result types.
3. Reject any design that lets adapter output decide durable truth or bypass
   typed Procedure contracts.

## Validation

```powershell
cargo fmt --package andromeda-srpl-execution-adapter --check
cargo check -p andromeda-srpl-execution-adapter --all-targets
cargo test -p andromeda-srpl-execution-adapter
```

## Troubleshooting

| Symptom | Corrective action |
| --- | --- |
| The adapter needs to commit, rollback, or flush WAL. | Move that behavior to the transaction, WAL, or storage owner; keep this crate as a typed request/result boundary. |
| The adapter needs transport session state. | Keep transport behavior in RPC or QUIC runtime crates. |
| Adapter results become durable truth. | Reject the design; durable truth comes from catalog, WAL, storage, and transaction evidence. |
| Concrete execution code must import parser or compatibility-surface internals. | Move the shared typed contract into this adapter boundary or a lower language-model crate. |

## References

- `AGENTS.md`
- `crates/AGENTS.md`
- `crates/README.md`
- `docs/adr/ADR-0002-WORKSPACE_AND_CRATE_BOUNDARIES.md`
- `docs/adr/ADR-0002-WORKSPACE_AND_CRATE_BOUNDARIES.md`
- `crates/andromeda-srpl/src/execution_adapter.rs`
- `crates/andromeda-srpl/src/execution_adapter/`
