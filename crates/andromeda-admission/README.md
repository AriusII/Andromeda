# andromeda-admission

## Purpose

`andromeda-admission` is the R3 owner for pre-transaction Procedure admission.

Admission decides whether a request may enter execution. It must complete before transaction creation and before Procedure runtime dispatch. The output of this crate is admission evidence, not business execution and not durable storage truth.

`andromeda-exec` keeps compatibility reexports while this crate owns the admission types, rejection evidence, permission evaluator trait, and pre-transaction contract validation.

## Scope

This future crate owns:

- Application Surface admission for typed, cataloged Procedure invocations.
- Caller identity, surface plane, permission, Procedure contract binding, catalog version, and contract hash checks.
- Resource budget and request-bound admission evidence.
- Rejection outcomes that prove no transaction was created.
- Handoff contracts for execution orchestration and ResultStream policy creation.

## Non-goals

This future crate does not own:

- Transaction state, WAL durability, MVCC visibility, storage truth, recovery replay, or catalog publication.
- Procedure body execution, SRPL interpretation, business validation, or handler dispatch.
- Application-facing SQL, raw command text, dynamic table names, dynamic predicates, or shape-shifting returns.
- Administration, backup, restore, HA/DR, or cluster control admission through the Application Surface.
- Business hardcoding or application-specific authorization shortcuts.

## Prerequisites

Before adding behavior here, confirm:

- Admission inputs are typed and include a Procedure contract reference.
- The caller surface is known before any permission or resource decision.
- Rejection paths can prove no transaction id, WAL append, or runtime handler execution occurred.
- Admission evidence can be correlated with execution trace records without becoming storage truth.

## Procedure

1. Decode or receive only a typed Procedure invocation envelope from a valid surface.
2. Resolve the Procedure contract binding and catalog version.
3. Validate contract hash, invocation identity, permission scope, and resource budget.
4. Reject Administration, HA/DR, backup, restore, or cluster capabilities on the Application Surface.
5. Emit an admission receipt only after all pre-transaction checks pass.
6. Emit a typed rejection with no transaction side effect when any check fails.

## Validation

Run the owner and compatibility gates:

```powershell
cargo check -p andromeda-admission --all-targets
cargo check -p andromeda-exec --all-targets
cargo test -p andromeda-cli --test workspace_dependency_topology -- --nocapture
cargo test -p andromeda-cli --test orphan_source_invariants -- --nocapture
```

Runtime changes must include tests proving admission before transaction, no transaction on denial, contract-hash and catalog-version rejection, wrong-surface rejection, resource-budget rejection, no application-facing SQL, and no business hardcoding.

## Troubleshooting

| Symptom | Check |
| --- | --- |
| A denied request has a transaction id | Admission is running too late; make the transaction boundary depend on an admission receipt. |
| A request bypasses Procedure contracts | Require contract hash and catalog version evidence in the admission input. |
| An operator capability enters through the Application Surface | Reject the surface before any runtime dispatch or transaction creation. |
| Business-specific conditions appear in admission | Move them to cataloged Procedure logic or a typed policy owner, not admission. |

## References

- `AGENTS.md`
- `crates/AGENTS.md`
- `crates/README.md`
- `crates/andromeda-exec/README.md`
- `docs/adr/ADR-0002-WORKSPACE_AND_CRATE_BOUNDARIES.md`
- `docs/status.md`
