---
name: procedure-store-evidence
description: "Use for Procedure Store, ScenarioEvidence and feedback loops."
category: andromeda-optimizer
---

# procedure-store-evidence

## When to use
Work touches observed execution history or predictive evidence.

## Purpose
Use for Procedure Store, ScenarioEvidence and feedback loops.

## Process
- Procedure Store observes; it does not decide alone.
- ScenarioEvidence is non-authoritative, versioned and expirable.
- Separate confidence, criticality, freshness, realism and stability scores.
- Correlate with real invocation metrics.

## Expected output
- Evidence lifecycle plan.

## Guardrails
- No pseudo-truth aggregate score.

## Andromeda baseline
- Rust 1.95.0 / Edition 2024 / resolver 3.
- QUIC + custom Protobuf RPC; no gRPC and no JSON native protocol.
- Procedure-only application surface and SRPL contracts.
- WAL before visible commit; recovery and audit are mission-critical.
- Worker artifacts belong under `.work/codex/<task-slug>/`.
