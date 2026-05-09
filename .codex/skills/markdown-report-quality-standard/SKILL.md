---
name: markdown-report-quality-standard
description: "Use for `.work` reports, plans, mission reports and final consolidations."
category: codex-workers
---

# markdown-report-quality-standard

## When to use
Any agent writes Markdown as a work artifact.

## Purpose
Use for `.work` reports, plans, mission reports and final consolidations.

## Process
- Use clear headings and tables where useful.
- Include objective, scope, evidence, decisions, TODOs, dependencies, validation, risks, and next actions.
- Use American English for code/file identifiers; French narrative is acceptable for user-facing summaries if requested.
- Be precise and factual.

## Expected output
- High-detail Markdown report suited for handoff.

## Guardrails
- No vague summaries. No missing evidence.

## Andromeda baseline
- Rust 1.95.0 / Edition 2024 / resolver 3.
- QUIC + custom Protobuf RPC; no gRPC and no JSON native protocol.
- Procedure-only application surface and SRPL contracts.
- WAL before visible commit; recovery and audit are mission-critical.
- Worker artifacts belong under `.work/codex/<task-slug>/`.
