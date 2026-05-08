# andromeda-retry

## Purpose

`andromeda-retry` is the future R3 owner for execution retry classification and idempotency evidence.

Retry behavior must classify errors after Procedure contract-first invocation and must never create duplicate visible outcomes. Retries depend on admission evidence, transaction terminal evidence, rollback fences, and idempotency policy. They do not decide storage truth.

This scaffold is not a promoted Cargo workspace member until a later packet adds a manifest, root workspace wiring, topology tests, and compatibility evidence.

## Scope

This future crate owns:

- Retry classification for timeout, deadlock, cancellation, transient resource pressure, unavailable handler, and terminal non-retryable errors.
- Idempotency evidence tied to Procedure contract identity, catalog version, invocation identity, and admitted route context.
- Retry budget and attempt accounting.
- Routing outcomes that distinguish retry, rollback, fail-fast, and terminal completion.
- Conflict rejection when durable evidence already proves a committed or rolled-back terminal state.

## Non-goals

This future crate does not own:

- Admission checks, Procedure runtime dispatch, transaction state authority, WAL byte formats, storage truth, or recovery replay.
- Business-specific retry policies, inventory-specific compensation, payment-specific behavior, or tenant-specific workflow branches.
- Application-facing SQL, raw command text, dynamic table names, dynamic predicates, or shape-shifting returns.
- ResultStream payload sequencing, QUIC transport behavior, or protocol byte codecs.
- Optimization, benchmark, GPU, trace, or temporary memory output as retry truth.

## Prerequisites

Before adding behavior here, confirm:

- The retry input is tied to an admitted, typed Procedure invocation.
- Idempotency policy is explicit and versioned.
- Durable terminal evidence is checked before allowing another attempt.
- Rollback and retry routing cannot produce both committed and rolled-back terminal outcomes.
- Retry budgets are bounded and observable.

## Procedure

1. Accept retry classification input from execution orchestration or Procedure runtime error routing.
2. Bind retry state to Procedure contract hash, catalog version, invocation identity, and admission evidence.
3. Reject retries when durable evidence already proves a terminal state.
4. Route timeout and deadlock outcomes through explicit rollback or retry decisions.
5. Decrement bounded retry budget and emit retry evidence.
6. Return a typed routing decision to execution orchestration without executing business logic.

## Validation

For the scaffold, validate that only README and `src/lib.rs` files were added under this directory.

Before promoting this crate into the workspace, add and run:

```powershell
cargo test -p andromeda-retry --tests
cargo test -p andromeda-cli --test workspace_dependency_topology -- --nocapture
cargo test -p andromeda-cli --test orphan_source_invariants -- --nocapture
```

Runtime promotion must include tests for retryable and non-retryable classification, idempotency evidence, durable terminal-state rejection, timeout and deadlock routing, retry-budget exhaustion, no application-facing SQL, and no business hardcoding.

## Troubleshooting

| Symptom | Check |
| --- | --- |
| A retry happens after a committed terminal state | Check durable terminal evidence before issuing another attempt. |
| Timeout handling creates conflicting outcomes | Route timeout through one rollback or retry decision with explicit evidence. |
| Retry policy depends on product-specific data | Move business policy outside the retry owner and keep retry classification generic. |
| Retry accepts raw command text | Bind retry state to typed Procedure contract and admitted invocation identity. |

## References

- `AGENTS.md`
- `crates/AGENTS.md`
- `crates/README.md`
- `crates/andromeda-exec/README.md`
- `docs/adr/ADR-0018-engine-crate-mapping-policy.md`
- `documentations/implementation/target-crate-gap-ledger-2026-05-08.md`
