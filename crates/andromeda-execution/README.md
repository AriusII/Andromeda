# andromeda-execution

## Purpose

`andromeda-execution` is the future R3 owner for admitted Procedure execution orchestration.

This scaffold records the split boundary currently held by `andromeda-exec`. The required order is admission before transaction, Procedure contract-first invocation, and ResultStream metadata-before-payload emission. The scaffold is not a promoted Cargo workspace member until a later packet adds a manifest, root workspace wiring, topology tests, and compatibility evidence.

## Scope

This future crate owns:

- Orchestration after admission proves the caller, surface, Procedure contract binding, catalog version, permissions, and resource budget.
- Transaction boundary coordination after admission succeeds, without owning transaction state authority.
- Dispatch to the Procedure runtime through typed, cataloged Procedure contracts.
- ResultStream sequencing coordination, including metadata before payload and terminal completion evidence.
- Retry routing handoff for timeout, deadlock, rollback, and idempotency decisions.
- Execution trace correlation across admission, dispatch, transaction, result streaming, and retry outcomes.

## Non-goals

This future crate does not own:

- Application-facing SQL, raw command text, dynamic table names, dynamic predicates, or shape-shifting returns.
- Business hardcoding for inventory, ordering, billing, tenant policy, or any application-specific workflow.
- Procedure contract definitions, catalog storage, Procedure Store records, WAL byte formats, storage truth, MVCC authority, or recovery replay.
- Administration, backup, restore, HA/DR, cluster control, or operator-only capabilities through the Application Surface.
- GPU, benchmark, analytics, trace, or temporary runtime output as execution truth.

## Prerequisites

Before adding behavior here, confirm:

- The split packet has an approved manifest and workspace topology plan.
- `andromeda-exec` remains the compatibility owner until direct owner tests and old-import compatibility tests pass.
- Every invocation path starts from a typed Procedure contract and an admission receipt.
- No transaction is created before admission succeeds.
- Durable WAL evidence is available before a committed outcome becomes visible.

## Procedure

1. Start from an admitted Procedure invocation receipt.
2. Verify the receipt binds a Procedure contract hash, catalog version, invocation identity, caller surface, permission evidence, and resource budget.
3. Create transaction work only after admission succeeds.
4. Dispatch through the Procedure runtime without adding business-specific branches.
5. Emit ResultStream metadata before any payload batch.
6. Route timeout, deadlock, rollback, and retry decisions through the retry owner.
7. Correlate terminal execution evidence through the execution trace owner.

## Validation

For the scaffold, validate that only README and `src/lib.rs` files were added under this directory.

Before promoting this crate into the workspace, add and run:

```powershell
cargo test -p andromeda-execution --tests
cargo test -p andromeda-cli --test workspace_dependency_topology -- --nocapture
cargo test -p andromeda-cli --test orphan_source_invariants -- --nocapture
```

Runtime promotion must include tests for admission-before-transaction, Procedure contract-first dispatch, ResultStream metadata-before-payload sequencing, no application-facing SQL, no business hardcoding, retry terminal-state consistency, and durable commit evidence.

## Troubleshooting

| Symptom | Check |
| --- | --- |
| A transaction exists for a rejected request | Move the check into admission and require an admission receipt before transaction creation. |
| A dispatch path accepts raw command text | Replace it with a typed Procedure contract binding and catalog version evidence. |
| A payload appears before metadata | Route through the ResultStream owner and reject the sequence before emission. |
| Application-specific branching appears in orchestration | Move the behavior to a cataloged Procedure implementation, not the engine orchestrator. |

## References

- `AGENTS.md`
- `crates/AGENTS.md`
- `crates/README.md`
- `crates/andromeda-exec/README.md`
- `docs/adr/ADR-0002-WORKSPACE_AND_CRATE_BOUNDARIES.md`
- `docs/status.md`
