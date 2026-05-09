# andromeda-procedure-runtime

## Purpose

`andromeda-procedure-runtime` is the R3 owner for generic Procedure runtime dispatch after admission.

The runtime invokes cataloged Procedure implementations through typed contracts. It must not become an application-specific business module, a raw SQL tunnel, or a bypass around admission, transaction, WAL, catalog, or security boundaries.

`andromeda-exec` keeps compatibility reexports while this crate owns generic dispatch evidence, dispatch request validation, remote-unavailable errors, and the generic SRPL-to-local dispatch adapter.

## Scope

This future crate owns:

- Generic Procedure handler invocation after an admission receipt exists.
- Runtime dispatch contracts that require Procedure contract hash, catalog version, invocation identity, and typed input shape.
- Handler lifecycle boundaries for begin, execute, rollback, terminal result mapping, and cancellation handoff.
- Typed runtime errors for unavailable handlers, contract drift, handler failure, cancellation, and rollback routing.
- SRPL adapter handoff points that remain Procedure contract-first.

## Non-goals

This future crate does not own:

- Admission checks, caller permission policy, or surface authorization.
- Transaction state authority, WAL byte formats, storage truth, MVCC visibility, or recovery replay.
- ResultStream frame wire contracts, concrete QUIC transport, or Protobuf schema generation.
- Business hardcoding for particular products, tenants, inventories, workflows, or policies.
- Application-facing SQL, dynamic table names, dynamic predicates, raw command text, or shape-shifting returns.

## Prerequisites

Before adding behavior here, confirm:

- A valid admission receipt exists before runtime dispatch.
- The Procedure contract binding is complete and versioned.
- Handler selection is by typed Procedure identity, not by dynamic command text.
- Runtime errors can be routed to retry, rollback, or terminal completion without creating conflicting outcomes.
- No handler path can publish committed state before durable WAL evidence exists.

## Procedure

1. Accept only admitted Procedure invocation work.
2. Resolve the handler through a typed Procedure identity and contract binding.
3. Validate runtime input shape against the Procedure contract before execution.
4. Execute the handler through generic runtime traits, not business-specific branches.
5. Return typed runtime evidence to execution orchestration.
6. Hand terminal errors to retry or rollback routing with enough evidence to avoid duplicate terminal states.

## Validation

Run the owner and compatibility gates:

```powershell
cargo check -p andromeda-procedure-runtime --all-targets
cargo check -p andromeda-exec --all-targets
cargo test -p andromeda-cli --test workspace_dependency_topology -- --nocapture
cargo test -p andromeda-cli --test orphan_source_invariants -- --nocapture
```

Runtime changes must include tests for contract-first invocation, handler lookup rejection, input-shape rejection, no application-facing SQL, no business hardcoding, cancellation routing, rollback routing, and durable commit evidence handoff.

## Troubleshooting

| Symptom | Check |
| --- | --- |
| A handler runs without an admission receipt | Move the dispatch entry point behind admission evidence. |
| Handler lookup depends on text commands | Replace it with typed Procedure identity and contract binding. |
| Product-specific code appears in runtime dispatch | Move the behavior outside the runtime owner and keep the runtime generic. |
| A runtime failure creates ambiguous terminal state | Route through retry or rollback evidence before completion mapping. |

## References

- `AGENTS.md`
- `crates/AGENTS.md`
- `crates/README.md`
- `crates/andromeda-exec/README.md`
- `docs/adr/ADR-0018-engine-crate-mapping-policy.md`
- `docs/implementation/extraction-status.md`
