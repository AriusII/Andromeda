---
name: write-worker-mission-report-contract
description: "Use by write workers that must execute a bounded change and produce one mission report."
category: codex-workers
---

# write-worker-mission-report-contract

## When to use
A write worker receives a scope from an orchestrator.

## Purpose
Use by write workers that must execute a bounded change and produce one mission report.

## Process
- Read the orchestrator handoff and the linked analysis report.
- Edit only authorized paths.
- Record every changed file and why it changed.
- Run or document validation commands.
- Produce one mission report under `.work/codex/<task-slug>/missions/`.

## Expected output
- Mission report with scope, files changed, changes, validation, blockers, risks, next tasks.

## Guardrails
- No scope expansion. No hidden edits. No missing mission report.

## Andromeda baseline
- Rust 1.95.0 / Edition 2024 / resolver 3.
- QUIC + custom Protobuf RPC; no gRPC and no JSON native protocol.
- Procedure-only application surface and SRPL contracts.
- WAL before visible commit; recovery and audit are mission-critical.
- Worker artifacts belong under `.work/codex/<task-slug>/`.
