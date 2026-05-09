---
name: task-scope-bounding
description: "Use to transform broad user requests into precise bounded Codex work orders."
category: codex-orchestration
---

# task-scope-bounding

## When to use
The user request is broad, ambiguous, multi-crate, or mission-critical.

## Purpose
Use to transform broad user requests into precise bounded Codex work orders.

## Process
- Extract explicit goal, non-goals, target paths, risk level, and expected artifacts.
- Classify as analysis-only, refactor, implementation, roadmap, architecture, or verification.
- Define allowed paths, forbidden paths, and stop conditions.
- List assumptions and ask no unnecessary questions if a safe partial plan is possible.

## Expected output
- Bounded scope, exclusions, risk class, required reports, acceptance criteria.

## Guardrails
- Do not widen scope. Do not silently convert analysis into implementation.

## Andromeda baseline
- Rust 1.95.0 / Edition 2024 / resolver 3.
- QUIC + custom Protobuf RPC; no gRPC and no JSON native protocol.
- Procedure-only application surface and SRPL contracts.
- WAL before visible commit; recovery and audit are mission-critical.
- Worker artifacts belong under `.work/codex/<task-slug>/`.
