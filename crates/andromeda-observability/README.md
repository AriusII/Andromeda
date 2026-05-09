# andromeda-observability

## Purpose

`andromeda-observability` owns shared observability identifiers, correlation metadata, and reusable critical-decision vocabulary used by trace, audit, query, and durable journal code.

## Scope

- Own stable trace and event identifiers.
- Own event schema version constants.
- Own reusable critical-decision kinds and their runtime-free projection shape.
- Own request, session, catalog, transaction, durable LSN, and protocol correlation metadata.
- Provide small value objects that can be reused without depending on the `andromeda-observe` runtime.

## Non-goals

- Do not add instrumentation, exporters, metrics collection, tracing runtime, durable audit journals, sinks, retention, or replay behavior.
- Do not make correlation metadata the source of catalog, transaction, storage, WAL, recovery, or security truth.
- Do not introduce application-facing ad hoc SQL or runtime protocol serialization.

## Prerequisites

- Workspace crate conventions in `crates/AGENTS.md` apply.
- Consumers must validate event envelopes or domain-specific trace records at their own boundary.

## Procedure

1. Use `TraceId` and `EventId` for typed correlation instead of primitive aliases.
2. Carry `EventCorrelation` when a trace must explain request/session, catalog, transaction, LSN, or protocol context.
3. Keep runtime emission, durable persistence, and query logic in higher-level crates.
4. Add only cross-cutting observability value objects here.

## Validation

- Inspect `src/lib.rs` and identifier modules for value-object-only behavior.
- When validating by command, use `cargo check -p andromeda-observability --all-targets`.

## Troubleshooting

- If a consumer needs `EventEnvelope`, depend on `andromeda-observe` instead.
- If a consumer needs security/admin/admission audit records, depend on `andromeda-audit` instead.
- If correlation values are treated as authoritative state, move that logic back to the owning runtime subsystem.

## References

- `AGENTS.md`
- `crates/AGENTS.md`
- `crates/andromeda-observe/README.md`
