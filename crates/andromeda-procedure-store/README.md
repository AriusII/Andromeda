# andromeda-procedure-store

## Purpose

`andromeda-procedure-store` defines runtime-free, evidence-only Procedure Store primitives.

Use this crate when a component needs typed invocation identities, invocation status labels, evidence markers, advisory feedback, bounded metrics, regression signals, or audit correlation values for Procedure invocation history.

## Scope

This crate owns:

- Procedure and invocation identity primitives.
- Invocation status and terminal-state classification.
- Evidence digests, evidence kinds, and evidence markers.
- Invocation history records, advisory feedback, invocation metrics, and regression signals.
- Audit correlation values that bind Procedure evidence to audit and decision-trace evidence.
- The `InvocationEvidenceSink` trait surface for recording status and evidence through an owning implementation.

The crate describes evidence shapes. It does not provide durable Procedure Store storage by itself.

## Non-goals

- Do not treat Procedure Store evidence as storage truth, catalog truth, optimizer authority, or plan-selection authority.
- Do not own WAL replay, catalog publication, durable history storage, audit-ledger persistence, Procedure execution, or rollback behavior.
- Do not make advisory feedback or regression signals automatically change execution plans.
- Do not add application-facing ad hoc SQL or bypass typed Procedure contracts.
- Do not claim runtime or release readiness from these primitives alone.

## Ownership

`andromeda-procedure-store` owns invocation evidence primitives and their local validation rules.

Execution, catalog, WAL, storage, observability, and audit owners remain responsible for durable persistence, recovery behavior, publication order, trace retention, and operator-facing interpretation. Sink implementations must live in an owning runtime or durable component, not in this primitive crate.

## Validation

For documentation-only changes, check that this README keeps the required headings and preserves the evidence-only boundary.

For source changes in this crate, prefer:

```powershell
cargo test -p andromeda-procedure-store
```

Run focused evidence contract tests when changing evidence, feedback, metrics, regression, or audit correlation semantics.

## References

- `Cargo.toml`
- `src/lib.rs`
- `src/history.rs`
- `src/evidence.rs`
- `src/feedback.rs`
- `src/metrics.rs`
- `src/regression.rs`
- `src/audit.rs`
- `tests/evidence_contract.rs`
- `../README.md`
- `../../AGENTS.md`
