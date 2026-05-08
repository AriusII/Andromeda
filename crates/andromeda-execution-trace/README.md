# andromeda-execution-trace

## Purpose

`andromeda-execution-trace` owns execution trace events and in-memory trace ledger test support outside `andromeda-exec`.

## Non-goals

- Durable storage design or append protocol ownership.
- Retry policy or timeout policy enforcement.
- Permission model definitions and transaction commit authority.
- Exporter formatting or long-term query analytics.

## Scope

The crate stores invocation lifecycle evidence for admission, dispatch, execution, failure, and timeout events. Trace data is explanatory evidence only, not commit authority, retry authority, or storage truth.

## Prerequisites

- Trace event sources must already be validated by caller-owned contracts before emission.
- Event producers must provide an invocable `trace_id` and `invocation_id`.
- Implementations must preserve append semantics for forensic continuity and never drop terminal lifecycle events silently.

## Procedure

1. Add new lifecycle events only when a durable subsystem boundary needs extra forensic visibility.
2. Keep trace events descriptive and stable (`trace_id`, `invocation_id`, terminal state, reason).
3. Route every emitted event through `AuditLedger` to keep observability behavior testable and consistent.
4. Keep test ledgers in `InMemoryAuditLedger`; do not use production persistence contracts as test fallback unless explicitly required.
5. Add or adjust focused unit tests when event schema or query behavior changes.

## Validation

```powershell
cargo test -p andromeda-execution-trace
```

## Troubleshooting

- Event is missing from traces: verify the caller emits every lifecycle boundary and that query filters include both `trace_id` and `invocation_id`.
- Events do not appear in deterministic order: verify append order and serialization boundaries in the emitting integration point.
- Recovery review is inconclusive: validate `TimeoutExceeded` and final event emission ordering around terminal states.

## References

- [`Cargo.toml`](Cargo.toml)
- [`src/lib.rs`](src/lib.rs)
- [`../andromeda-exec`](../andromeda-exec)
