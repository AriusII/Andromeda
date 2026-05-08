# andromeda-observe

## Purpose

`andromeda-observe` owns trace envelopes, runtime emission helpers, durable audit journal contracts, query models, exporters, and post-fact decision explainability for Andromeda.

Observability records are bounded and audit-safe. They support review, replay, correlation, and forensic explanation, but they must not become storage truth or the transaction commit path. Shared identifiers live in `andromeda-observability`; typed audit trace contracts live in `andromeda-audit` and are reexported here for compatibility.

## Scope

This crate provides:

- Event envelopes, lifecycle sequencing, validation, and in-memory sinks.
- Decision, protocol, placement, durability, transition, and core trace event families.
- Compatibility reexports for shared observability identifiers and typed audit traces.
- Durable audit journal records, checksum chaining, replay evidence, retention policy, compaction reports, and file-backed audit sink contracts.
- Bounded trace query specifications, filters, result metadata, and in-memory query sources.
- Exporter contracts and mock exporters for tests.
- Principal binding evidence that supports audit review without expanding runtime authority.

## Non-goals

- Do not make trace records the source of storage, catalog, transaction, or security truth.
- Do not put observability emission, audit compaction, or exporter work in the commit-critical path.
- Do not log secrets, raw credentials, or unbounded payloads.
- Do not turn diagnostic JSON or exporter output into the runtime protocol.
- Do not let retention compaction erase the evidence required to explain retained audit chains.
- Do not move durable audit journal truth into `andromeda-audit`; that crate owns trace contracts only.

## Prerequisites

- Use typed event families and envelopes instead of free-form strings for critical audit evidence.
- Set explicit query limits and filters for operator-facing trace inspection.
- Preserve checksum-chain and replay evidence when using durable audit journals.
- Redact or exclude sensitive values before trace output reaches CLI, exporter, or test fixtures.

## Procedure

1. Create typed trace or audit records at the owning subsystem boundary.
2. Attach correlation and principal binding evidence needed for post-fact review.
3. Validate envelopes before emission.
4. Use bounded query specifications when reading traces or durable audit records.
5. Preserve replay and compaction evidence when rewriting retained audit records.
6. Keep exporter output diagnostic and downstream-facing, not authoritative runtime state.

## Validation

For durable audit and query changes, prefer:

```powershell
cargo test -p andromeda-observe --test durable_audit_sink_contract -- --nocapture
cargo test -p andromeda-observe --test durable_audit_query_contract -- --nocapture
cargo test -p andromeda-observe --test durable_audit_retention_contract -- --nocapture
```

For event-family and operator-surface contracts, use:

```powershell
cargo test -p andromeda-observe --test audit_family_contract -- --nocapture
cargo test -p andromeda-observe --test admission_audit_contract -- --nocapture
cargo test -p andromeda-observe --test hadr_backup_audit_contract -- --nocapture
cargo test -p andromeda-observe --test protocol_correlation_contract -- --nocapture
```

Before accepting source changes, use the broader workspace gates listed in `crates/README.md`.

## Troubleshooting

- If a query returns too many records, reduce the limit, add an LSN range, or filter by trace id, principal, or event family.
- If durable replay fails, inspect checksum-chain and truncation evidence before trusting partial output.
- If compaction changes retained records, verify that rethreaded chain evidence and compaction anchors remain available.
- If trace output contains sensitive values, fix the producing event or exporter path before adding downstream redaction-only workarounds.

## References

- [Workspace crate rules](../README.md)
- [`src/lib.rs`](src/lib.rs)
- [`src/events`](src/events)
- [`src/events/durable_audit`](src/events/durable_audit)
- [`src/query`](src/query)
- [`../andromeda-observability`](../andromeda-observability)
- [`../andromeda-audit`](../andromeda-audit)
