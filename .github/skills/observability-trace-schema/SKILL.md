---
name: observability-trace-schema
description: Applies trace schema, metrics, and alert rules when prompts mention observability, traces, metrics, alerts, telemetry, or operational evidence.
license: MIT
---

# observability-trace-schema

## When to use
- The prompt mentions observability, trace, telemetry, span, metric, alert, dashboard, DecisionTrace, audit correlation, or operations signal.
- A change adds or modifies instrumentation in execution, WAL, RPC, catalog, optimizer, storage, or security paths.
- A review asks whether a signal is useful, stable, or safe.

## Purpose
Make operational behavior observable without weakening correctness or leaking sensitive information. The skill aligns traces, metrics, and alerts with Andromeda's documented trace schema and operations guidance so incidents can be diagnosed from durable evidence.

## Process
1. Read observability architecture, operations, metrics/alerts, and trace schema ADR.
2. Identify the event boundary and correlation IDs: request, procedure, transaction, WAL, catalog, principal, plan, or recovery.
3. Emit structured low-cardinality metrics and stable trace fields; avoid secrets and unbounded labels.
4. Tie alerts to actionable runbooks and severity, not noisy implementation internals.
5. Add tests or assertions for trace schema, metric naming, and correlation when the codebase has existing validation.

## Expected output
- A trace/metric/alert plan naming fields, labels, cardinality, and correlation IDs.
- Runbook mapping for actionable alerts.
- Privacy and stability risks for any new signal.

## Reference docs
- `docs/architecture/OBSERVABILITY_ARCHITECTURE.md`
- `docs/operations/OBSERVABILITY_OPERATIONS.md`
- `docs/operations/METRICS_AND_ALERTS.md`
- `docs/adr/ADR-0008-OBSERVABILITY_TRACE_SCHEMA.md`

## Guardrails
- Do not log secrets, certificate material, or sensitive payloads.
- Do not create high-cardinality labels from user input.
- Do not substitute logs for durability or audit evidence.
- When doctrine is implicated, cite `docs/project/ANDROMEDA_DOCTRINE.md` and treat conflicts as blockers.

## Andromeda baseline
- Rust 1.95.0 / Edition 2024 / resolver 3.
- QUIC + custom Protobuf RPC.
- no gRPC.
- no JSON native protocol.
- procedure-only application surface through typed, cataloged, versioned Procedure contracts.
- WAL-before-visible-commit with recovery evidence.
- GPU and accelerated paths outside commit, rollback, recovery, MVCC visibility, security admission, and authorization paths.
