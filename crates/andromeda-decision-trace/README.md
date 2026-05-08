# andromeda-decision-trace

## Purpose

`andromeda-decision-trace` owns runtime-free decision trace contracts used by optimizer, statistics, plan cache, benchmark evidence, analytics, and operator explanation paths.

The crate exposes bounded v0 trace identities, reason codes, evidence digests, version bindings, and adaptive-control disablement state. Runtime event sinks and durable audit export remain outside this crate.

## Scope

This crate is expected to own:

- Runtime-free decision trace identities and stable event payload contracts.
- Trace fields for candidates considered, candidates rejected, evidence used, evidence ignored, fallback reasons, and disablement state.
- Version bindings for statistics, plans, procedures, policies, and scenario evidence.
- Redaction-safe trace vocabulary for operator explanation and post-fact review.
- Compatibility boundaries with the observability crate that persists or exports traces.

## Non-goals

- Do not make trace records the source of storage, catalog, WAL, transaction, recovery, or security truth.
- Do not put trace export or durable audit work in the commit-critical path.
- Do not log secrets, raw credentials, unbounded payloads, or native Rust layouts.
- Do not let benchmark output, ScenarioEvidence, or GPU output become authoritative because it appears in a trace.
- Do not expose traces as an application-facing ad hoc query surface.

## Prerequisites

- Keep persistent trace emission and exporter behavior in `andromeda-observe` until a registered extraction work order moves runtime integrations.
- Preserve audit-safe redaction and bounded payload rules.
- Keep trace emission explanatory, not authoritative.

## Procedure

1. Define runtime-free trace contracts before moving persistent or exporter behavior.
2. Separate decision contracts from sinks, journals, and exporters.
3. Bind adaptive decisions to complete statistics, plan, catalog, contract, and policy versions.
4. Represent benchmark, ScenarioEvidence, GPU, and analytics inputs as advisory evidence only.
5. Keep current observability compatibility imports until callers migrate.

## Validation

Future behavior changes should use:

```powershell
cargo test -p andromeda-decision-trace
cargo test -p andromeda-observe --test audit_family_contract -- --nocapture
cargo test -p andromeda-cli --test workspace_dependency_topology -- --nocapture
```

Use this crate for runtime-free contract validation; use observability crates for sinks and retained audit records.

## Troubleshooting

- If a decision cannot explain stale or ignored evidence, add an explicit trace status.
- If trace data is treated as correctness proof, move the proof to the owning runtime or recovery gate.
- If trace output includes sensitive values, fix the producing contract before adding exporter-only redaction.

## References

- [Workspace crate rules](../README.md)
- [Current observability owner](../andromeda-observe/README.md)
- [Current benchmark owner](../andromeda-bench/README.md)
