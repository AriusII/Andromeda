# andromeda-execution-trace

## Purpose

`andromeda-execution-trace` is the future R3 owner for execution trace correlation.

Execution traces explain Procedure admission, dispatch, transaction, ResultStream, retry, and terminal outcomes. They are evidence for review and audit correlation. They are not storage truth, commit authority, retry authority, or a replacement for durable WAL evidence.

This scaffold is not a promoted Cargo workspace member until a later packet adds a manifest, root workspace wiring, topology tests, and compatibility evidence.

## Scope

This future crate owns:

- Execution trace event taxonomy for admitted Procedure invocations.
- Correlation identifiers that connect admission, Procedure runtime, transaction evidence, ResultStream events, retry decisions, and terminal completion.
- Trace-safe summaries of contract hash, catalog version, invocation identity, surface, permission evidence, and terminal state.
- Rejection and failure trace evidence that proves whether transaction creation occurred.
- Redaction and bounded trace payload rules for execution evidence.

## Non-goals

This future crate does not own:

- Durable audit journal storage, WAL records, transaction state, recovery replay, or storage truth.
- Admission decisions, Procedure runtime dispatch, retry decisions, or ResultStream sequencing.
- Application-facing SQL, raw command text, dynamic predicates, or shape-shifting returns.
- Business hardcoding or application-specific trace schemas.
- Commit, rollback, catalog publication, authorization, or HA/DR authority.

## Prerequisites

Before adding behavior here, confirm:

- Trace events are derived from typed Procedure contracts and admitted route context.
- Trace output cannot decide commit, rollback, recovery, authorization, or retry.
- Sensitive payload fields are redacted or omitted by default.
- Every terminal trace can be correlated with durable terminal evidence when a transaction exists.
- Rejection traces can prove no transaction was created when admission fails.

## Procedure

1. Start a trace only from a typed invocation context or admission rejection.
2. Record contract hash, catalog version, invocation identity, surface, and permission evidence as bounded fields.
3. Correlate Procedure runtime, transaction, ResultStream, retry, and terminal events with stable identifiers.
4. Keep payload data out of trace fields unless an explicit redaction rule admits it.
5. Mark traces as explanatory evidence, not database truth.
6. Require durable terminal evidence references for committed or rolled-back transaction outcomes.

## Validation

For the scaffold, validate that only README and `src/lib.rs` files were added under this directory.

Before promoting this crate into the workspace, add and run:

```powershell
cargo test -p andromeda-execution-trace --tests
cargo test -p andromeda-cli --test workspace_dependency_topology -- --nocapture
cargo test -p andromeda-cli --test orphan_source_invariants -- --nocapture
```

Runtime promotion must include tests for admission rejection traces, no transaction on denial, terminal correlation, redaction, bounded payload fields, metadata-before-payload trace ordering, no application-facing SQL, and no business hardcoding.

## Troubleshooting

| Symptom | Check |
| --- | --- |
| Trace output is used as commit truth | Replace the decision source with transaction and WAL durability evidence. |
| A trace contains raw command text | Store typed Procedure identity and contract evidence instead. |
| A denial trace has transaction evidence | Ensure admission rejection happens before transaction creation. |
| Trace fields contain business-specific payloads | Apply redaction and keep trace schemas engine-generic. |

## References

- `AGENTS.md`
- `crates/AGENTS.md`
- `crates/README.md`
- `crates/andromeda-exec/README.md`
- `crates/andromeda-observe/README.md`
- `docs/adr/ADR-0018-engine-crate-mapping-policy.md`
- `documentations/implementation/target-crate-gap-ledger-2026-05-08.md`
