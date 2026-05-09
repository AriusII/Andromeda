---
name: work-directory-output-contract
description: "Use when workers or orchestrators need to write intermediate Codex task artifacts."
category: codex-workers
---

# work-directory-output-contract

## When to use
Any worker needs to create a plan, analysis report, mission report, or final consolidation.

## Purpose
Use when workers or orchestrators need to write intermediate Codex task artifacts.

## Process
- Use `.work/codex/<task-slug>/analysis/` for read-only worker reports.
- Use `.work/codex/<task-slug>/plans/` for orchestrator plans.
- Use `.work/codex/<task-slug>/missions/` for write worker mission reports.
- Use `.work/codex/<task-slug>/final/` for final consolidation.
- Use stable kebab-case file names.

## Expected output
- Correct `.work` path and one-report-per-worker discipline.

## Guardrails
- Do not write worker artifacts into docs, source, or root unless explicitly requested.

## Andromeda baseline
- Rust 1.95.0 / Edition 2024 / resolver 3.
- QUIC + custom Protobuf RPC; no gRPC and no JSON native protocol.
- Procedure-only application surface and SRPL contracts.
- WAL before visible commit; recovery and audit are mission-critical.
- Worker artifacts belong under `.work/codex/<task-slug>/`.
